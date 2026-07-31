//! Exact helper-backed EVEX saturating integer-pack memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexIntegerArithmeticMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexIntegerPackMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_integer_pack_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackKind {
    name: &'static str,
    map: X86VecMap,
    opcode: u8,
    src_elem: VecElementType,
    dst_elem: VecElementType,
    to_unsigned: bool,
}

impl PackKind {
    const ALL: [Self; 4] = [
        Self::new(
            "VPACKSSWB",
            X86VecMap::Map0F,
            0x63,
            VecElementType::I16,
            VecElementType::I8,
            false,
        ),
        Self::new(
            "VPACKUSWB",
            X86VecMap::Map0F,
            0x67,
            VecElementType::I16,
            VecElementType::I8,
            true,
        ),
        Self::new(
            "VPACKSSDW",
            X86VecMap::Map0F,
            0x6B,
            VecElementType::I32,
            VecElementType::I16,
            false,
        ),
        Self::new(
            "VPACKUSDW",
            X86VecMap::Map0F38,
            0x2B,
            VecElementType::I32,
            VecElementType::I16,
            true,
        ),
    ];

    const fn new(
        name: &'static str,
        map: X86VecMap,
        opcode: u8,
        src_elem: VecElementType,
        dst_elem: VecElementType,
        to_unsigned: bool,
    ) -> Self {
        Self {
            name,
            map,
            opcode,
            src_elem,
            dst_elem,
            to_unsigned,
        }
    }

    const fn map_byte(self) -> u8 {
        match self.map {
            X86VecMap::Map0F => 1,
            X86VecMap::Map0F38 => 2,
            _ => unreachable!(),
        }
    }

    const fn is_wig(self) -> bool {
        matches!(self.src_elem, VecElementType::I16)
    }

    const fn allows_broadcast(self) -> bool {
        matches!(self.src_elem, VecElementType::I32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
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
struct IntegerPackMemoryCase {
    kind: PackKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
    /// Raw EVEX.W for byte-result WIG encodings. Dword-to-word forms use W0.
    wig_w: bool,
}

impl IntegerPackMemoryCase {
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

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    const fn w(self) -> bool {
        self.kind.is_wig() && self.wig_w
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
        if self.broadcast() {
            stack_encoding(self)
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

fn memory_encoding(case: IntegerPackMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(case.mask() < 8 && (!case.zeroing() || case.mask() != 0));
    assert!(!case.broadcast() || case.kind.allows_broadcast());
    let p0 = case.kind.map_byte()
        | 0x60
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.w() { 0x80 } else { 0 } | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        ((case.destination & 7) << 3) | if sib { 4 } else { 3 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: IntegerPackMemoryCase) -> Vec<u8> {
    let p0 = case.kind.map_byte()
        | 0x60
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = if case.w() { 0x80 } else { 0 } | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | 0x10
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode,
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
}

fn register_encoding(case: IntegerPackMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = case.kind.map_byte()
        | 0x40
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = if case.w() { 0x80 } else { 0 } | (((!case.source1) & 0x0F) << 3) | 0x05;
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
        X86InstructionBytes::new(bytes).expect("EVEX integer-pack provenance"),
    );
    function
}

fn lift_case(case: IntegerPackMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexIntegerPackMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_integer_pack_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: IntegerPackMemoryCase) -> (Vec<u8>, usize) {
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
        .unwrap_or_else(|error| panic!("{case:?}: EVEX integer-pack lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX integer pack"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<IntegerPackMemoryCase> {
    let mut cases = Vec::new();
    for kind in PackKind::ALL {
        for wig_w in [false, true] {
            if !kind.is_wig() && wig_w {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for (destination, source1) in [(0, 0), (9, 10), (17, 18)] {
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        if form == SourceForm::Broadcast && !kind.allows_broadcast() {
                            continue;
                        }
                        for control in MaskControl::ALL {
                            cases.push(IntegerPackMemoryCase {
                                kind,
                                width,
                                destination,
                                source1,
                                form,
                                control,
                                wig_w,
                            });
                        }
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn integer_pack_rewrites_match_six_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0xE1, 0x6D, 0x00, 0x63, 0x0A],
            &[0x62, 0xE1, 0x6D, 0x00, 0x63, 0xC8],
        ),
        (
            &[0x62, 0x71, 0x2D, 0x2B, 0x67, 0x0A],
            &[0x62, 0x71, 0x2D, 0x2B, 0x67, 0xC8],
        ),
        (
            &[0x62, 0x71, 0x2D, 0xCB, 0x67, 0x0A],
            &[0x62, 0x71, 0x2D, 0xCB, 0x67, 0xC8],
        ),
        (
            &[0x62, 0xE1, 0x6D, 0xC1, 0x6B, 0x0A],
            &[0x62, 0xE1, 0x6D, 0xC1, 0x6B, 0xC8],
        ),
        (
            &[0x62, 0x62, 0x2D, 0x54, 0x2B, 0x0A],
            &[0x62, 0x62, 0x2D, 0x54, 0x2B, 0x0C, 0x24],
        ),
        (
            &[0x62, 0xF1, 0x6D, 0x19, 0x6B, 0x0A],
            &[0x62, 0xF1, 0x6D, 0x19, 0x6B, 0x0C, 0x24],
        ),
    ];
    for (memory, llvm) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_integer_pack_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        let replay = match encoding.replay {
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                register_instruction,
                ..
            } => register_instruction,
            X86EvexIntegerArithmeticMemoryReplay::Broadcast { stack_instruction } => {
                stack_instruction
            }
            X86EvexIntegerArithmeticMemoryReplay::MaskedVector { .. } => {
                panic!("Type E4NF pack used masked per-lane replay")
            }
        };
        assert_eq!(replay.as_slice(), *llvm, "{memory:02X?}");
    }
}

#[test]
fn integer_pack_classifier_exhausts_1_474_560_operand_control_wig_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in PackKind::ALL {
        for wig_w in [false, true] {
            if !kind.is_wig() && wig_w {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for destination in 0..32u8 {
                    for source1 in 0..32u8 {
                        for form in [SourceForm::Vector, SourceForm::Broadcast] {
                            if form == SourceForm::Broadcast && !kind.allows_broadcast() {
                                continue;
                            }
                            for mask in 0..8u8 {
                                for zeroing in [false, true] {
                                    if zeroing && mask == 0 {
                                        continue;
                                    }
                                    let case = IntegerPackMemoryCase {
                                        kind,
                                        width,
                                        destination,
                                        source1,
                                        form,
                                        control: if mask == 0 {
                                            MaskControl::None
                                        } else if zeroing {
                                            MaskControl::Zero
                                        } else {
                                            MaskControl::Merge
                                        },
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
                                                .evex_integer_pack_memory_encoding()
                                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                            assert_eq!(encoding.map, kind.map, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.opcode, kind.opcode,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(encoding.w, case.w(), "{bytes:02X?}");
                                            assert_eq!(encoding.width, width, "{bytes:02X?}");
                                            assert_eq!(
                                                encoding.src_elem, kind.src_elem,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                encoding.dst_elem, kind.dst_elem,
                                                "{bytes:02X?}"
                                            );
                                            assert_eq!(
                                                encoding.to_unsigned, kind.to_unsigned,
                                                "{bytes:02X?}"
                                            );
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
                                            assert_eq!(
                                                encoding.needs_avx512vl,
                                                width != VecWidth::V512,
                                                "{bytes:02X?}"
                                            );
                                            match encoding.replay {
                                                X86EvexIntegerArithmeticMemoryReplay::Vector {
                                                    scratch,
                                                    register_instruction,
                                                } => {
                                                    assert_eq!(form, SourceForm::Vector);
                                                    assert_ne!(
                                                        scratch, destination,
                                                        "{bytes:02X?}"
                                                    );
                                                    assert_ne!(scratch, source1, "{bytes:02X?}");
                                                    assert_eq!(
                                                        register_instruction
                                                            .evex_register_integer_pack_needs_vl(),
                                                        Some(width != VecWidth::V512),
                                                        "{bytes:02X?}"
                                                    );
                                                }
                                                X86EvexIntegerArithmeticMemoryReplay::Broadcast {
                                                    ..
                                                } => {
                                                    assert_eq!(form, SourceForm::Broadcast);
                                                }
                                                X86EvexIntegerArithmeticMemoryReplay::MaskedVector {
                                                    ..
                                                } => panic!(
                                                    "Type E4NF pack selected masked replay: {bytes:02X?}"
                                                ),
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
    assert_eq!(accepted, 1_474_560);
}

#[test]
fn integer_pack_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let case = IntegerPackMemoryCase {
        kind: PackKind::ALL[2],
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
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
        (1, 0x04), // map
        (2, 0x01), // mandatory prefix
        (4, 0x40), // non-owned opcode
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
    let mut byte_broadcast = IntegerPackMemoryCase {
        kind: PackKind::ALL[0],
        ..case
    }
    .bytes();
    byte_broadcast[3] |= 0x10;
    malformed.push(byte_broadcast);
    let mut dword_w1 = valid.clone();
    dword_w1[2] |= 0x80;
    malformed.push(dword_w1);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_pack_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_integer_pack_memory_encoding()
            .is_some(),
        "FS/address-size prefixes belong to helper address evaluation"
    );

    for wig_w in [false, true] {
        let bytes = IntegerPackMemoryCase {
            kind: PackKind::ALL[1],
            wig_w,
            ..case
        }
        .bytes();
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_integer_pack_memory_encoding()
                .is_some(),
            "WIG word-to-byte form rejected W={wig_w}: {bytes:02X?}"
        );
    }
}

#[test]
fn all_216_integer_pack_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 216);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.map, case.kind.map);
            assert_eq!(exact.encoding.opcode, case.kind.opcode);
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.src_elem, case.kind.src_elem);
            assert_eq!(exact.encoding.dst_elem, case.kind.dst_elem);
            assert_eq!(exact.encoding.to_unsigned, case.kind.to_unsigned);
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(exact.encoding.source1, case.source1);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.w, case.w());
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    4
                } else {
                    case.width.bytes()
                }
            );
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
    assert_eq!(lowerings, 216 * LEVELS.len());
}

#[test]
fn type_e4nf_memory_graphs_always_preserve_one_complete_access() {
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let pred_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count();
            let ordinary_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
                .count();
            assert_eq!((ordinary_loads, pred_loads), (1, 0), "{level:?} {case:?}");
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
}

#[test]
fn integer_pack_sequence_fails_closed_for_provenance_and_graph_mutations() {
    for case in [
        IntegerPackMemoryCase {
            kind: PackKind::ALL[1],
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            wig_w: true,
        },
        IntegerPackMemoryCase {
            kind: PackKind::ALL[3],
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            wig_w: false,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function, false).is_none());

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[3] = (bytes[3] & !7) | if case.mask() == 1 { 2 } else { 1 };
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong mask provenance", &wrong_provenance);

        let mut wrong_address = function.clone();
        let memory_op = wrong_address.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
            .unwrap();
        match &mut memory_op.kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
            }
            _ => unreachable!(),
        }
        assert_rejected("virtual address", &wrong_address);

        let mut wrong_pack = function.clone();
        let pack = wrong_pack.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::VPackSat { .. }))
            .unwrap();
        let OpKind::VPackSat {
            to_unsigned,
            block_lanes,
            ..
        } = &mut pack.kind
        else {
            unreachable!()
        };
        *to_unsigned = !*to_unsigned;
        *block_lanes = block_lanes.wrapping_add(1);
        assert_rejected("wrong pack contract", &wrong_pack);

        let mut wrong_lane = function.clone();
        let extract = wrong_lane.blocks[0]
            .ops
            .iter_mut()
            .rev()
            .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
            .unwrap();
        let OpKind::VExtractLane { lane, .. } = &mut extract.kind else {
            unreachable!()
        };
        *lane = lane.wrapping_add(1);
        assert_rejected("wrong result lane", &wrong_lane);

        let mut tail = function.clone();
        tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFF),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFF)),
                src: SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("same-PC tail", &tail);
    }
}

#[test]
fn integer_pack_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = IntegerPackMemoryCase {
        kind: PackKind::ALL[2],
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::None,
        wig_w: false,
    };
    let broadcast_case = IntegerPackMemoryCase {
        kind: PackKind::ALL[3],
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
        ..vector_case
    };

    let mut rip = vector_case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = vector_case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = broadcast_case.bytes();
    fs.insert(0, 0x64);
    let mut gs_addr32 = broadcast_case.bytes();
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32.push(0x8B);
    gs_addr32.push(2);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let address_cases = [
        (
            "RIP+disp32",
            vector_case,
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 10),
            },
        ),
        (
            "addr32 base",
            vector_case,
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS broadcast",
            broadcast_case,
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB broadcast",
            broadcast_case,
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 8,
            })),
        ),
    ];

    for (name, case, bytes, expected_address) in address_cases {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } =>
                        addr == &expected_address,
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    for (case, expected_address) in [
        (
            IntegerPackMemoryCase {
                kind: PackKind::ALL[1],
                width: VecWidth::V512,
                destination: 17,
                source1: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                wig_w: true,
            },
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R16)),
                index: x86(X86Reg::R17),
                scale: 2,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        ),
        (
            IntegerPackMemoryCase {
                kind: PackKind::ALL[3],
                width: VecWidth::V512,
                destination: 25,
                source1: 26,
                form: SourceForm::Broadcast,
                control: MaskControl::Zero,
                wig_w: false,
            },
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R20)),
                index: x86(X86Reg::R21),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        ),
    ] {
        let mut bytes = memory_encoding(case, true);
        bytes[1] |= 0x08;
        bytes[2] &= !0x04;
        if case.destination == 25 {
            bytes[6] = 0xEC;
        }
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                matches!(
                    function.blocks[0].ops.first().map(|op| &op.kind),
                    Some(OpKind::X86RequireApx)
                ),
                "{level:?} {bytes:02X?}: APX address lost its dynamic guard"
            );
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } =>
                        addr == &expected_address,
                    _ => false,
                }),
                "{level:?} {bytes:02X?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
            lower(&function, case);
        }
    }
}

#[test]
fn integer_pack_rejects_the_avx_only_state_bridge() {
    let case = IntegerPackMemoryCase {
        kind: PackKind::ALL[1],
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
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
        .expect_err("AVX-only state bridge must reject EVEX integer packs");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
