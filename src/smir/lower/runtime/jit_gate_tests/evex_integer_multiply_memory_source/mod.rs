//! Exact helper-backed EVEX packed-integer multiply memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexIntegerArithmeticMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexIntegerArithmeticMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_integer_arithmetic_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classifier;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MultiplyKind {
    SignedDwordToQword,
    UnsignedDwordToQword,
    RoundedHighSignedWord,
    HighUnsignedWord,
    HighSignedWord,
    LowWord,
    LowDword,
    LowQword,
}

impl MultiplyKind {
    const ALL: [Self; 8] = [
        Self::SignedDwordToQword,
        Self::UnsignedDwordToQword,
        Self::RoundedHighSignedWord,
        Self::HighUnsignedWord,
        Self::HighSignedWord,
        Self::LowWord,
        Self::LowDword,
        Self::LowQword,
    ];

    const fn map(self) -> X86VecMap {
        match self {
            Self::UnsignedDwordToQword
            | Self::HighUnsignedWord
            | Self::HighSignedWord
            | Self::LowWord => X86VecMap::Map0F,
            Self::SignedDwordToQword
            | Self::RoundedHighSignedWord
            | Self::LowDword
            | Self::LowQword => X86VecMap::Map0F38,
        }
    }

    const fn map_bits(self) -> u8 {
        match self.map() {
            X86VecMap::Map0F => 1,
            X86VecMap::Map0F38 => 2,
            _ => unreachable!(),
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::SignedDwordToQword => 0x28,
            Self::UnsignedDwordToQword => 0xF4,
            Self::RoundedHighSignedWord => 0x0B,
            Self::HighUnsignedWord => 0xE4,
            Self::HighSignedWord => 0xE5,
            Self::LowWord => 0xD5,
            Self::LowDword | Self::LowQword => 0x40,
        }
    }

    /// Result, writemask, and Type E4 memory-access granularity.
    const fn elem(self) -> VecElementType {
        match self {
            Self::RoundedHighSignedWord
            | Self::HighUnsignedWord
            | Self::HighSignedWord
            | Self::LowWord => VecElementType::I16,
            Self::LowDword => VecElementType::I32,
            Self::SignedDwordToQword | Self::UnsignedDwordToQword | Self::LowQword => {
                VecElementType::I64
            }
        }
    }

    const fn is_wig(self) -> bool {
        matches!(
            self,
            Self::RoundedHighSignedWord
                | Self::HighUnsignedWord
                | Self::HighSignedWord
                | Self::LowWord
        )
    }

    const fn fixed_w(self) -> bool {
        match self {
            Self::LowDword => false,
            Self::SignedDwordToQword | Self::UnsignedDwordToQword | Self::LowQword => true,
            _ => false,
        }
    }

    const fn allows_broadcast(self) -> bool {
        matches!(
            self,
            Self::SignedDwordToQword | Self::UnsignedDwordToQword | Self::LowDword | Self::LowQword
        )
    }

    const fn is_widening(self) -> bool {
        matches!(self, Self::SignedDwordToQword | Self::UnsignedDwordToQword)
    }

    const fn needs_avx512dq(self) -> bool {
        matches!(self, Self::LowQword)
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
struct MultiplyMemoryCase {
    kind: MultiplyKind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
    /// Raw EVEX.W for WIG word encodings; fixed-W forms retain their required
    /// value.
    w: bool,
}

impl MultiplyMemoryCase {
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

    const fn lanes(self) -> usize {
        self.width.lanes(self.kind.elem()) as usize
    }

    const fn memory_width(self) -> MemWidth {
        match self.kind.elem() {
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            VecElementType::I64 => MemWidth::B8,
            _ => unreachable!(),
        }
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding_with_controls(self, false, self.mask(), self.zeroing())
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.source1)
            .expect("two operands leave a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.broadcast() || self.mask() != 0 {
            stack_encoding(self)
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("EVEX packed-integer multiply vector width"),
    }))
}

fn memory_encoding_with_controls(
    case: MultiplyMemoryCase,
    sib: bool,
    mask: u8,
    zeroing: bool,
) -> Vec<u8> {
    assert!(case.destination < 32 && case.source1 < 32);
    assert!(mask < 8 && (!zeroing || mask != 0));
    assert!(!case.broadcast() || case.kind.allows_broadcast());
    assert!(case.kind.is_wig() || case.w == case.kind.fixed_w());
    let p0 = 0x60
        | case.kind.map_bits()
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(zeroing) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | mask;
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn stack_encoding(case: MultiplyMemoryCase) -> Vec<u8> {
    let p0 = 0x60
        | case.kind.map_bits()
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
}

fn register_encoding(case: MultiplyMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x40
        | case.kind.map_bits()
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | (((!case.source1) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | if case.source1 & 16 == 0 { 0x08 } else { 0 }
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        case.kind.opcode(),
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
        X86InstructionBytes::new(bytes).expect("EVEX multiply provenance"),
    );
    function
}

fn lift_case(case: MultiplyMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexIntegerArithmeticMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_integer_arithmetic_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: MultiplyMemoryCase) -> (Vec<u8>, usize) {
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
        requirements.needs_avx512dq,
        case.kind.needs_avx512dq(),
        "{case:?}"
    );
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (!case.kind.needs_avx512dq() || std::is_x86_feature_detected!("avx512dq"))
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
        .unwrap_or_else(|error| panic!("{case:?}: EVEX multiply lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX multiply"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<MultiplyMemoryCase> {
    let mut cases = Vec::new();
    for kind in MultiplyKind::ALL {
        for w in [false, true] {
            if !kind.is_wig() && w != kind.fixed_w() {
                continue;
            }
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                for (destination, source1) in [(0, 0), (9, 10), (17, 18)] {
                    for form in [SourceForm::Vector, SourceForm::Broadcast] {
                        if form == SourceForm::Broadcast && !kind.allows_broadcast() {
                            continue;
                        }
                        for control in MaskControl::ALL {
                            cases.push(MultiplyMemoryCase {
                                kind,
                                width,
                                destination,
                                source1,
                                form,
                                control,
                                w,
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
fn all_432_multiply_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 432);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert!(exact.encoding.is_integer_multiply(), "{case:?}");
            assert_eq!(exact.encoding.map, case.kind.map(), "{case:?}");
            assert_eq!(exact.encoding.opcode, case.kind.opcode(), "{case:?}");
            assert_eq!(exact.encoding.width, case.width, "{case:?}");
            assert_eq!(exact.encoding.elem, case.kind.elem(), "{case:?}");
            assert_eq!(exact.encoding.destination, case.destination, "{case:?}");
            assert_eq!(exact.encoding.source1, case.source1, "{case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{case:?}");
            assert_eq!(exact.encoding.w, case.w, "{case:?}");
            assert_eq!(
                exact.encoding.needs_avx512dq,
                case.kind.needs_avx512dq(),
                "{case:?}"
            );
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    case.memory_width().bytes()
                } else {
                    case.width.bytes()
                },
                "{case:?}"
            );
            assert_eq!(
                exact.consumed,
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
    assert_eq!(lowerings, 432 * LEVELS.len());
}

#[test]
fn multiply_type_e4_graphs_preserve_lane_and_single_broadcast_accesses() {
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
            assert_eq!(
                (ordinary_loads, pred_loads),
                match (case.control, case.form) {
                    (MaskControl::None, _) => (1, 0),
                    (_, SourceForm::Broadcast) => (0, 1),
                    (_, SourceForm::Vector) => (0, case.lanes()),
                },
                "{level:?} {case:?}: {:#?}",
                function.blocks[0].ops
            );
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
fn multiply_sequence_fails_closed_for_each_distinct_semantic_graph() {
    let cases = [
        MultiplyMemoryCase {
            kind: MultiplyKind::LowDword,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            w: false,
        },
        MultiplyMemoryCase {
            kind: MultiplyKind::HighSignedWord,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: true,
        },
        MultiplyMemoryCase {
            kind: MultiplyKind::SignedDwordToQword,
            width: VecWidth::V256,
            destination: 9,
            source1: 10,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: true,
        },
    ];
    for case in cases {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function, true).is_some(), "{case:?}");
        assert!(sequence(&function, false).is_none(), "{case:?}");

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[4] ^= 1;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong opcode provenance", &wrong_provenance);

        let mut wrong_address = function.clone();
        let memory_op = wrong_address.blocks[0]
            .ops
            .iter_mut()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::Load { .. }
                        | OpKind::VLoad { .. }
                        | OpKind::PredLoad { .. }
                        | OpKind::Lea { .. }
                )
            })
            .unwrap();
        match &mut memory_op.kind {
            OpKind::Load { addr, .. }
            | OpKind::VLoad { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::Lea { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
            }
            _ => unreachable!(),
        }
        assert_rejected("virtual address", &wrong_address);

        let mut hinted = function.clone();
        let arithmetic = hinted.blocks[0]
            .ops
            .iter_mut()
            .find(|op| match case.kind {
                MultiplyKind::LowWord | MultiplyKind::LowDword | MultiplyKind::LowQword => {
                    matches!(op.kind, OpKind::VMul { .. })
                }
                MultiplyKind::RoundedHighSignedWord
                | MultiplyKind::HighUnsignedWord
                | MultiplyKind::HighSignedWord => {
                    matches!(op.kind, OpKind::VMulShiftSat { .. })
                }
                MultiplyKind::SignedDwordToQword | MultiplyKind::UnsignedDwordToQword => {
                    matches!(op.kind, OpKind::MulS { .. } | OpKind::MulU { .. })
                }
            })
            .unwrap();
        arithmetic.x86_hint = Some(crate::smir::ir::ops::X86OpHint::MovImmModRm);
        assert_rejected("unexpected arithmetic hint", &hinted);

        let mut tail = function;
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
fn multiply_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = MultiplyMemoryCase {
        kind: MultiplyKind::SignedDwordToQword,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::None,
        w: true,
    };
    let broadcast_case = MultiplyMemoryCase {
        kind: MultiplyKind::LowQword,
        width: VecWidth::V256,
        destination: 9,
        source1: 10,
        form: SourceForm::Broadcast,
        control: MaskControl::Merge,
        w: true,
    };
    let high_word_case = MultiplyMemoryCase {
        kind: MultiplyKind::HighUnsignedWord,
        width: VecWidth::V128,
        destination: 1,
        source1: 2,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        w: true,
    };

    let mut rip = vector_case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = high_word_case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = broadcast_case.bytes();
    fs.insert(0, 0x64);
    let addresses = [
        (
            "RIP+disp32 widening multiply",
            vector_case,
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 10),
            },
        ),
        (
            "addr32 high-word multiply",
            high_word_case,
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS low-qword broadcast",
            broadcast_case,
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rdx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
    ];
    for (name, case, bytes, expected_address) in addresses {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. }
                    | OpKind::VLoad { addr, .. }
                    | OpKind::PredLoad { addr, .. }
                    | OpKind::Lea { addr, .. } => addr == &expected_address,
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

    for case in [vector_case, broadcast_case, high_word_case] {
        let mut bytes = memory_encoding_with_controls(case, true, case.mask(), case.zeroing());
        bytes[1] |= 0x08; // APX B4 extends SIB base RAX to R16.
        bytes[2] &= !0x04; // APX X4 extends SIB index RCX to R17.
        let expected_address = Address::BaseIndexScale {
            base: Some(x86(X86Reg::R16)),
            index: x86(X86Reg::R17),
            scale: 2,
            disp: 0,
            disp_size: DispSize::Auto,
        };
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
                    OpKind::Load { addr, .. }
                    | OpKind::VLoad { addr, .. }
                    | OpKind::PredLoad { addr, .. }
                    | OpKind::Lea { addr, .. } => addr == &expected_address,
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
fn multiply_rejects_the_avx_only_state_bridge() {
    let case = MultiplyMemoryCase {
        kind: MultiplyKind::LowQword,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        w: true,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(
        lowerer.lower_function(&function).is_err(),
        "AVX-only bridge admitted EVEX packed-integer multiply state"
    );
}
