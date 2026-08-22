use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::{OpId, SignExtend};
use crate::smir::lower::runtime::{
    X86_VECTOR_STATE_K16, X86_VECTOR_STATE_K64, x86_native_vector_uses_k16_opmasks_excluding,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApproxOperation {
    Recip14,
    Rsqrt14,
    RecipFp16,
    RsqrtFp16,
    Recip28,
    Rsqrt28,
}

impl ApproxOperation {
    const ALL: [Self; 6] = [
        Self::Recip14,
        Self::Rsqrt14,
        Self::RecipFp16,
        Self::RsqrtFp16,
        Self::Recip28,
        Self::Rsqrt28,
    ];

    const fn kind(self) -> X86EvexScalarFpUnaryMemoryKind {
        match self {
            Self::Recip14 => X86EvexScalarFpUnaryMemoryKind::Recip14,
            Self::Rsqrt14 => X86EvexScalarFpUnaryMemoryKind::Rsqrt14,
            Self::RecipFp16 => X86EvexScalarFpUnaryMemoryKind::RecipFp16,
            Self::RsqrtFp16 => X86EvexScalarFpUnaryMemoryKind::RsqrtFp16,
            Self::Recip28 => X86EvexScalarFpUnaryMemoryKind::Recip28,
            Self::Rsqrt28 => X86EvexScalarFpUnaryMemoryKind::Rsqrt28,
        }
    }

    const fn formats(self) -> &'static [ScalarFormat] {
        match self {
            Self::RecipFp16 | Self::RsqrtFp16 => &[ScalarFormat::F16],
            _ => &[ScalarFormat::F32, ScalarFormat::F64],
        }
    }

    const fn map(self) -> u8 {
        match self {
            Self::RecipFp16 | Self::RsqrtFp16 => 6,
            _ => 2,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::Recip14 | Self::RecipFp16 => 0x4D,
            Self::Rsqrt14 | Self::RsqrtFp16 => 0x4F,
            Self::Recip28 => 0xCB,
            Self::Rsqrt28 => 0xCD,
        }
    }

    const fn needs_er(self) -> bool {
        matches!(self, Self::Recip28 | Self::Rsqrt28)
    }

    const fn uses_k16_opmasks(self) -> bool {
        !matches!(self, Self::RecipFp16 | Self::RsqrtFp16)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ApproxMemoryCase {
    operation: ApproxOperation,
    format: ScalarFormat,
    destination: u8,
    merge: u8,
    ll: u8,
    control: MaskControl,
}

impl ApproxMemoryCase {
    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    fn bytes(self) -> Vec<u8> {
        approx_memory_encoding(
            self.operation,
            self.format,
            self.destination,
            self.merge,
            self.ll,
            self.mask(),
            self.zeroing(),
            3,
        )
    }

    fn stack_instruction(self) -> Vec<u8> {
        approx_stack_encoding(
            self.operation,
            self.format,
            self.destination,
            self.merge,
            self.ll,
            self.mask(),
            self.zeroing(),
        )
    }

    const fn bridge_case(self) -> ScalarUnaryMemoryCase {
        ScalarUnaryMemoryCase {
            operation: UnaryOperation::GetExponent,
            format: self.format,
            destination: self.destination,
            merge: self.merge,
            ll: self.ll,
            control: self.control,
            immediate: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn approx_memory_encoding(
    operation: ApproxOperation,
    format: ScalarFormat,
    destination: u8,
    merge: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    base: u8,
) -> Vec<u8> {
    assert!(operation.formats().contains(&format));
    assert!(destination < 32 && merge < 32 && base < 16);
    assert!(ll < 3 && mask < 8 && (!zeroing || mask != 0));
    vec![
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | 0x40
            | (if base & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | operation.map(),
        (u8::from(format.w()) << 7) | (((!merge) & 0x0F) << 3) | 0x05,
        (u8::from(zeroing) << 7) | (ll << 5) | (if merge & 16 == 0 { 0x08 } else { 0 }) | mask,
        operation.opcode(),
        ((destination & 7) << 3) | (base & 7),
    ]
}

#[allow(clippy::too_many_arguments)]
fn approx_stack_encoding(
    operation: ApproxOperation,
    format: ScalarFormat,
    destination: u8,
    merge: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
) -> Vec<u8> {
    let mut bytes =
        approx_memory_encoding(operation, format, destination, merge, ll, mask, zeroing, 4);
    bytes.insert(6, 0x24);
    bytes
}

fn all_approx_cases() -> Vec<ApproxMemoryCase> {
    let mut cases = Vec::new();
    for operation in ApproxOperation::ALL {
        for &format in operation.formats() {
            for (destination, merge) in [(0, 1), (17, 17), (31, 30)] {
                for ll in 0..=2 {
                    for control in MaskControl::ALL {
                        cases.push(ApproxMemoryCase {
                            operation,
                            format,
                            destination,
                            merge,
                            ll,
                            control,
                        });
                    }
                }
            }
        }
    }
    cases
}

fn lift_approx(case: ApproxMemoryCase) -> SmirFunction {
    function_from_bytes(&case.bytes(), case)
}

fn exact_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexScalarFpUnaryMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    (0..function.blocks[0].ops.len()).find_map(|index| {
        x86_jit_evex_scalar_fp_unary_memory_sequence(
            &function.blocks[0],
            index,
            allow_mem,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
    })
}

fn lower_approx(function: &SmirFunction, case: ApproxMemoryCase) -> (Vec<u8>, usize) {
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
    assert_eq!(
        x86_native_vector_uses_k16_opmasks_excluding(function, &excluded),
        case.operation.uses_k16_opmasks(),
        "{case:?}"
    );

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx512bw,
        !case.operation.uses_k16_opmasks(),
        "{case:?}"
    );
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert_eq!(
        requirements.needs_avx512er,
        case.operation.needs_er(),
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512fp16,
        case.format == ScalarFormat::F16,
        "{case:?}"
    );
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.operation.uses_k16_opmasks(),
        "{case:?}"
    );
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && (case.operation.uses_k16_opmasks() || std::is_x86_feature_detected!("avx512bw"))
            && (!case.operation.needs_er() || crate::smir::lower::runtime::x86_host_has_avx512er())
            && (case.format != ScalarFormat::F16 || std::is_x86_feature_detected!("avx512fp16")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_narrow_vector_opmask_helpers(case.operation.uses_k16_opmasks());
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed approximation: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed scalar approximation"),
        result.entry_offset,
    )
}

#[test]
fn scalar_approx_classifier_exhaustively_rewrites_1_843_200_control_and_apx_cells() {
    let mut accepted = 0usize;
    for operation in ApproxOperation::ALL {
        for &format in operation.formats() {
            for ll in 0..=2u8 {
                for destination in 0..32u8 {
                    for merge in 0..32u8 {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let canonical = approx_memory_encoding(
                                    operation,
                                    format,
                                    destination,
                                    merge,
                                    ll,
                                    mask,
                                    zeroing,
                                    3,
                                );
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical.clone();
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_scalar_fp_unary_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.kind, operation.kind(), "{bytes:02X?}");
                                        assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.destination, destination,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.merge, merge, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.writemask,
                                            (mask != 0).then_some(mask),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                        assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                        assert_eq!(encoding.immediate, None, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.memory_width,
                                            format.memory_width(),
                                            "{bytes:02X?}"
                                        );
                                        assert!(!encoding.needs_avx512dq, "{bytes:02X?}");
                                        assert_eq!(
                                            encoding.needs_avx512er,
                                            operation.needs_er(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512fp16,
                                            format == ScalarFormat::F16,
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.stack_instruction.as_slice(),
                                            approx_stack_encoding(
                                                operation,
                                                format,
                                                destination,
                                                merge,
                                                ll,
                                                mask,
                                                zeroing,
                                            ),
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
    }
    assert_eq!(accepted, 10 * 3 * 32 * 32 * 15 * 2 * 2);
}

#[test]
fn scalar_approx_stack_encodings_match_ten_independent_llvm_23_anchors() {
    for (actual, llvm) in [
        (
            approx_stack_encoding(
                ApproxOperation::Recip14,
                ScalarFormat::F64,
                16,
                1,
                0,
                0,
                false,
            ),
            &[0x62, 0xE2, 0xF5, 0x08, 0x4D, 0x04, 0x24][..],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Recip14,
                ScalarFormat::F32,
                0,
                1,
                0,
                1,
                false,
            ),
            &[0x62, 0xF2, 0x75, 0x09, 0x4D, 0x04, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Rsqrt14,
                ScalarFormat::F64,
                31,
                30,
                0,
                7,
                true,
            ),
            &[0x62, 0x62, 0x8D, 0x87, 0x4F, 0x3C, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Rsqrt14,
                ScalarFormat::F32,
                17,
                18,
                0,
                3,
                false,
            ),
            &[0x62, 0xE2, 0x6D, 0x03, 0x4F, 0x0C, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::RecipFp16,
                ScalarFormat::F16,
                16,
                1,
                0,
                0,
                false,
            ),
            &[0x62, 0xE6, 0x75, 0x08, 0x4D, 0x04, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::RsqrtFp16,
                ScalarFormat::F16,
                31,
                30,
                0,
                7,
                true,
            ),
            &[0x62, 0x66, 0x0D, 0x87, 0x4F, 0x3C, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Recip28,
                ScalarFormat::F64,
                16,
                1,
                0,
                0,
                false,
            ),
            &[0x62, 0xE2, 0xF5, 0x08, 0xCB, 0x04, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Recip28,
                ScalarFormat::F32,
                0,
                1,
                0,
                1,
                false,
            ),
            &[0x62, 0xF2, 0x75, 0x09, 0xCB, 0x04, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Rsqrt28,
                ScalarFormat::F64,
                31,
                30,
                0,
                7,
                true,
            ),
            &[0x62, 0x62, 0x8D, 0x87, 0xCD, 0x3C, 0x24],
        ),
        (
            approx_stack_encoding(
                ApproxOperation::Rsqrt28,
                ScalarFormat::F32,
                17,
                18,
                0,
                3,
                false,
            ),
            &[0x62, 0xE2, 0x6D, 0x03, 0xCD, 0x0C, 0x24],
        ),
    ] {
        assert_eq!(actual, llvm);
    }
}

#[test]
fn scalar_approx_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = approx_memory_encoding(
        ApproxOperation::Recip28,
        ScalarFormat::F64,
        17,
        30,
        2,
        3,
        false,
        3,
    );
    let mut malformed = Vec::new();
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut memory_sae = valid.clone();
    memory_sae[3] |= 0x10;
    malformed.push(memory_sae);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    for (index, mask) in [(0, 0x01), (1, 0x01), (2, 0x01), (4, 0x01)] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut half_w1 = approx_memory_encoding(
        ApproxOperation::RecipFp16,
        ScalarFormat::F16,
        0,
        1,
        0,
        0,
        false,
        3,
    );
    half_w1[2] |= 0x80;
    malformed.push(half_w1);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_fp_unary_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_270_scalar_approx_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_approx_cases();
    assert_eq!(cases.len(), 270);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_approx(case), level);
            let exact = exact_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(
                exact.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.kind, case.operation.kind(), "{case:?}");
            assert_eq!(exact.encoding.elem, case.format.elem(), "{case:?}");
            assert_eq!(exact.encoding.destination, case.destination, "{case:?}");
            assert_eq!(exact.encoding.merge, case.merge, "{case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{case:?}");
            assert_eq!(exact.encoding.immediate, None, "{case:?}");
            assert!(matches!(
                function.blocks[0].ops[exact.load_offset].kind,
                OpKind::Load { width, sign: SignExtend::Zero, .. }
                    | OpKind::PredLoad { width, signed: SignExtend::Zero, .. }
                    if width == case.format.memory_width()
            ));

            let (code, _) = lower_approx(&function, case);
            let expected = case.stack_instruction();
            assert_eq!(
                code.windows(expected.len())
                    .filter(|window| *window == expected)
                    .count(),
                1,
                "{level:?} {case:?}: {code:02X?}"
            );
            assert!(
                code.windows(5)
                    .any(|window| { window == [0xBA, case.format.memory_size() as u8, 0, 0, 0] }),
                "{level:?} {case:?}: missing exact helper width"
            );
            if case.operation.uses_k16_opmasks() {
                assert!(code.windows(2).any(|window| window == [0xC5, 0xF8]));
                assert!(!code.windows(3).any(|window| window == [0xC4, 0xE1, 0xF8]));
            } else {
                assert!(code.windows(3).any(|window| window == [0xC4, 0xE1, 0xF8]));
            }
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 270 * LEVELS.len());
}

#[test]
fn scalar_approx_sequence_fails_closed_for_semantic_provenance_and_ssa_mutations() {
    let case = ApproxMemoryCase {
        operation: ApproxOperation::Recip28,
        format: ScalarFormat::F64,
        destination: 17,
        merge: 30,
        ll: 2,
        control: MaskControl::Merge,
    };
    let exact = optimize(lift_approx(case), OptLevel::O0);
    assert!(exact_sequence(&exact, true).is_some());
    assert!(exact_sequence(&exact, false).is_none());

    let mut sae = exact.clone();
    let Some(OpKind::X86Recip28 {
        suppress_exceptions,
        ..
    }) = sae.blocks[0].ops.last_mut().map(|op| &mut op.kind)
    else {
        unreachable!()
    };
    *suppress_exceptions = true;
    assert!(exact_sequence(&sae, true).is_none());

    let mut wrong_hint = exact.clone();
    wrong_hint.blocks[0].ops.last_mut().unwrap().x86_hint = None;
    assert!(exact_sequence(&wrong_hint, true).is_none());

    let mut wrong_width = exact.clone();
    let load = wrong_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let OpKind::PredLoad { width, .. } = &mut load.kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    assert!(exact_sequence(&wrong_width, true).is_none());

    let mut wrong_provenance = exact.clone();
    let mut wrong_bytes = case.bytes();
    wrong_bytes[3] |= 0x10;
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_bytes).unwrap(),
    );
    assert!(exact_sequence(&wrong_provenance, true).is_none());

    let mut trailing = exact.clone();
    let mut duplicate = trailing.blocks[0].ops.last().unwrap().clone();
    duplicate.id = OpId(trailing.blocks[0].ops.len() as u16);
    trailing.blocks[0].ops.push(duplicate);
    assert!(exact_sequence(&trailing, true).is_none());

    let mut duplicate_definition = exact.clone();
    let loaded = match duplicate_definition.blocks[0].ops[0].kind {
        OpKind::Mov { dst, .. } => dst,
        _ => unreachable!(),
    };
    duplicate_definition.blocks[0].ops.insert(
        1,
        SmirOp::new(
            OpId(1),
            PC,
            OpKind::Mov {
                dst: loaded,
                src: crate::smir::ir::types::SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ),
    );
    assert!(exact_sequence(&duplicate_definition, true).is_none());
}

#[test]
fn classic_approx_k16_bridge_fails_closed_when_mixed_with_full_opmask_replay() {
    let classic = ApproxMemoryCase {
        operation: ApproxOperation::Recip28,
        format: ScalarFormat::F64,
        destination: 17,
        merge: 30,
        ll: 2,
        control: MaskControl::Merge,
    };
    let half = ApproxMemoryCase {
        operation: ApproxOperation::RsqrtFp16,
        format: ScalarFormat::F16,
        destination: 31,
        merge: 17,
        ll: 1,
        control: MaskControl::Zero,
    };
    let mut function = lift_approx(classic);
    let half_function = lift_approx(half);
    let mut half_block = SmirBlock::new(BlockId(1), PC);
    half_block.ops = half_function.blocks[0].ops.clone();
    half_block.set_terminator(Terminator::Return { values: Vec::new() });
    function.add_block(half_block);
    function.x86_instruction_bytes.insert(
        (BlockId(1), PC),
        X86InstructionBytes::new(&half.bytes()).unwrap(),
    );

    let requirements =
        x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
    assert!(requirements.has_k16_opmask_span);
    assert!(requirements.needs_avx512bw);
    assert!(requirements.needs_avx512er);
    assert!(requirements.needs_avx512fp16);
    assert!(!x86_native_vector_uses_k16_opmasks_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}

fn element_mask(format: ScalarFormat) -> u64 {
    match format {
        ScalarFormat::F16 => u64::from(u16::MAX),
        ScalarFormat::F32 => u64::from(u32::MAX),
        ScalarFormat::F64 => u64::MAX,
    }
}

fn inactive_destination(case: ApproxMemoryCase, initial: &GuestRegs) -> [u64; 8] {
    let mut expected = initial.zmm[usize::from(case.merge)];
    let mask = element_mask(case.format);
    let low = match case.control {
        MaskControl::Merge => initial.zmm[usize::from(case.destination)][0] & mask,
        MaskControl::Zero => 0,
        MaskControl::None => unreachable!(),
    };
    expected[0] = (expected[0] & !mask) | low;
    expected[2..].fill(0);
    expected
}

#[test]
fn all_270_scalar_approx_cells_have_o0_o1_o2_interpreter_equivalence_and_exact_masks() {
    let cases = all_approx_cases();
    assert_eq!(cases.len(), 270);
    let mut active_executions = 0usize;
    let mut inactive_executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bridge = case.bridge_case();
        let source = source_bits(case.format, ordinal);
        let active_initial = initial_registers(bridge, ordinal, true);
        let baseline = optimize(lift_approx(case), OptLevel::O0);
        let active_expected = interpreter_success(&baseline, &active_initial, source, bridge);
        assert_eq!(active_expected.rflags, active_initial.rflags, "{case:?}");
        assert_eq!(active_expected.k, active_initial.k, "{case:?}");
        assert_eq!(
            active_expected.zmm[usize::from(case.destination)][2..],
            [0; 6],
            "{case:?}: bits above XMM must be zero"
        );
        for level in LEVELS {
            let function = optimize(lift_approx(case), level);
            let active = interpreter_success(&function, &active_initial, source, bridge);
            assert_eq!(active, active_expected, "{level:?} {case:?}: active");
            active_executions += 1;
        }

        if case.control != MaskControl::None {
            let inactive_initial = initial_registers(bridge, ordinal, false);
            let inactive_expected =
                interpreter_success(&baseline, &inactive_initial, source, bridge);
            assert_eq!(
                inactive_expected.zmm[usize::from(case.destination)],
                inactive_destination(case, &inactive_initial),
                "{case:?}: inactive merge/zero and source-1 upper lanes"
            );
            assert_eq!(inactive_expected.mxcsr, inactive_initial.mxcsr, "{case:?}");
            for level in LEVELS {
                let function = optimize(lift_approx(case), level);
                let inactive = interpreter_success(&function, &inactive_initial, source, bridge);
                assert_eq!(inactive, inactive_expected, "{level:?} {case:?}: inactive");
                inactive_executions += 1;
            }
        }
    }
    assert_eq!(active_executions, 270 * LEVELS.len());
    assert_eq!(inactive_executions, 10 * 3 * 3 * 2 * LEVELS.len());
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct ScalarMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = address;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_scalar_approx_memory_matches_interpretation_faults_and_mask_suppression() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("avx512f") {
        eprintln!("skipping native scalar approximation differential: host lacks AVX/AVX-512F");
        return;
    }

    let mut executions = 0usize;
    for (ordinal, case) in all_approx_cases().into_iter().enumerate() {
        if case.operation.needs_er() && !crate::smir::lower::runtime::x86_host_has_avx512er() {
            continue;
        }
        if case.format == ScalarFormat::F16
            && (!std::is_x86_feature_detected!("avx512bw")
                || !std::is_x86_feature_detected!("avx512fp16"))
        {
            continue;
        }
        for level in [OptLevel::O0, OptLevel::O2] {
            let bridge = case.bridge_case();
            let function = optimize(lift_approx(case), level);
            let (code, entry) = lower_approx(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = source_bits(case.format, ordinal);

            let mut context = ScalarMemoryContext {
                value: source,
                ok: 1,
                ..ScalarMemoryContext::default()
            };
            let mut registers = initial_registers(bridge, ordinal, true);
            registers.vector_active = if case.operation.uses_k16_opmasks() {
                X86_VECTOR_STATE_K16
            } else {
                X86_VECTOR_STATE_K64
            };
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.load_fn = scalar_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, bridge);
            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
            assert_eq!(context.last_size, case.format.memory_size() as u64);
            assert_eq!(context.last_signed, 0);

            let mut fault_context = ScalarMemoryContext {
                value: source ^ u64::MAX,
                ok: 0,
                ..ScalarMemoryContext::default()
            };
            let mut fault = initial_registers(bridge, ordinal ^ 0x55, true);
            fault.vector_active = if case.operation.uses_k16_opmasks() {
                X86_VECTOR_STATE_K16
            } else {
                X86_VECTOR_STATE_K64
            };
            fault.ctx = (&mut fault_context as *mut ScalarMemoryContext) as u64;
            fault.load_fn = scalar_load_helper as usize as u64;
            let mut fault_expected = fault;
            fault_expected.exit_pc = PC;
            exec.run(entry, &mut fault);
            fault_expected.host_mxcsr = fault.host_mxcsr;
            assert_eq!(fault, fault_expected, "{level:?} {case:?}: source fault");
            assert_eq!(fault_context.calls, 1, "{level:?} {case:?}: fault");

            if case.control != MaskControl::None {
                let mut suppressed_context = ScalarMemoryContext {
                    value: source ^ u64::MAX,
                    ok: 0,
                    ..ScalarMemoryContext::default()
                };
                let mut suppressed = initial_registers(bridge, ordinal ^ 0xAA, false);
                suppressed.vector_active = if case.operation.uses_k16_opmasks() {
                    X86_VECTOR_STATE_K16
                } else {
                    X86_VECTOR_STATE_K64
                };
                suppressed.ctx = (&mut suppressed_context as *mut ScalarMemoryContext) as u64;
                suppressed.load_fn = scalar_load_helper as usize as u64;
                let mut suppressed_expected =
                    interpreter_success(&function, &suppressed, source, bridge);
                exec.run(entry, &mut suppressed);
                suppressed_expected.host_mxcsr = suppressed.host_mxcsr;
                assert_eq!(
                    suppressed, suppressed_expected,
                    "{level:?} {case:?}: inactive mask"
                );
                assert_eq!(suppressed_context.calls, 0, "{level:?} {case:?}");
            }
            executions += 1;
        }
    }
    assert!(executions >= 4 * 3 * 3 * 2 * 2);
}
