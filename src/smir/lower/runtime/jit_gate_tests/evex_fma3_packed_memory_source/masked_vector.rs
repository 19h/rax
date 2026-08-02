//! Exact per-active-lane replay coverage for writemasked packed EVEX FMA3
//! full-vector memory sources.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::types::{MemWidth, OpWidth, SignExtend, SrcOperand};
use crate::smir::lower::runtime::X86JitEvexPackedFma3MemorySequence;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    F16,
    F32,
    F64,
}

impl Format {
    const ALL: [Self; 3] = [Self::F16, Self::F32, Self::F64];

    const fn elem(self) -> VecElementType {
        match self {
            Self::F16 => VecElementType::F16,
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn map(self) -> u8 {
        match self {
            Self::F16 => 6,
            Self::F32 | Self::F64 => 2,
        }
    }

    const fn vec_map(self) -> X86VecMap {
        match self {
            Self::F16 => X86VecMap::Map6,
            Self::F32 | Self::F64 => X86VecMap::Map0F38,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::F64)
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::F16 => MemWidth::B2,
            Self::F32 => MemWidth::B4,
            Self::F64 => MemWidth::B8,
        }
    }

    const fn one(self) -> u64 {
        match self {
            Self::F16 => 0x3C00,
            Self::F32 => 0x3F80_0000,
            Self::F64 => 0x3FF0_0000_0000_0000,
        }
    }

    const fn two(self) -> u64 {
        match self {
            Self::F16 => 0x4000,
            Self::F32 => 0x4000_0000,
            Self::F64 => 0x4000_0000_0000_0000,
        }
    }

    const fn three(self) -> u64 {
        match self {
            Self::F16 => 0x4200,
            Self::F32 => 0x4040_0000,
            Self::F64 => 0x4008_0000_0000_0000,
        }
    }

    const fn five(self) -> u64 {
        match self {
            Self::F16 => 0x4500,
            Self::F32 => 0x40A0_0000,
            Self::F64 => 0x4014_0000_0000_0000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaskedVectorCase {
    opcode: u8,
    format: Format,
    width: VecWidth,
    destination: u8,
    source1: u8,
    mask: u8,
    zeroing: bool,
}

impl MaskedVectorCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn lanes(self) -> u8 {
        self.width.lanes(self.format.elem()) as u8
    }

    const fn kind(self) -> X86FmaKind {
        match self.opcode & 0x0F {
            0x06 => X86FmaKind::AddSub,
            0x07 => X86FmaKind::SubAdd,
            0x08 => X86FmaKind::Add,
            0x0A => X86FmaKind::Sub,
            0x0C => X86FmaKind::NegativeMultiplyAdd,
            0x0E => X86FmaKind::NegativeMultiplySub,
            _ => unreachable!(),
        }
    }

    const fn order(self) -> X86FmaOrder {
        match self.opcode >> 4 {
            0x09 => X86FmaOrder::Order132,
            0x0A => X86FmaOrder::Order213,
            0x0B => X86FmaOrder::Order231,
            _ => unreachable!(),
        }
    }

    fn p0(self) -> u8 {
        (if self.destination & 8 == 0 { 0x80 } else { 0 })
            | 0x60
            | (if self.destination & 16 == 0 { 0x10 } else { 0 })
            | self.format.map()
    }

    fn p1(self) -> u8 {
        (u8::from(self.format.w()) << 7) | (((!self.source1) & 0x0F) << 3) | 0x05
    }

    fn p2(self) -> u8 {
        (u8::from(self.zeroing) << 7)
            | (self.ll() << 5)
            | (if self.source1 & 16 == 0 { 0x08 } else { 0 })
            | self.mask
    }

    fn bytes(self) -> [u8; 6] {
        [
            0x62,
            self.p0(),
            self.p1(),
            self.p2(),
            self.opcode,
            ((self.destination & 7) << 3) | 0x02,
        ]
    }

    fn stack_instruction(self) -> [u8; 7] {
        [
            0x62,
            (self.p0() & 0x97) | 0x60,
            self.p1() | 0x04,
            self.p2(),
            self.opcode,
            ((self.destination & 7) << 3) | 0x04,
            0x24,
        ]
    }
}

fn fill_vector(format: Format, bits: u64) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    let lane_bytes = format.memory_width().bytes() as usize;
    let encoded = bits.to_le_bytes();
    for lane in bytes.chunks_exact_mut(lane_bytes) {
        lane.copy_from_slice(&encoded[..lane_bytes]);
    }
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn lift_case(case: MaskedVectorCase) -> SmirFunction {
    let bytes = case.bytes();
    lift_bytes(case, &bytes)
}

fn lift_bytes(case: MaskedVectorCase, bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("masked packed EVEX FMA3 provenance"),
    );
    function
}

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexPackedFma3MemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first(),
        Some(SmirOp {
            kind: OpKind::X86RequireApx,
            ..
        })
    ));
    x86_jit_evex_packed_fma3_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: MaskedVectorCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512fp16,
        case.format == Format::F16,
        "{case:?}"
    );
    assert!(!requirements.needs_fma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.format != Format::F16 || std::is_x86_feature_detected!("avx512fp16"))
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(
        !x86_native_vector_features_supported_excluding(function, &excluded),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: masked packed EVEX FMA3 lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize masked packed EVEX FMA3"),
        result.entry_offset,
    )
}

fn graph_cases() -> Vec<MaskedVectorCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for format in Format::ALL {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for (destination, source1) in [(0, 0), (0, 1), (17, 17), (31, 30)] {
                    for zeroing in [false, true] {
                        cases.push(MaskedVectorCase {
                            opcode,
                            format,
                            width,
                            destination,
                            source1,
                            mask: 1 + opcode % 7,
                            zeroing,
                        });
                    }
                }
            }
        }
    }
    cases
}

fn lowering_cases() -> Vec<MaskedVectorCase> {
    let mut cases = Vec::new();
    for opcode in PACKED_OPCODES {
        for format in Format::ALL {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for zeroing in [false, true] {
                    cases.push(MaskedVectorCase {
                        opcode,
                        format,
                        width,
                        destination: 0,
                        source1: 1,
                        mask: 7,
                        zeroing,
                    });
                }
            }
        }
    }
    cases
}

fn assert_exact_graph(function: &SmirFunction, case: MaskedVectorCase) {
    let ops = &function.blocks[0].ops;
    let lanes = case.lanes() as usize;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(case.mask)));
    assert!(ops.iter().all(|op| op.guest_pc == PC), "{case:?}");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .count(),
        lanes,
        "{case:?}: one conditional memory read per source lane"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. })),
        "{case:?}: masked vector source must not contain an eager load"
    );
    assert_eq!(
        ops.iter()
            .filter(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Virtual(_),
                        ..
                    }
                )
            })
            .count(),
        lanes,
        "{case:?}: one source reconstruction insert per lane"
    );
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op.kind, OpKind::Select { .. }))
            .count(),
        lanes,
        "{case:?}: one destination merge/zero selection per lane"
    );

    let lea = ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::Lea { .. }))
        .expect("masked packed FMA3 memory address");
    assert!(
        matches!(
            lea.kind,
            OpKind::Lea {
                dst: VReg::Virtual(_),
                ..
            }
        ),
        "{case:?}: {:?}",
        lea.kind
    );
    let fma = ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86Fma(_) | OpKind::X86FP16Fma { .. }))
        .expect("masked packed FMA3 operation");
    match (&fma.kind, case.format) {
        (OpKind::X86Fma(op), Format::F32 | Format::F64) => {
            assert_eq!(op.mask, Some(mask), "{case:?}");
            assert_eq!(op.elem, case.format.elem(), "{case:?}");
            assert_eq!(op.kind, case.kind(), "{case:?}");
            assert_eq!(op.order, case.order(), "{case:?}");
            assert_eq!(op.round, FpRoundMode::Dynamic, "{case:?}");
            assert_eq!(op.lanes, case.lanes(), "{case:?}");
        }
        (
            OpKind::X86FP16Fma {
                mask: actual_mask,
                kind,
                order,
                round,
                lanes: actual_lanes,
                ..
            },
            Format::F16,
        ) => {
            assert_eq!(*actual_mask, Some(mask), "{case:?}");
            assert_eq!(*kind, case.kind(), "{case:?}");
            assert_eq!(*order, case.order(), "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert_eq!(*actual_lanes, case.lanes(), "{case:?}");
        }
        (other, _) => panic!("{case:?}: FMA operation {other:?}"),
    }
    assert_eq!(
        fma.x86_hint,
        Some(X86OpHint::EvexOp {
            map: case.format.vec_map(),
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: case.width,
            w: case.format.w(),
        }),
        "{case:?}"
    );
}

#[test]
fn masked_vector_classifier_exhaustively_rewrites_9_289_728_control_and_apx_address_cells() {
    let mut accepted = 0usize;
    for opcode in PACKED_OPCODES {
        for format in Format::ALL {
            for (ll, width) in [
                (0, VecWidth::V128),
                (1, VecWidth::V256),
                (2, VecWidth::V512),
            ] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for mask in 1..=7u8 {
                            for zeroing in [false, true] {
                                let case = MaskedVectorCase {
                                    opcode,
                                    format,
                                    width,
                                    destination,
                                    source1,
                                    mask,
                                    zeroing,
                                };
                                for (base_high, index_high) in
                                    [(false, false), (false, true), (true, false), (true, true)]
                                {
                                    let mut bytes = case.bytes();
                                    bytes[1] |= u8::from(base_high) << 3;
                                    if index_high {
                                        bytes[2] &= !0x04;
                                    }
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_packed_fma3_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.width, width, "{bytes:02X?}");
                                    assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                    assert_eq!(encoding.writemask, Some(mask), "{bytes:02X?}");
                                    assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                    assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                                    assert_eq!(encoding.w, format.w(), "{bytes:02X?}");
                                    assert_eq!(encoding.needs_avx512vl, ll != 2, "{bytes:02X?}");
                                    let X86EvexPackedFma3MemoryReplay::MaskedVector {
                                        stack_instruction,
                                    } = encoding.replay
                                    else {
                                        panic!("{bytes:02X?}: masked vector selected wrong replay");
                                    };
                                    assert_eq!(
                                        stack_instruction.as_slice(),
                                        case.stack_instruction(),
                                        "{bytes:02X?}"
                                    );
                                    accepted += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 18 * 3 * 3 * 32 * 32 * 7 * 2 * 4);
}

#[test]
fn masked_vector_rewrite_matches_independent_llvm_23_encodings() {
    let cases = [
        (
            MaskedVectorCase {
                opcode: 0x98,
                format: Format::F64,
                width: VecWidth::V128,
                destination: 0,
                source1: 0,
                mask: 1,
                zeroing: false,
            },
            &[0x62, 0xF2, 0xFD, 0x09, 0x98, 0x04, 0x24][..],
        ),
        (
            MaskedVectorCase {
                opcode: 0x98,
                format: Format::F32,
                width: VecWidth::V512,
                destination: 0,
                source1: 0,
                mask: 1,
                zeroing: true,
            },
            &[0x62, 0xF2, 0x7D, 0xC9, 0x98, 0x04, 0x24][..],
        ),
        (
            MaskedVectorCase {
                opcode: 0x98,
                format: Format::F16,
                width: VecWidth::V512,
                destination: 0,
                source1: 0,
                mask: 1,
                zeroing: true,
            },
            &[0x62, 0xF6, 0x7D, 0xC9, 0x98, 0x04, 0x24][..],
        ),
    ];
    for (case, llvm) in cases {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_packed_fma3_memory_encoding()
            .unwrap();
        let X86EvexPackedFma3MemoryReplay::MaskedVector { stack_instruction } = encoding.replay
        else {
            panic!("{case:?}: masked vector selected wrong replay");
        };
        assert_eq!(stack_instruction.as_slice(), llvm, "{case:?}");
        assert_eq!(case.stack_instruction(), llvm, "{case:?}");
    }
}

#[test]
fn all_1_296_opcode_format_width_alias_and_mask_shapes_lift_optimize_and_admit() {
    let cases = graph_cases();
    assert_eq!(cases.len(), 18 * 3 * 3 * 4 * 2);
    let mut admissions = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let sequence = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(
                sequence.consumed
                    + usize::from(matches!(
                        function.blocks[0].ops.first(),
                        Some(SmirOp {
                            kind: OpKind::X86RequireApx,
                            ..
                        })
                    )),
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.memory_offset, 2, "{level:?} {case:?}");
            assert_eq!(
                sequence.memory_size,
                case.width.bytes(),
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_eq!(sequence.encoding.writemask, Some(case.mask));
            assert_eq!(sequence.encoding.zeroing, case.zeroing);
            admissions += 1;
        }
    }
    assert_eq!(admissions, 1_296 * LEVELS.len());
}

#[test]
fn all_324_opcode_format_width_and_mask_modes_lower_exactly() {
    let cases = lowering_cases();
    assert_eq!(cases.len(), 18 * 3 * 3 * 2);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, _) = lower(&function, case);
            let expected = case.stack_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 324 * LEVELS.len());
}

#[test]
fn all_252_segment_addr32_rip_and_apx_address_cells_admit_and_lower() {
    let mut cells = 0usize;
    for w in [false, true] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in MemoryForm::ALL {
                for zeroing in [false, true] {
                    let memory_case = FmaMemoryCase {
                        opcode: 0x98,
                        w,
                        width,
                        form,
                    };
                    let case = MaskedVectorCase {
                        opcode: memory_case.opcode,
                        format: if w { Format::F64 } else { Format::F32 },
                        width,
                        destination: memory_case.destination(),
                        source1: memory_case.source1(),
                        mask: 5,
                        zeroing,
                    };
                    let mut bytes = memory_case.bytes();
                    let evex = usize::from(form == MemoryForm::FsAddr32Sib) * 2;
                    bytes[evex + 3] |= case.mask | (u8::from(zeroing) << 7);
                    for level in LEVELS {
                        let function = optimize(lift_bytes(case, &bytes), level);
                        assert_exact_graph(&function, case);
                        let sequence = sequence(&function, true)
                            .unwrap_or_else(|| panic!("{level:?} {form:?} {case:?}"));
                        assert_eq!(
                            sequence.consumed
                                + usize::from(matches!(
                                    function.blocks[0].ops.first(),
                                    Some(SmirOp {
                                        kind: OpKind::X86RequireApx,
                                        ..
                                    })
                                )),
                            function.blocks[0].ops.len(),
                            "{level:?} {form:?} {case:?}"
                        );
                        let (code, _) = lower(&function, case);
                        let expected = case.stack_instruction();
                        assert!(
                            code.windows(expected.len())
                                .any(|window| window == expected),
                            "{level:?} {form:?} {case:?}: missing {expected:02X?}"
                        );
                        cells += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cells, 2 * 3 * 7 * 2 * LEVELS.len());
}

#[test]
fn masked_vector_lowering_stages_disjoint_16_32_and_64_bit_lanes() {
    for (format, staging_load, final_store) in [
        (
            Format::F16,
            &[0x66, 0x8B, 0x44, 0x24, 0x48][..],
            &[0x66, 0x89, 0x44, 0x24, 0x46][..],
        ),
        (
            Format::F32,
            &[0x8B, 0x44, 0x24, 0x48][..],
            &[0x89, 0x44, 0x24, 0x44][..],
        ),
        (
            Format::F64,
            &[0x48, 0x8B, 0x44, 0x24, 0x48][..],
            &[0x48, 0x89, 0x44, 0x24, 0x40][..],
        ),
    ] {
        let case = MaskedVectorCase {
            opcode: 0x9E,
            format,
            width: VecWidth::V512,
            destination: 17,
            source1: 17,
            mask: 7,
            zeroing: true,
        };
        let function = optimize(lift_case(case), OptLevel::O2);
        let (code, _) = lower(&function, case);
        for lane in 0..case.lanes() {
            let lane_mask = (1u32 << lane).to_le_bytes();
            let guard = [
                0x9C,
                0x50,
                0xC4,
                0xE1,
                0xFB,
                0x93,
                0xC0 | case.mask,
                0xF7,
                0xC0,
                lane_mask[0],
                lane_mask[1],
                lane_mask[2],
                lane_mask[3],
                0x0F,
                0x84,
            ];
            assert!(
                code.windows(guard.len()).any(|window| window == guard),
                "{format:?} lane {lane}: {guard:02X?}"
            );
        }
        let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
        let release_frame = [0x48, 0x8D, 0x64, 0x24, 0x50];
        assert_eq!(
            code.windows(allocate_frame.len())
                .filter(|window| *window == allocate_frame)
                .count(),
            1,
            "{format:?}"
        );
        assert_eq!(
            code.windows(release_frame.len())
                .filter(|window| *window == release_frame)
                .count(),
            usize::from(case.lanes()) + 1,
            "{format:?}"
        );
        assert!(
            code.windows(staging_load.len())
                .any(|window| window == staging_load),
            "{format:?}: scalar result must use the disjoint staging slot"
        );
        assert!(
            code.windows(final_store.len())
                .any(|window| window == final_store),
            "{format:?}: final active lane must end at payload byte 63"
        );

        let mut avx_only = X86_64Lowerer::new();
        avx_only.set_mem_helpers(true);
        avx_only.set_preserve_vector_mem_helpers(true);
        avx_only.set_avx_ymm16_vector_state(true);
        let error = avx_only
            .lower_function(&function)
            .expect_err("AVX-only state bridge must reject AVX-512 FMA3 replay");
        assert!(
            format!("{error:?}").contains("AVX-only vector bridge"),
            "{error:?}"
        );
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: sequence classifier admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted malformed graph"
    );
}

#[test]
fn masked_vector_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let case = MaskedVectorCase {
        opcode: 0x98,
        format: Format::F64,
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        mask: 3,
        zeroing: false,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&base, false).is_none(), "memory-disabled gate");

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();
    assert_rejected("missing provenance", &missing_provenance);

    let mut wrong_provenance = base.clone();
    let mut bytes = case.bytes();
    bytes[4] = 0xA8;
    wrong_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert_rejected("wrong opcode provenance", &wrong_provenance);

    let mut wrong_seed = base.clone();
    let OpKind::Mov { src, .. } = &mut wrong_seed.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *src = SrcOperand::Imm(1);
    assert_rejected("nonzero vector seed", &wrong_seed);

    let mut hinted_address = base.clone();
    hinted_address.blocks[0].ops[2].x86_hint = Some(X86OpHint::MovImmModRm);
    assert_rejected("hinted address", &hinted_address);

    let first_load = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let mut wrong_width = base.clone();
    let OpKind::PredLoad { width, .. } = &mut wrong_width.blocks[0].ops[first_load].kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    assert_rejected("wrong lane width", &wrong_width);

    let mut wrong_condition = base.clone();
    let OpKind::PredLoad { cond, .. } = &mut wrong_condition.blocks[0].ops[first_load].kind else {
        unreachable!()
    };
    *cond = VReg::Arch(ArchReg::X86(X86Reg::K(case.mask)));
    assert_rejected("wrong load predicate", &wrong_condition);

    let first_insert = base.blocks[0]
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Virtual(_),
                    ..
                }
            )
        })
        .unwrap();
    let mut wrong_lane = base.clone();
    let OpKind::VInsertLane { lane, .. } = &mut wrong_lane.blocks[0].ops[first_insert].kind else {
        unreachable!()
    };
    *lane = 1;
    assert_rejected("wrong source lane", &wrong_lane);

    let fma_index = base.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86Fma(_)))
        .unwrap();
    let mut wrong_round = base.clone();
    let OpKind::X86Fma(op) = &mut wrong_round.blocks[0].ops[fma_index].kind else {
        unreachable!()
    };
    op.round = FpRoundMode::RoundNearest;
    assert_rejected("wrong rounding", &wrong_round);

    let mut wrong_mask = base.clone();
    let OpKind::X86Fma(op) = &mut wrong_mask.blocks[0].ops[fma_index].kind else {
        unreachable!()
    };
    op.mask = None;
    assert_rejected("wrong FMA mask", &wrong_mask);

    let mut wrong_order = base.clone();
    let OpKind::X86Fma(op) = &mut wrong_order.blocks[0].ops[fma_index].kind else {
        unreachable!()
    };
    op.order = X86FmaOrder::Order231;
    assert_rejected("wrong FMA order", &wrong_order);

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFF)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    assert_rejected("same-PC tail", &same_pc_tail);
}

fn initial_registers(case: MaskedVectorCase, mask: u64) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| 0x1000 + index as u64 * 0x101),
        zmm: std::array::from_fn(|index| {
            fill_vector(
                case.format,
                if index & 1 == 0 {
                    case.format.one()
                } else {
                    case.format.two()
                },
            )
        }),
        k: std::array::from_fn(|index| {
            if index == usize::from(case.mask) {
                mask
            } else {
                0
            }
        }),
        rflags: 0x8D7,
        mxcsr: 0x1F80,
        vector_active: X86_VECTOR_STATE_K64,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        ..GuestRegs::default()
    };
    registers.gpr[2] = 0x2000;
    registers.zmm[usize::from(case.destination)] = fill_vector(case.format, case.format.one());
    if case.source1 != case.destination {
        registers.zmm[usize::from(case.source1)] = fill_vector(case.format, case.format.two());
    }
    registers
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    case: MaskedVectorCase,
) -> GuestRegs {
    let mut expected = super::super::vex_fma3_memory_source::interpreter_success(
        function, initial, source, 0x2000, case.width,
    );
    expected.vector_scratch = initial.vector_scratch;
    expected
}

#[test]
fn masked_vector_interpretation_is_o0_o1_o2_equivalent_for_all_opcode_format_width_modes() {
    let cases = lowering_cases();
    let mut comparisons = 0usize;
    for case in cases {
        let lane_mask = (1u64 << case.lanes()) - 1;
        let initial = initial_registers(case, 0xA5A5_A5A5 & lane_mask);
        let source = fill_vector(case.format, case.format.three());
        let expected = interpreter_success(&lift_case(case), &initial, source, case);
        for level in LEVELS {
            let actual =
                interpreter_success(&optimize(lift_case(case), level), &initial, source, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 324 * LEVELS.len());
}

#[test]
fn masked_vector_add132_manual_merge_zero_and_upper_lane_results_are_exact() {
    for format in Format::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for zeroing in [false, true] {
                let case = MaskedVectorCase {
                    opcode: 0x98,
                    format,
                    width,
                    destination: 0,
                    source1: 1,
                    mask: 1,
                    zeroing,
                };
                let lane_mask = (1u64 << case.lanes()) - 1;
                let active = 0xA5A5_A5A5 & lane_mask;
                let initial = initial_registers(case, active);
                let actual = interpreter_success(
                    &lift_case(case),
                    &initial,
                    fill_vector(format, format.three()),
                    case,
                );
                let mut expected_bytes = [0u8; 64];
                let lane_bytes = format.memory_width().bytes() as usize;
                for lane in 0..usize::from(case.lanes()) {
                    let bits = if active & (1 << lane) != 0 {
                        format.five()
                    } else if zeroing {
                        0
                    } else {
                        format.one()
                    };
                    expected_bytes[lane * lane_bytes..(lane + 1) * lane_bytes]
                        .copy_from_slice(&bits.to_le_bytes()[..lane_bytes]);
                }
                let expected = std::array::from_fn(|word| {
                    u64::from_le_bytes(expected_bytes[word * 8..word * 8 + 8].try_into().unwrap())
                });
                assert_eq!(actual.zmm[0], expected, "{case:?}");
                assert_eq!(actual.mxcsr, initial.mxcsr, "{case:?}");
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
struct LaneMemoryContext {
    base: u64,
    value: [u8; 64],
    lane_bytes: usize,
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 32],
}

#[cfg(target_arch = "x86_64")]
extern "C" fn lane_load_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size as usize, context.lane_bytes);
    assert_eq!(signed, 0);
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + context.lane_bytes <= context.value.len());
    LoadResult {
        value: match context.lane_bytes {
            2 => u16::from_le_bytes(context.value[offset..offset + 2].try_into().unwrap()) as u64,
            4 => u32::from_le_bytes(context.value[offset..offset + 4].try_into().unwrap()) as u64,
            8 => u64::from_le_bytes(context.value[offset..offset + 8].try_into().unwrap()),
            _ => unreachable!("packed FMA3 element helper width"),
        },
        ok: 1,
    }
}

#[cfg(target_arch = "x86_64")]
fn vector_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_masked_vector_matches_interpretation_faults_without_commit_and_suppresses_lanes() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native masked packed EVEX FMA3: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let selected = [
        MaskedVectorCase {
            opcode: 0x98,
            format: Format::F32,
            width: VecWidth::V128,
            destination: 0,
            source1: 1,
            mask: 1,
            zeroing: false,
        },
        MaskedVectorCase {
            opcode: 0xAE,
            format: Format::F64,
            width: VecWidth::V256,
            destination: 17,
            source1: 17,
            mask: 7,
            zeroing: true,
        },
        MaskedVectorCase {
            opcode: 0xB7,
            format: Format::F32,
            width: VecWidth::V512,
            destination: 31,
            source1: 30,
            mask: 3,
            zeroing: false,
        },
        MaskedVectorCase {
            opcode: 0x9C,
            format: Format::F16,
            width: VecWidth::V512,
            destination: 0,
            source1: 1,
            mask: 1,
            zeroing: true,
        },
    ];
    let cases: Vec<_> = selected
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .filter(|case| case.format != Format::F16 || has_fp16)
        .collect();
    assert!(!cases.is_empty());
    let expected_successes = 2 * cases.len();

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for case in cases {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = fill_vector(case.format, case.format.three());
            let source_bytes = vector_bytes(source);
            let lane_bytes = case.format.memory_width().bytes() as usize;
            let active = 0b1101u64;

            let mut registers = initial_registers(case, active);
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: source_bytes,
                lane_bytes,
                fail_address: None,
                calls: 0,
                addresses: [0; 32],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, case);
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            let expected_addresses: Vec<_> = (0..case.lanes())
                .filter(|lane| active & (1 << lane) != 0)
                .map(|lane| 0x2000 + u64::from(lane) * lane_bytes as u64)
                .collect();
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: active addresses"
            );
            successes += 1;

            let fail_address = 0x2000 + 2 * lane_bytes as u64;
            let mut registers = initial_registers(case, active);
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: source_bytes,
                lane_bytes,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 32],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(
                context.addresses[context.calls - 1],
                fail_address,
                "{level:?} {case:?}: fault address"
            );
            faults += 1;

            let mut registers = initial_registers(case, 0);
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: source_bytes,
                lane_bytes,
                fail_address: Some(0x2000),
                calls: 0,
                addresses: [0; 32],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, case);
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected,
                "{level:?} {case:?}: all lanes suppressed"
            );
            assert_eq!(context.calls, 0, "{level:?} {case:?}");
            suppressions += 1;
        }
    }
    assert_eq!(successes, faults);
    assert_eq!(successes, suppressions);
    assert_eq!(successes, expected_successes);
}
