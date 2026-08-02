//! Exact helper-backed EVEX VPUNPCK* Full Mem coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexIntegerInterleaveMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_integer_interleave_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E60;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InterleaveKind {
    name: &'static str,
    opcode: u8,
    elem: VecElementType,
    high: bool,
}

impl InterleaveKind {
    const ALL: [Self; 8] = [
        Self::new("VPUNPCKLBW", 0x60, VecElementType::I8, false),
        Self::new("VPUNPCKLWD", 0x61, VecElementType::I16, false),
        Self::new("VPUNPCKLDQ", 0x62, VecElementType::I32, false),
        Self::new("VPUNPCKLQDQ", 0x6C, VecElementType::I64, false),
        Self::new("VPUNPCKHBW", 0x68, VecElementType::I8, true),
        Self::new("VPUNPCKHWD", 0x69, VecElementType::I16, true),
        Self::new("VPUNPCKHDQ", 0x6A, VecElementType::I32, true),
        Self::new("VPUNPCKHQDQ", 0x6D, VecElementType::I64, true),
    ];

    const fn new(name: &'static str, opcode: u8, elem: VecElementType, high: bool) -> Self {
        Self {
            name,
            opcode,
            elem,
            high,
        }
    }

    const fn is_wig(self) -> bool {
        matches!(self.elem, VecElementType::I8 | VecElementType::I16)
    }

    const fn w(self, wig_w: bool) -> bool {
        match self.elem {
            VecElementType::I8 | VecElementType::I16 => wig_w,
            VecElementType::I32 => false,
            VecElementType::I64 => true,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (3, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegerInterleaveMemoryCase {
    kind: InterleaveKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    control: MaskControl,
    /// Raw EVEX.W for byte/word WIG encodings.
    wig_w: bool,
}

impl IntegerInterleaveMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    const fn w(self) -> bool {
        self.kind.w(self.wig_w)
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        register_encoding(self, self.scratch())
    }
}

fn memory_encoding(case: IntegerInterleaveMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(case.mask() < 8 && (!case.zeroing() || case.mask() != 0));
    let p0 = 0x61
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.w()) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn register_encoding(case: IntegerInterleaveMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x41
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = (u8::from(case.w()) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
    ]
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("EVEX integer-interleave provenance"),
    );
    function
}

fn lift_case(case: IntegerInterleaveMemoryCase) -> SmirFunction {
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

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexIntegerInterleaveMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_integer_interleave_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: IntegerInterleaveMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
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
        .unwrap_or_else(|error| panic!("{case:?}: EVEX integer-interleave lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX integer interleave"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<IntegerInterleaveMemoryCase> {
    let mut cases = Vec::new();
    for kind in InterleaveKind::ALL {
        for wig_w in [false, true] {
            if !kind.is_wig() && wig_w {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for (destination, source1) in [(0, 0), (9, 10), (17, 17)] {
                    for control in MaskControl::ALL {
                        cases.push(IntegerInterleaveMemoryCase {
                            kind,
                            width,
                            destination,
                            source1,
                            control,
                            wig_w,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn interleave_rewrites_match_eight_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xF1, 0x6D, 0x8A, 0x60, 0x0A],
            &[0x62, 0xF1, 0x6D, 0x8A, 0x60, 0xC8],
        ),
        (
            &[0x62, 0x71, 0x2D, 0x2B, 0x61, 0x0A],
            &[0x62, 0x71, 0x2D, 0x2B, 0x61, 0xC8],
        ),
        (
            &[0x62, 0xE1, 0x6D, 0xC1, 0x62, 0x0A],
            &[0x62, 0xE1, 0x6D, 0xC1, 0x62, 0xC8],
        ),
        (
            &[0x62, 0x61, 0xAD, 0x47, 0x6C, 0x0A],
            &[0x62, 0x61, 0xAD, 0x47, 0x6C, 0xC8],
        ),
        (
            &[0x62, 0xF1, 0x6D, 0x8A, 0x68, 0x0A],
            &[0x62, 0xF1, 0x6D, 0x8A, 0x68, 0xC8],
        ),
        (
            &[0x62, 0x71, 0x2D, 0x29, 0x69, 0x0A],
            &[0x62, 0x71, 0x2D, 0x29, 0x69, 0xC8],
        ),
        (
            &[0x62, 0xE1, 0x6D, 0x44, 0x6A, 0x0A],
            &[0x62, 0xE1, 0x6D, 0x44, 0x6A, 0xC8],
        ),
        (
            &[0x62, 0x61, 0xAD, 0xC5, 0x6D, 0x0A],
            &[0x62, 0x61, 0xAD, 0xC5, 0x6D, 0xC8],
        ),
    ];
    for (memory, register) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_interleave_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(
            encoding.register_instruction.as_slice(),
            *register,
            "{memory:02X?}"
        );
    }
}

#[test]
fn interleave_classifier_exhausts_2_211_840_operand_control_wig_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in InterleaveKind::ALL {
        for wig_w in [false, true] {
            if !kind.is_wig() && wig_w {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let case = IntegerInterleaveMemoryCase {
                                    kind,
                                    width,
                                    destination,
                                    source1,
                                    control: MaskControl::None,
                                    wig_w,
                                };
                                let mut canonical = memory_encoding(case, true);
                                canonical[3] =
                                    (canonical[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical.clone();
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_integer_interleave_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                                        assert_eq!(encoding.elem, kind.elem, "{bytes:02X?}");
                                        assert_eq!(encoding.high, kind.high, "{bytes:02X?}");
                                        assert_eq!(encoding.opcode, kind.opcode, "{bytes:02X?}");
                                        assert_eq!(encoding.w, kind.w(wig_w), "{bytes:02X?}");
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
                                        assert_eq!(encoding.memory_size, width.bytes());
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            width != VecWidth::V512
                                        );
                                        assert_ne!(encoding.scratch, destination);
                                        assert_ne!(encoding.scratch, source1);
                                        assert_eq!(
                                            encoding
                                                .register_instruction
                                                .evex_register_integer_interleave_needs_vl(),
                                            Some(width != VecWidth::V512),
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
    assert_eq!(accepted, 2_211_840);
}

#[test]
fn interleave_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = IntegerInterleaveMemoryCase {
        kind: InterleaveKind::ALL[2],
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        control: MaskControl::Merge,
        wig_w: false,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x02), // map
        (2, 0x01), // mandatory prefix
        (3, 0x10), // broadcast
        (4, 0x04), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut dword_w1 = valid.clone();
    dword_w1[2] |= 0x80;
    malformed.push(dword_w1);
    let mut qword_w0 = IntegerInterleaveMemoryCase {
        kind: InterleaveKind::ALL[3],
        ..case
    }
    .bytes();
    qword_w0[2] &= !0x80;
    malformed.push(qword_w0);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_interleave_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_integer_interleave_memory_encoding()
            .is_some(),
        "FS/address-size prefixes belong to helper address evaluation"
    );

    for kind in [InterleaveKind::ALL[0], InterleaveKind::ALL[1]] {
        for wig_w in [false, true] {
            let bytes = IntegerInterleaveMemoryCase {
                kind,
                wig_w,
                ..case
            }
            .bytes();
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_integer_interleave_memory_encoding()
                    .is_some(),
                "{} rejected W={wig_w}: {bytes:02X?}",
                kind.name
            );
        }
    }
}

#[test]
fn all_324_interleave_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 324);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.elem, case.kind.elem);
            assert_eq!(exact.encoding.high, case.kind.high);
            assert_eq!(exact.encoding.opcode, case.kind.opcode);
            assert_eq!(exact.encoding.w, case.w());
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.memory_size, case.width.bytes());
            assert_eq!(exact.consumed, function.blocks[0].ops.len());

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
    assert_eq!(lowerings, 324 * LEVELS.len());
}

#[test]
fn type_e4nf_interleave_graphs_always_preserve_one_complete_access() {
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let vector_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VLoad { width, .. } if width == case.width))
                .count();
            let pred_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count();
            assert_eq!((vector_loads, pred_loads), (1, 0), "{level:?} {case:?}");
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence classifier admitted malformed graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer admitted malformed graph"
    );
}

#[test]
fn interleave_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let case = IntegerInterleaveMemoryCase {
        kind: InterleaveKind::ALL[5],
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        control: MaskControl::Zero,
        wig_w: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, false).is_none());

    let mut mutations = Vec::<(&str, SmirFunction)>::new();
    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_provenance = function.clone();
    let mut bytes = case.bytes();
    bytes[4] = 0x61;
    wrong_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("wrong opcode provenance", wrong_provenance));

    let load_index = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .unwrap();
    let interleave_index = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VInterleave { .. }))
        .unwrap();

    let mut wrong_address = function.clone();
    if let OpKind::VLoad { addr, .. } = &mut wrong_address.blocks[0].ops[load_index].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
    }
    mutations.push(("virtual address", wrong_address));

    let mut wrong_width = function.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[load_index].kind {
        *width = VecWidth::V128;
    }
    mutations.push(("load width", wrong_width));

    let mut wrong_interleave = function.clone();
    if let OpKind::VInterleave {
        src1,
        elem,
        high,
        block_lanes,
        ..
    } = &mut wrong_interleave.blocks[0].ops[interleave_index].kind
    {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(11)));
        *elem = VecElementType::I8;
        *high = false;
        *block_lanes = block_lanes.wrapping_add(1);
    }
    mutations.push(("interleave contract", wrong_interleave));

    let mut wrong_hint = function.clone();
    wrong_hint.blocks[0].ops[interleave_index].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: case.kind.opcode,
        width: case.width,
        w: false,
    });
    mutations.push(("interleave hint", wrong_hint));

    let mut wrong_lane = function.clone();
    let extract = wrong_lane.blocks[0]
        .ops
        .iter_mut()
        .rev()
        .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .unwrap();
    if let OpKind::VExtractLane { lane, .. } = &mut extract.kind {
        *lane = lane.wrapping_add(1);
    }
    mutations.push(("mask-result lane", wrong_lane));

    let mut tail = function.clone();
    tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFF)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    mutations.push(("same-PC tail", tail));

    for (name, mutated) in mutations {
        assert_rejected(name, &mutated);
    }
}

#[test]
fn interleave_segment_addr32_rip_disp8_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let case = IntegerInterleaveMemoryCase {
        kind: InterleaveKind::ALL[6],
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        control: MaskControl::Merge,
        wig_w: false,
    };

    let mut rip = case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes();
    fs.insert(0, 0x64);
    let mut gs_addr32 = memory_encoding(case, true);
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32[6] = 0x8B;
    gs_addr32.push(2);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);
    let mut disp8 = case.bytes();
    disp8[5] = (disp8[5] & 0x38) | 0x43;
    disp8.push(0xFE);

    let address_cases = [
        (
            "RIP+disp32",
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 10),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS base",
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rdx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB",
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 2 * i64::from(case.width.bytes()),
            })),
        ),
        (
            "compressed disp8",
            disp8,
            Address::BaseOffset {
                base: x86(X86Reg::Rbx),
                offset: -2 * i64::from(case.width.bytes()),
                disp_size: DispSize::Disp8,
            },
        ),
    ];

    for (name, bytes, expected_address) in address_cases {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| matches!(
                    &op.kind,
                    OpKind::VLoad { addr, width, .. }
                        if addr == &expected_address && *width == case.width
                )),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let apx_case = IntegerInterleaveMemoryCase {
        kind: InterleaveKind::ALL[1],
        width: VecWidth::V512,
        destination: 25,
        source1: 26,
        control: MaskControl::Zero,
        wig_w: true,
    };
    let mut apx = memory_encoding(apx_case, true);
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    let expected_address = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    let base = lift_bytes(&apx);
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(
            matches!(
                function.blocks[0].ops.first().map(|op| &op.kind),
                Some(OpKind::X86RequireApx)
            ),
            "{level:?} {apx:02X?}: APX address lost its dynamic guard"
        );
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::VLoad { addr, .. } if addr == &expected_address
        )));
        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {apx:02X?}"));
        lower(&function, apx_case);
    }
}

#[test]
fn interleave_rejects_the_avx_only_state_bridge() {
    let case = IntegerInterleaveMemoryCase {
        kind: InterleaveKind::ALL[4],
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        control: MaskControl::Zero,
        wig_w: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject EVEX integer interleaves");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
