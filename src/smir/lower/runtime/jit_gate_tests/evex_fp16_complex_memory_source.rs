//! Exact helper-backed packed/scalar EVEX FP16 complex memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, MemWidth, OpId, SourceArch, VReg,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedFp16ComplexMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, X86JitEvexPackedFp16ComplexMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_evex_packed_fp16_complex_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xF1C0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

mod scalar_memory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComplexOperation {
    ConjugateAccumulate,
    Accumulate,
    ConjugateMultiply,
    Multiply,
}

impl ComplexOperation {
    const ALL: [Self; 4] = [
        Self::ConjugateAccumulate,
        Self::Accumulate,
        Self::ConjugateMultiply,
        Self::Multiply,
    ];

    const fn opcode(self) -> u8 {
        match self {
            Self::ConjugateAccumulate | Self::Accumulate => 0x56,
            Self::ConjugateMultiply | Self::Multiply => 0xD6,
        }
    }

    const fn accumulate(self) -> bool {
        matches!(self, Self::ConjugateAccumulate | Self::Accumulate)
    }

    const fn conjugate(self) -> bool {
        matches!(self, Self::ConjugateAccumulate | Self::ConjugateMultiply)
    }

    const fn pp(self) -> u8 {
        if self.conjugate() { 3 } else { 2 }
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
            Self::Merge => (1, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fp16ComplexMemoryCase {
    operation: ComplexOperation,
    width: VecWidth,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
}

impl Fp16ComplexMemoryCase {
    const fn destination(self) -> u8 {
        0
    }

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

    fn bytes(self) -> [u8; 6] {
        memory_encoding(
            self.operation,
            self.destination(),
            self.source1,
            self.ll(),
            self.mask(),
            self.zeroing(),
            self.broadcast(),
            3,
        )
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(
                self.operation,
                self.destination(),
                self.source1,
                self.ll(),
                self.mask(),
                self.zeroing(),
                self.broadcast(),
            )
            .to_vec()
        } else {
            register_encoding(
                self.operation,
                self.destination(),
                self.source1,
                self.scratch(),
                self.ll(),
                0,
                false,
            )
            .to_vec()
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination() && *candidate != self.source1)
            .expect("two operands leave at least fourteen low vector registers")
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("packed EVEX FP16 complex width"),
    }))
}

fn memory_encoding(
    operation: ComplexOperation,
    destination: u8,
    source1: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
    base: u8,
) -> [u8; 6] {
    assert!(destination < 32 && source1 < 32 && base < 16 && destination != source1);
    assert!(ll < 3 && mask < 8 && (!zeroing || mask != 0));
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | 0x40
            | (if base & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | 0x06,
        (((!source1) & 0x0F) << 3) | 0x04 | operation.pp(),
        (u8::from(zeroing) << 7)
            | (ll << 5)
            | (u8::from(broadcast) << 4)
            | (if source1 & 16 == 0 { 0x08 } else { 0 })
            | mask,
        operation.opcode(),
        ((destination & 7) << 3) | (base & 7),
    ]
}

fn stack_encoding(
    operation: ComplexOperation,
    destination: u8,
    source1: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    broadcast: bool,
) -> [u8; 7] {
    let mut encoding = memory_encoding(
        operation,
        destination,
        source1,
        ll,
        mask,
        zeroing,
        broadcast,
        4,
    );
    encoding[1] |= 0x20;
    [
        encoding[0],
        encoding[1],
        encoding[2],
        encoding[3],
        encoding[4],
        encoding[5],
        0x24,
    ]
}

fn register_encoding(
    operation: ComplexOperation,
    destination: u8,
    source1: u8,
    source2: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(
        destination < 32
            && source1 < 32
            && source2 < 32
            && destination != source1
            && destination != source2
    );
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | (if source2 & 16 == 0 { 0x40 } else { 0 })
            | (if source2 & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | 0x06,
        (((!source1) & 0x0F) << 3) | 0x04 | operation.pp(),
        (u8::from(zeroing) << 7) | (ll << 5) | (if source1 & 16 == 0 { 0x08 } else { 0 }) | mask,
        operation.opcode(),
        0xC0 | ((destination & 7) << 3) | (source2 & 7),
    ]
}

fn lift_case(case: Fp16ComplexMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
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
        X86InstructionBytes::new(&bytes).expect("packed FP16-complex memory provenance"),
    );
    function
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
) -> Option<X86JitEvexPackedFp16ComplexMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first(),
        Some(SmirOp {
            kind: OpKind::X86RequireApx,
            ..
        })
    ));
    x86_jit_evex_packed_fp16_complex_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: Fp16ComplexMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx512fp16, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512fp16")
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
        .unwrap_or_else(|error| panic!("{case:?}: packed FP16-complex memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed packed FP16 complex"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<Fp16ComplexMemoryCase> {
    let mut cases = Vec::new();
    for operation in ComplexOperation::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source1 in [1, 17, 30] {
                for form in [SourceForm::Vector, SourceForm::Broadcast] {
                    for control in MaskControl::ALL {
                        cases.push(Fp16ComplexMemoryCase {
                            operation,
                            width,
                            source1,
                            form,
                            control,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn packed_fp16_complex_memory_classifier_exhaustively_rewrites_1_428_480_control_and_apx_address_cells()
 {
    let mut accepted = 0usize;
    for operation in ComplexOperation::ALL {
        for (ll, width) in [
            (0, VecWidth::V128),
            (1, VecWidth::V256),
            (2, VecWidth::V512),
        ] {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    if destination == source1 {
                        continue;
                    }
                    for broadcast in [false, true] {
                        for mask in 0..8u8 {
                            for zeroing in [false, true] {
                                if zeroing && mask == 0 {
                                    continue;
                                }
                                let canonical = memory_encoding(
                                    operation,
                                    destination,
                                    source1,
                                    ll,
                                    mask,
                                    zeroing,
                                    broadcast,
                                    3,
                                );
                                for base_high in [false, true] {
                                    for index_high in [false, true] {
                                        let mut bytes = canonical;
                                        bytes[1] |= u8::from(base_high) << 3;
                                        if index_high {
                                            bytes[2] &= !0x04;
                                        }
                                        let encoding = X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_packed_fp16_complex_memory_encoding()
                                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                        assert_eq!(encoding.width, width, "{bytes:02X?}");
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
                                            encoding.accumulate,
                                            operation.accumulate(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.conjugate,
                                            operation.conjugate(),
                                            "{bytes:02X?}"
                                        );
                                        assert_eq!(
                                            encoding.needs_avx512vl,
                                            ll != 2,
                                            "{bytes:02X?}"
                                        );

                                        let expected_stack = stack_encoding(
                                            operation,
                                            destination,
                                            source1,
                                            ll,
                                            mask,
                                            zeroing,
                                            broadcast,
                                        );
                                        match encoding.replay {
                                            X86EvexPackedFp16ComplexMemoryReplay::Broadcast {
                                                stack_instruction,
                                            } => {
                                                assert!(broadcast, "{bytes:02X?}");
                                                assert_eq!(
                                                    stack_instruction.as_slice(),
                                                    expected_stack,
                                                    "{bytes:02X?}"
                                                );
                                            }
                                            X86EvexPackedFp16ComplexMemoryReplay::MaskedVector {
                                                stack_instruction,
                                            } => {
                                                assert!(!broadcast && mask != 0, "{bytes:02X?}");
                                                assert_eq!(
                                                    stack_instruction.as_slice(),
                                                    expected_stack,
                                                    "{bytes:02X?}"
                                                );
                                            }
                                            X86EvexPackedFp16ComplexMemoryReplay::Vector {
                                                scratch,
                                                register_instruction,
                                            } => {
                                                assert!(!broadcast && mask == 0, "{bytes:02X?}");
                                                let expected_scratch = (0..16)
                                                    .find(|candidate| {
                                                        *candidate != destination
                                                            && *candidate != source1
                                                    })
                                                    .unwrap();
                                                assert_eq!(
                                                    scratch, expected_scratch,
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    register_instruction.as_slice(),
                                                    register_encoding(
                                                        operation,
                                                        destination,
                                                        source1,
                                                        scratch,
                                                        ll,
                                                        0,
                                                        false,
                                                    ),
                                                    "{bytes:02X?}"
                                                );
                                                assert_eq!(
                                                    register_instruction
                                                        .evex_register_packed_fp16_complex_needs_vl(),
                                                    Some(ll != 2),
                                                    "{bytes:02X?}"
                                                );
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
    assert_eq!(accepted, 4 * 3 * 32 * 31 * 2 * 15 * 2 * 2);

    for opcode in 0..=u8::MAX {
        let bytes = memory_encoding(
            ComplexOperation::ConjugateAccumulate,
            0,
            1,
            0,
            0,
            false,
            false,
            3,
        );
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3], opcode, bytes[5]];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fp16_complex_memory_encoding()
                .is_some(),
            matches!(opcode, 0x56 | 0x57 | 0xD6 | 0xD7),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn packed_fp16_complex_encodings_match_six_independent_llvm_23_anchors() {
    for (actual, llvm) in [
        (
            register_encoding(ComplexOperation::ConjugateAccumulate, 0, 1, 2, 0, 0, false).to_vec(),
            vec![0x62, 0xF6, 0x77, 0x08, 0x56, 0xC2],
        ),
        (
            register_encoding(ComplexOperation::Accumulate, 16, 17, 0, 1, 0, false).to_vec(),
            vec![0x62, 0xE6, 0x76, 0x20, 0x56, 0xC0],
        ),
        (
            register_encoding(ComplexOperation::ConjugateMultiply, 31, 30, 0, 2, 0, false).to_vec(),
            vec![0x62, 0x66, 0x0F, 0x40, 0xD6, 0xF8],
        ),
        (
            stack_encoding(ComplexOperation::Multiply, 0, 1, 0, 3, true, true).to_vec(),
            vec![0x62, 0xF6, 0x76, 0x9B, 0xD6, 0x04, 0x24],
        ),
        (
            stack_encoding(
                ComplexOperation::ConjugateAccumulate,
                16,
                17,
                1,
                7,
                false,
                false,
            )
            .to_vec(),
            vec![0x62, 0xE6, 0x77, 0x27, 0x56, 0x04, 0x24],
        ),
        (
            stack_encoding(ComplexOperation::Multiply, 31, 30, 2, 5, true, false).to_vec(),
            vec![0x62, 0x66, 0x0E, 0xC5, 0xD6, 0x3C, 0x24],
        ),
    ] {
        assert_eq!(actual, llvm);
    }
}

#[test]
fn packed_fp16_complex_memory_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let valid = memory_encoding(
        ComplexOperation::ConjugateAccumulate,
        0,
        1,
        0,
        1,
        false,
        false,
        3,
    )
    .to_vec();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x01), // map
        (2, 0x80), // W
        (2, 0x02), // F2/F3 selector changed to an unowned pp
        (4, 0x02), // non-owned opcode
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
    let mut aliased_destination_source1 = valid.clone();
    aliased_destination_source1[2] = (aliased_destination_source1[2] & 0x07) | 0x78;
    malformed.push(aliased_destination_source1);
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_fp16_complex_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    let encoding = X86InstructionBytes::new(&prefixed)
        .unwrap()
        .evex_packed_fp16_complex_memory_encoding()
        .expect("FS/address-size prefixes belong to helper address evaluation");
    let X86EvexPackedFp16ComplexMemoryReplay::MaskedVector { stack_instruction } = encoding.replay
    else {
        panic!("prefixed masked vector selected wrong replay")
    };
    assert_eq!(
        stack_instruction.as_slice(),
        stack_encoding(
            ComplexOperation::ConjugateAccumulate,
            0,
            1,
            0,
            1,
            false,
            false,
        )
    );
}

#[test]
fn packed_fp16_complex_apx_r16_r17_sib_address_lifts_admits_and_lowers_exactly() {
    // VFCMADDCPH xmm16{k1},xmm17,[r16+r17*2+16]. The disp8 value 1 is
    // compressed by the 16-byte full-vector tuple.
    let bytes = [0x62, 0xEE, 0x72, 0x01, 0x56, 0x44, 0x48, 0x01];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .expect("APX-extended packed FP16-complex memory source");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut base = SmirFunction::new(FunctionId(0), block.id, PC);
    base.add_block(block);
    base.x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    for level in LEVELS {
        let function = optimize(base.clone(), level);
        assert!(
            function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseIndexScale {
                        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
                        index: VReg::Arch(ArchReg::X86(X86Reg::R17)),
                        scale: 2,
                        disp: 16,
                        disp_size: DispSize::Disp8,
                    },
                    ..
                }
            )),
            "{level:?}: {:#?}",
            function.blocks[0].ops
        );
        let exact = sequence(&function, true).expect("APX-address sequence");
        assert_eq!(exact.address_offset, 2, "{level:?}");
        assert_eq!(exact.encoding.destination, 16, "{level:?}");
        assert_eq!(exact.encoding.source1, 17, "{level:?}");
        let X86EvexPackedFp16ComplexMemoryReplay::MaskedVector { stack_instruction } =
            exact.encoding.replay
        else {
            panic!("{level:?}: APX masked vector selected wrong replay")
        };
        let expected = [0x62, 0xE6, 0x76, 0x01, 0x56, 0x04, 0x24];
        assert_eq!(stack_instruction.as_slice(), expected, "{level:?}");

        let case = Fp16ComplexMemoryCase {
            operation: ComplexOperation::Accumulate,
            width: VecWidth::V128,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        };
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{level:?}: missing APX-address stack replay"
        );
    }
}

#[test]
fn all_216_packed_fp16_complex_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 216);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let sequence = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(sequence.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.destination,
                case.destination(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.source1, case.source1,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.zeroing,
                case.zeroing(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.accumulate,
                case.operation.accumulate(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.conjugate,
                case.operation.conjugate(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.memory_size,
                if case.broadcast() {
                    MemWidth::B4.bytes()
                } else {
                    case.width.bytes()
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.address_offset,
                match (case.form, case.control) {
                    (SourceForm::Vector, MaskControl::None) => 0,
                    (SourceForm::Vector, _) => 2,
                    (SourceForm::Broadcast, MaskControl::None) => 0,
                    (SourceForm::Broadcast, _) => 5,
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );

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
fn masked_vector_lowering_stages_disjoint_32_bit_pairs_and_rejects_avx_only_bridge() {
    let case = Fp16ComplexMemoryCase {
        operation: ComplexOperation::Multiply,
        width: VecWidth::V512,
        source1: 17,
        form: SourceForm::Vector,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let (code, _) = lower(&function, case);
    for lane in 0..16 {
        let lane_mask = (1u32 << lane).to_le_bytes();
        let guard = [
            0x9C,
            0x50,
            0xC4,
            0xE1,
            0xFB,
            0x93,
            0xC0 | case.mask(),
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
            "lane {lane}: {guard:02X?}"
        );
    }
    let allocate_frame = [0x48, 0x8D, 0x64, 0x24, 0xB0];
    let release_frame = [0x48, 0x8D, 0x64, 0x24, 0x50];
    assert_eq!(
        code.windows(allocate_frame.len())
            .filter(|window| *window == allocate_frame)
            .count(),
        1
    );
    assert_eq!(
        code.windows(release_frame.len())
            .filter(|window| *window == release_frame)
            .count(),
        17
    );
    assert!(
        code.windows(4)
            .any(|window| window == [0x8B, 0x44, 0x24, 0x48]),
        "32-bit pair helper return must be read from the disjoint staging slot"
    );
    assert!(
        code.windows(4)
            .any(|window| window == [0x89, 0x44, 0x24, 0x44]),
        "pair 15 must end at payload bytes 60..63"
    );

    let mut avx_only = X86_64Lowerer::new();
    avx_only.set_mem_helpers(true);
    avx_only.set_preserve_vector_mem_helpers(true);
    avx_only.set_avx_ymm16_vector_state(true);
    let error = avx_only
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject AVX-512-FP16 replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
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
fn packed_fp16_complex_memory_sequence_fails_closed_for_provenance_and_graph_mutations() {
    let cases = [
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::ConjugateAccumulate,
            width: VecWidth::V128,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::Accumulate,
            width: VecWidth::V256,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::ConjugateMultiply,
            width: VecWidth::V512,
            source1: 1,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::Multiply,
            width: VecWidth::V128,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ];
    for case in cases {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(
            sequence(&function, false).is_none(),
            "{case:?}: memory-disabled admission"
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let wrong_operation = match case.operation {
            ComplexOperation::ConjugateAccumulate => ComplexOperation::Accumulate,
            _ => ComplexOperation::ConjugateAccumulate,
        };
        wrong_provenance.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&memory_encoding(
                wrong_operation,
                case.destination(),
                case.source1,
                case.ll(),
                case.mask(),
                case.zeroing(),
                case.broadcast(),
                3,
            ))
            .unwrap(),
        );
        assert_rejected("wrong provenance", &wrong_provenance);

        let mut wrong_round = function.clone();
        let complex = wrong_round.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FP16Complex { .. }))
            .unwrap();
        let OpKind::X86FP16Complex { round, .. } = &mut complex.kind else {
            unreachable!()
        };
        *round = FpRoundMode::RoundNearest;
        assert_rejected("wrong rounding", &wrong_round);

        let mut wrong_source = function.clone();
        let complex = wrong_source.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FP16Complex { .. }))
            .unwrap();
        let OpKind::X86FP16Complex { src1, .. } = &mut complex.kind else {
            unreachable!()
        };
        *src1 = vector(if case.source1 == 1 { 2 } else { 1 }, case.width);
        assert_rejected("wrong source1", &wrong_source);

        let mut wrong_operation = function.clone();
        let complex = wrong_operation.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86FP16Complex { .. }))
            .unwrap();
        let OpKind::X86FP16Complex { accumulate, .. } = &mut complex.kind else {
            unreachable!()
        };
        *accumulate = !*accumulate;
        assert_rejected("wrong complex operation", &wrong_operation);

        let mut hinted_memory = function.clone();
        let address_index = sequence(&hinted_memory, true).unwrap().address_offset;
        hinted_memory.blocks[0].ops[address_index].x86_hint =
            Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("hinted memory", &hinted_memory);

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
        assert_rejected("same-PC tail", &tail);
    }
}

fn initial_registers(case: Fp16ComplexMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA5A5_0000_0000_0000u64
                ^ ((ordinal as u64) << 12)
                ^ (index as u64 * 0x0101_0101_0101_0101)
        }),
        zmm: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x3C00_BC00_3555_B555u64
                    ^ ((register as u64) << 48)
                    ^ ((word as u64) * 0x0001_0001_0001_0001)
            })
        }),
        k: std::array::from_fn(|index| {
            if index == 1 {
                0xA5A5_A5A5
            } else {
                0xF0F0_0000 ^ index as u64
            }
        }),
        rflags: 0x8D5,
        mxcsr: 0x1F80 | (((ordinal & 3) as u32) << 13),
        vector_active: X86_VECTOR_STATE_K64,
        ..GuestRegs::default()
    };
    registers.gpr[3] = 0x2000;
    registers.zmm[case.destination() as usize] = std::array::from_fn(|word| {
        0x4000_C000_3800_B800u64 ^ ((word as u64) * 0x0001_0001_0001_0001)
    });
    registers
}

fn memory_value(ordinal: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x3C00_4000_BC00_3555u64
            ^ ((ordinal as u64) << 32)
            ^ ((word as u64) * 0x0001_0001_0001_0001)
    })
}

fn interpreter_context(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

fn interpreter_registers(context: &SmirContext, initial: &GuestRegs) -> GuestRegs {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut result = *initial;
    result.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        result.zmm[index].copy_from_slice(&value[..8]);
    }
    result.k = x86.k;
    result.rflags = x86.rflags;
    result.mxcsr = x86.mxcsr;
    result
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: [u64; 8],
    case: Fp16ComplexMemoryCase,
) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x10000);
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(memory_value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    let size = if case.broadcast() {
        MemWidth::B4.bytes()
    } else {
        case.width.bytes()
    } as usize;
    memory.load(0x2000, &bytes[..size]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    interpreter_registers(&context, initial)
}

#[test]
fn packed_fp16_complex_memory_o0_o1_o2_interpretation_is_exactly_equivalent() {
    let cases = all_cases();
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_registers(case, ordinal);
        let memory = memory_value(ordinal);
        let expected = interpreter_success(&lift_case(case), &initial, memory, case);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, memory, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 216 * LEVELS.len());
}

#[test]
fn type_e4_pair_masks_suppress_inactive_faults_and_active_faults_do_not_commit() {
    let mut suppressions = 0usize;
    let mut active_faults = 0usize;
    let mut full_tuple_faults = 0usize;
    for operation in ComplexOperation::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in [MaskControl::Merge, MaskControl::Zero] {
                    let case = Fp16ComplexMemoryCase {
                        operation,
                        width,
                        source1: 17,
                        form,
                        control,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let value = memory_value(width.bytes() as usize);

                        let mut inactive = initial_registers(case, width.bytes() as usize);
                        inactive.k[usize::from(case.mask())] = 0;
                        let expected = interpreter_success(&function, &inactive, value, case);
                        let mut context = interpreter_context(&inactive);
                        let mut inaccessible = FlatMemory::new(0x2000);
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut inaccessible,
                            &function.blocks[0],
                        );
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::Return { .. })
                        ));
                        assert_eq!(
                            interpreter_registers(&context, &inactive),
                            expected,
                            "suppressed {level:?} {case:?}"
                        );
                        suppressions += 1;

                        let mut active = initial_registers(case, width.bytes() as usize + 1);
                        active.k[usize::from(case.mask())] = 0b100;
                        let mut context = interpreter_context(&active);
                        let limit = if case.broadcast() { 0x2000 } else { 0x2008 };
                        let mut partial = FlatMemory::new(limit);
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut partial,
                            &function.blocks[0],
                        );
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ));
                        assert_eq!(
                            interpreter_registers(&context, &active),
                            active,
                            "active fault {level:?} {case:?}"
                        );
                        active_faults += 1;
                    }
                }
            }

            let case = Fp16ComplexMemoryCase {
                operation,
                width,
                source1: 17,
                form: SourceForm::Vector,
                control: MaskControl::None,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let initial = initial_registers(case, width.bytes() as usize + 2);
                let mut context = interpreter_context(&initial);
                let mut partial = FlatMemory::new(0x2000 + width.bytes() as usize - 1);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut partial,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ));
                assert_eq!(interpreter_registers(&context, &initial), initial);
                full_tuple_faults += 1;
            }
        }
    }
    assert_eq!(suppressions, 4 * 3 * 2 * 2 * LEVELS.len());
    assert_eq!(active_faults, suppressions);
    assert_eq!(full_tuple_faults, 4 * 3 * LEVELS.len());
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
    fail_address: Option<u64>,
    calls: usize,
    addresses: [u64; 16],
}

#[cfg(target_arch = "x86_64")]
extern "C" fn lane_load_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    assert_eq!(size, 4);
    assert_eq!(signed, 0);
    context.addresses[context.calls] = address;
    context.calls += 1;
    if context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address - context.base).unwrap();
    assert!(offset + 4 <= context.value.len());
    LoadResult {
        value: u32::from_le_bytes(context.value[offset..offset + 4].try_into().unwrap()) as u64,
        ok: 1,
    }
}

#[cfg(target_arch = "x86_64")]
fn memory_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn expected_vector_scratch(value: [u64; 8], width: VecWidth) -> [u64; 8] {
    let words = (width.bytes() / 8) as usize;
    std::array::from_fn(|word| if word < words { value[word] } else { 0 })
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_packed_fp16_complex_memory_matches_interpretation_faults_and_mask_suppression() {
    use super::vex_fma3_memory_source::{VectorMemoryContext, vector_load_helper};
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!(
            "skipping native packed FP16 complex memory differential: \
             host lacks AVX-512F/BW/FP16"
        );
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let selected = [
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::ConjugateAccumulate,
            width: VecWidth::V128,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::Accumulate,
            width: VecWidth::V256,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::ConjugateMultiply,
            width: VecWidth::V512,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::Multiply,
            width: VecWidth::V128,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::ConjugateAccumulate,
            width: VecWidth::V256,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
        },
        Fp16ComplexMemoryCase {
            operation: ComplexOperation::Multiply,
            width: VecWidth::V512,
            source1: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
    ];

    let cases: Vec<_> = selected
        .into_iter()
        .filter(|case| case.width == VecWidth::V512 || has_vl)
        .collect();
    assert!(!cases.is_empty());
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let value = memory_value(ordinal);
            let bytes = memory_bytes(value);

            if case.form == SourceForm::Vector && case.control == MaskControl::None {
                let mut context = VectorMemoryContext {
                    value,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected = interpreter_success(&function, &registers, value, case);
                expected.vector_scratch = expected_vector_scratch(value, case.width);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: success");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, 0x2000, "{level:?} {case:?}");
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                    "{level:?} {case:?}"
                );
                assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
                assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
                successes += 1;

                let mut context = VectorMemoryContext {
                    value,
                    ok: 0,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal ^ 0x55);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: fault");
                assert_eq!(context.calls, 1, "{level:?} {case:?}: fault");
                faults += 1;
                continue;
            }

            let mut registers = initial_registers(case, ordinal);
            if case.mask() != 0 {
                registers.k[usize::from(case.mask())] = 0x5555_5555;
            }
            let active_mask = if case.mask() == 0 {
                (1u64 << (case.width.bytes() / 4)) - 1
            } else {
                registers.k[usize::from(case.mask())]
            };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: None,
                calls: 0,
                addresses: [0; 16],
            };
            registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
            registers.load_fn = lane_load_helper as *const () as usize as u64;
            let mut expected = interpreter_success(&function, &registers, value, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            let expected_addresses: Vec<u64> = if case.broadcast() {
                vec![0x2000]
            } else {
                (0..case.width.bytes() / 4)
                    .filter(|lane| active_mask & (1 << lane) != 0)
                    .map(|lane| 0x2000 + u64::from(lane) * 4)
                    .collect()
            };
            assert_eq!(
                &context.addresses[..context.calls],
                expected_addresses,
                "{level:?} {case:?}: active source addresses"
            );
            successes += 1;

            let mut registers = initial_registers(case, ordinal ^ 0x55);
            if case.mask() != 0 {
                registers.k[usize::from(case.mask())] = 0b1101;
            }
            let fail_address = if case.broadcast() { 0x2000 } else { 0x2008 };
            let mut context = LaneMemoryContext {
                base: 0x2000,
                value: bytes,
                fail_address: Some(fail_address),
                calls: 0,
                addresses: [0; 16],
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

            if case.mask() != 0 {
                let mut registers = initial_registers(case, ordinal ^ 0xAA);
                registers.k[usize::from(case.mask())] = 0;
                let mut context = LaneMemoryContext {
                    base: 0x2000,
                    value: bytes,
                    fail_address: Some(0x2000),
                    calls: 0,
                    addresses: [0; 16],
                };
                registers.ctx = (&mut context as *mut LaneMemoryContext) as u64;
                registers.load_fn = lane_load_helper as *const () as usize as u64;
                let mut expected = interpreter_success(&function, &registers, value, case);

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
    }
    assert!(successes >= 4);
    assert_eq!(successes, faults);
    assert!(suppressions >= 2);
}
