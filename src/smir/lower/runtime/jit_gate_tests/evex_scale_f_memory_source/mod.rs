//! Exact helper-backed EVEX VSCALEFPD/PS/PH/SD/SS/SH memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexScaleFMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexScaleFMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_scale_f_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0xA700;
const DISP8: i32 = 2;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
    Scalar { ll: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Merge | Self::Zero => 1,
        }
    }

    const fn zeroing(self) -> bool {
        matches!(self, Self::Zero)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScaleFMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
}

impl ScaleFMemoryCase {
    const fn scalar(self) -> bool {
        matches!(self.form, SourceForm::Scalar { .. })
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn ll(self) -> u8 {
        match self.form {
            SourceForm::Scalar { ll } => ll,
            SourceForm::Vector | SourceForm::Broadcast => match self.width {
                VecWidth::V128 => 0,
                VecWidth::V256 => 1,
                VecWidth::V512 => 2,
                _ => unreachable!(),
            },
        }
    }

    const fn mask(self) -> u8 {
        self.control.mask()
    }

    const fn zeroing(self) -> bool {
        self.control.zeroing()
    }

    const fn memory_size(self) -> u32 {
        if self.scalar() || self.broadcast() {
            self.elem.bytes()
        } else {
            self.width.bytes()
        }
    }

    const fn compressed_displacement(self) -> i32 {
        DISP8 * self.memory_size() as i32
    }

    fn needs_avx512vl(self) -> bool {
        !self.scalar() && self.width != VecWidth::V512
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, self.mask(), self.zeroing(), false, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave at least fourteen low scratch registers")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.scalar() || self.broadcast() || self.mask() != 0 {
            stack_encoding(self, self.mask(), self.zeroing())
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

fn opcode_map(elem: VecElementType) -> u8 {
    match elem {
        VecElementType::F16 => 6,
        VecElementType::F32 | VecElementType::F64 => 2,
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    }
}

fn evex_fields(case: ScaleFMemoryCase, mask: u8, zeroing: bool) -> (u8, u8, u8) {
    assert!(case.destination < 32 && case.source1 < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    assert!(case.scalar() || case.ll() < 3);
    let mut p0 = 0x60 | opcode_map(case.elem);
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 =
        (u8::from(case.elem == VecElementType::F64) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(zeroing) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (if case.source1 & 16 == 0 { 0x08 } else { 0 })
        | mask;
    (p0, p1, p2)
}

fn memory_encoding(
    case: ScaleFMemoryCase,
    mask: u8,
    zeroing: bool,
    apx_base: bool,
    apx_index: bool,
) -> Vec<u8> {
    let (mut p0, mut p1, p2) = evex_fields(case, mask, zeroing);
    if !apx_base && !apx_index {
        return vec![
            0x62,
            p0,
            p1,
            p2,
            if case.scalar() { 0x2D } else { 0x2C },
            ((case.destination & 7) << 3) | 3,
        ];
    }
    if apx_base {
        p0 |= 0x08;
    }
    if apx_index {
        p1 &= !0x04;
    }
    vec![
        0x62,
        p0,
        p1,
        p2,
        if case.scalar() { 0x2D } else { 0x2C },
        0x40 | ((case.destination & 7) << 3) | 0x04,
        0x48,
        DISP8 as u8,
    ]
}

fn stack_encoding(case: ScaleFMemoryCase, mask: u8, zeroing: bool) -> Vec<u8> {
    let (p0, p1, mut p2) = evex_fields(case, mask, zeroing);
    if case.scalar() {
        p2 &= !0x60;
    }
    vec![
        0x62,
        p0,
        p1,
        p2,
        if case.scalar() { 0x2D } else { 0x2C },
        ((case.destination & 7) << 3) | 0x04,
        0x24,
    ]
}

#[test]
fn scalar_llig_is_accepted_and_canonicalized_for_host_replay() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for ll in 0..4 {
            let case = ScaleFMemoryCase {
                elem,
                width: VecWidth::V128,
                destination: 17,
                source1: 18,
                form: SourceForm::Scalar { ll },
                control: MaskControl::Merge,
            };
            let guest = case.bytes();
            assert_eq!((guest[3] >> 5) & 3, ll);
            let replay = X86InstructionBytes::new(&guest)
                .unwrap()
                .evex_scale_f_memory_encoding()
                .expect("scalar VSCALEF LLIG image");
            assert_eq!(replay_instruction(replay)[3] & 0x60, 0);
        }
    }
}

fn register_encoding(case: ScaleFMemoryCase, source2: u8) -> Vec<u8> {
    assert!(source2 < 32 && !case.broadcast());
    let (mut p0, p1, mut p2) = evex_fields(case, case.mask(), case.zeroing());
    p0 &= !0x60;
    if source2 & 16 == 0 {
        p0 |= 0x40;
    }
    if source2 & 8 == 0 {
        p0 |= 0x20;
    }
    p2 &= !0x10;
    vec![
        0x62,
        p0,
        p1,
        p2,
        if case.scalar() { 0x2D } else { 0x2C },
        0xC0 | ((case.destination & 7) << 3) | (source2 & 7),
    ]
}

fn replay_instruction(encoding: crate::smir::ir::X86EvexScaleFMemoryEncoding) -> Vec<u8> {
    match encoding.replay {
        X86EvexScaleFMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction.as_slice().to_vec(),
        X86EvexScaleFMemoryReplay::Broadcast { stack_instruction }
        | X86EvexScaleFMemoryReplay::MaskedVector { stack_instruction }
        | X86EvexScaleFMemoryReplay::Scalar { stack_instruction } => {
            stack_instruction.as_slice().to_vec()
        }
    }
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("VSCALEF encoding fits instruction metadata"),
    );
    function
}

fn lift_case(case: ScaleFMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence_index(function: &SmirFunction) -> usize {
    usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ))
}

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexScaleFMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexScaleFMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_scale_f_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: ScaleFMemoryCase) -> (Vec<u8>, usize) {
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
    assert_eq!(requirements.needs_avx512vl, case.needs_avx512vl());
    assert_eq!(
        requirements.needs_avx512fp16,
        case.elem == VecElementType::F16
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.elem != VecElementType::F16 || std::is_x86_feature_detected!("avx512fp16"))
            && (!case.needs_avx512vl() || std::is_x86_feature_detected!("avx512vl")),
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
        .unwrap_or_else(|error| panic!("{case:?}: VSCALEF memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VSCALEF memory"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<ScaleFMemoryCase> {
    let mut cases = Vec::new();
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source1 in [0, 15, 16, 31] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(ScaleFMemoryCase {
                            elem,
                            width,
                            destination: 17,
                            source1,
                            form,
                            control,
                        });
                    }
                }
            }
        }
        for ll in 0..4u8 {
            for source1 in [0, 15, 16, 31] {
                for control in MaskControl::ALL {
                    cases.push(ScaleFMemoryCase {
                        elem,
                        width: VecWidth::V128,
                        destination: 17,
                        source1,
                        form: SourceForm::Scalar { ll },
                        control,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn llvm_23_replay_byte_anchors_cover_all_six_scale_f_mnemonics() {
    let anchors: [(ScaleFMemoryCase, &[u8]); 6] = [
        (
            ScaleFMemoryCase {
                elem: VecElementType::F32,
                width: VecWidth::V128,
                destination: 0,
                source1: 3,
                form: SourceForm::Broadcast,
                control: MaskControl::None,
            },
            &[0x62, 0xF2, 0x65, 0x18, 0x2C, 0x04, 0x24],
        ),
        (
            ScaleFMemoryCase {
                elem: VecElementType::F64,
                width: VecWidth::V256,
                destination: 17,
                source1: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE2, 0xED, 0x21, 0x2C, 0x0C, 0x24],
        ),
        (
            ScaleFMemoryCase {
                elem: VecElementType::F16,
                width: VecWidth::V512,
                destination: 31,
                source1: 30,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
            },
            &[0x62, 0x66, 0x0D, 0xD1, 0x2C, 0x3C, 0x24],
        ),
        (
            ScaleFMemoryCase {
                elem: VecElementType::F32,
                width: VecWidth::V128,
                destination: 20,
                source1: 21,
                form: SourceForm::Scalar { ll: 0 },
                control: MaskControl::Zero,
            },
            &[0x62, 0xE2, 0x55, 0x81, 0x2D, 0x24, 0x24],
        ),
        (
            ScaleFMemoryCase {
                elem: VecElementType::F64,
                width: VecWidth::V128,
                destination: 31,
                source1: 30,
                form: SourceForm::Scalar { ll: 0 },
                control: MaskControl::Zero,
            },
            &[0x62, 0x62, 0x8D, 0x81, 0x2D, 0x3C, 0x24],
        ),
        (
            ScaleFMemoryCase {
                elem: VecElementType::F16,
                width: VecWidth::V128,
                destination: 17,
                source1: 18,
                form: SourceForm::Scalar { ll: 0 },
                control: MaskControl::Zero,
            },
            &[0x62, 0xE6, 0x6D, 0x81, 0x2D, 0x0C, 0x24],
        ),
    ];
    for (case, expected) in anchors {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_scale_f_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(replay_instruction(encoding), expected, "{case:?}");
    }
}

#[test]
fn scale_f_memory_classifier_exhausts_1_843_200_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for scalar in [false, true] {
            let ll_values: &[u8] = if scalar { &[0, 1, 2, 3] } else { &[0, 1, 2] };
            for &ll in ll_values {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        let broadcasts: &[bool] = if scalar { &[false] } else { &[false, true] };
                        for &broadcast in broadcasts {
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    for apx_base in [false, true] {
                                        for apx_index in [false, true] {
                                            let case = ScaleFMemoryCase {
                                                elem,
                                                width: if scalar {
                                                    VecWidth::V128
                                                } else {
                                                    [VecWidth::V128, VecWidth::V256, VecWidth::V512]
                                                        [usize::from(ll)]
                                                },
                                                destination,
                                                source1,
                                                form: if scalar {
                                                    SourceForm::Scalar { ll }
                                                } else if broadcast {
                                                    SourceForm::Broadcast
                                                } else {
                                                    SourceForm::Vector
                                                },
                                                control: MaskControl::None,
                                            };
                                            let bytes = memory_encoding(
                                                case, mask, zeroing, apx_base, apx_index,
                                            );
                                            let encoding = X86InstructionBytes::new(&bytes)
                                                .unwrap()
                                                .evex_scale_f_memory_encoding()
                                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                            assert_eq!(encoding.width, case.width, "{bytes:02X?}");
                                            assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.destination, destination,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.writemask,
                                                (mask != 0).then_some(mask),
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                            assert_eq!(encoding.scalar, scalar, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.needs_avx512vl,
                                                case.needs_avx512vl(),
                                                "{bytes:02X?}"
                                            );
                                            match encoding.replay {
                                                X86EvexScaleFMemoryReplay::Scalar { .. } => {
                                                    assert!(scalar, "{bytes:02X?}")
                                                }
                                                X86EvexScaleFMemoryReplay::Broadcast { .. } => {
                                                    assert!(!scalar && broadcast, "{bytes:02X?}")
                                                }
                                                X86EvexScaleFMemoryReplay::MaskedVector {
                                                    ..
                                                } => {
                                                    assert!(!scalar && !broadcast && mask != 0)
                                                }
                                                X86EvexScaleFMemoryReplay::Vector {
                                                    scratch,
                                                    ..
                                                } => {
                                                    assert!(!scalar && !broadcast && mask == 0);
                                                    assert_ne!(scratch, destination);
                                                    assert_ne!(scratch, source1);
                                                }
                                            }
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
    }
    assert_eq!(accepted, 1_843_200);
}

#[test]
fn all_360_lifter_shapes_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 360);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, case.elem);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.encoding.scalar, case.scalar());
            assert_eq!(exact.memory_size, case.memory_size());
            assert_eq!(
                exact.consumed + sequence_index(&function),
                function.blocks[0].ops.len()
            );
            assert!(sequence(&function, false).is_none());

            let (code, _) = lower(&function, case);
            let expected = case.expected_replay();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 360 * LEVELS.len());
}

#[test]
fn classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = ScaleFMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [(1, 0x01), (2, 0x01)] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut wrong_opcode = valid.clone();
    wrong_opcode[4] = 0x2E;
    malformed.push(wrong_opcode);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);

    let fp16 = ScaleFMemoryCase {
        elem: VecElementType::F16,
        ..case
    };
    let mut fp16_w = fp16.bytes();
    fp16_w[2] |= 0x80;
    malformed.push(fp16_w);

    let scalar = ScaleFMemoryCase {
        form: SourceForm::Scalar { ll: 3 },
        ..case
    };
    let mut scalar_b = scalar.bytes();
    scalar_b[3] |= 0x10;
    malformed.push(scalar_b);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scale_f_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_scale_f_memory_encoding()
            .is_some()
    );
    let mut repeat_prefixed = vec![0xF3];
    repeat_prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&repeat_prefixed)
            .unwrap()
            .evex_scale_f_memory_encoding()
            .is_none()
    );
}

#[test]
fn fs_gs_addr32_rip_relative_and_apx_addresses_remain_helper_only() {
    let case = ScaleFMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
    };
    for prefixes in [&[0x64][..], &[0x65][..], &[0x67][..], &[0x64, 0x67][..]] {
        let mut bytes = prefixes.to_vec();
        bytes.extend_from_slice(&case.bytes());
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                sequence(&function, true).is_some(),
                "{level:?} {bytes:02X?}"
            );
            lower(&function, case);
        }
    }

    let mut rip = case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    for level in LEVELS {
        let function = optimize(lift_bytes(&rip), level);
        assert!(function.blocks[0].ops.iter().any(|op| match &op.kind {
            OpKind::Load { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::VLoad { addr, .. } => matches!(
                addr,
                crate::smir::ir::types::Address::PcRel {
                    offset: 0x20,
                    disp_size: DispSize::Disp32,
                    ..
                }
            ),
            _ => false,
        }));
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} RIP-relative"));
        lower(&function, case);
    }

    let apx_bytes = memory_encoding(case, case.mask(), case.zeroing(), true, true);
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx_bytes), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert_eq!(sequence_index(&function), 1);
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} APX"));
        lower(&function, case);

        let mut missing_guard = function.clone();
        assert!(matches!(
            missing_guard.blocks[0].ops.remove(0).kind,
            OpKind::X86RequireApx
        ));
        assert!(sequence_at(&missing_guard, 0, true).is_none());
    }
}

#[test]
fn exact_sequence_rejects_mutated_semantics_provenance_and_same_pc_tail() {
    let case = ScaleFMemoryCase {
        elem: VecElementType::F64,
        width: VecWidth::V256,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());

    let reject = |name: &str, mutated: &SmirFunction| {
        assert!(
            sequence(mutated, true).is_none(),
            "{name}: {:#?}",
            mutated.blocks[0].ops
        );
    };
    let mutate_terminal =
        |function: &SmirFunction, mutation: &dyn Fn(&mut crate::smir::ir::ops::OpKind)| {
            let mut changed = function.clone();
            let terminal = changed.blocks[0]
                .ops
                .iter_mut()
                .find(|op| matches!(op.kind, OpKind::X86ScaleF { .. }))
                .expect("VSCALEF terminal");
            mutation(&mut terminal.kind);
            changed
        };
    reject(
        "source1",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86ScaleF { src1, .. } = kind {
                *src1 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(19)));
            }
        }),
    );
    reject(
        "element",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86ScaleF { elem, .. } = kind {
                *elem = VecElementType::F32;
            }
        }),
    );
    reject(
        "rounding",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86ScaleF { round, .. } = kind {
                *round = FpRoundMode::RoundDown;
            }
        }),
    );
    reject(
        "exception mode",
        &mutate_terminal(&function, &|kind| {
            if let OpKind::X86ScaleF {
                suppress_exceptions,
                ..
            } = kind
            {
                *suppress_exceptions = true;
            }
        }),
    );

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    reject("provenance", &missing_provenance);

    let mut hinted = function.clone();
    let address_index = sequence_index(&hinted) + sequence(&hinted, true).unwrap().address_offset;
    hinted.blocks[0].ops[address_index].x86_hint =
        Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
    reject("memory hint", &hinted);

    let mut tail = function.clone();
    tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFF)),
            src: crate::smir::ir::types::SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    ));
    reject("same-PC tail", &tail);
}
