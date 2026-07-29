//! Exact helper-backed AMD VEX FMA4 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86FmaOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FpRoundMode, FunctionId, MemWidth, OpId, SignExtend, VReg,
    VecElementType, VecWidth, VirtualId, X86FmaKind, X86FmaOrder, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::x86_host_has_fma4;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexFma4MemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_fma4_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

const PC: u64 = 0xF4A4;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const NATIVE_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const OPCODES: [u8; 20] = [
    0x5C, 0x5D, 0x5E, 0x5F, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x78, 0x79, 0x7A, 0x7B,
    0x7C, 0x7D, 0x7E, 0x7F,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryForm {
    Low,
    High,
    AllAlias,
    DestinationSource1Alias,
    DestinationIs4Alias,
    Source1Is4Alias,
    FsAddr32Sib,
    RipRelative,
}

impl MemoryForm {
    const ALL: [Self; 8] = [
        Self::Low,
        Self::High,
        Self::AllAlias,
        Self::DestinationSource1Alias,
        Self::DestinationIs4Alias,
        Self::Source1Is4Alias,
        Self::FsAddr32Sib,
        Self::RipRelative,
    ];

    #[cfg(target_arch = "x86_64")]
    const NATIVE: [Self; 4] = [Self::Low, Self::High, Self::AllAlias, Self::Source1Is4Alias];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fma4MemoryCase {
    opcode: u8,
    w: bool,
    encoded_256: bool,
    form: MemoryForm,
    ignored_low: u8,
}

impl Fma4MemoryCase {
    const fn destination(self) -> u8 {
        match self.form {
            MemoryForm::Low => 1,
            MemoryForm::High => 9,
            MemoryForm::AllAlias => 0,
            MemoryForm::DestinationSource1Alias => 15,
            MemoryForm::DestinationIs4Alias => 9,
            MemoryForm::Source1Is4Alias => 1,
            MemoryForm::FsAddr32Sib => 14,
            MemoryForm::RipRelative => 7,
        }
    }

    const fn source1(self) -> u8 {
        match self.form {
            MemoryForm::Low => 2,
            MemoryForm::High => 10,
            MemoryForm::AllAlias => 0,
            MemoryForm::DestinationSource1Alias => 15,
            MemoryForm::DestinationIs4Alias => 10,
            MemoryForm::Source1Is4Alias => 2,
            MemoryForm::FsAddr32Sib => 10,
            MemoryForm::RipRelative => 8,
        }
    }

    const fn is4(self) -> u8 {
        match self.form {
            MemoryForm::Low => 3,
            MemoryForm::High => 12,
            MemoryForm::AllAlias => 0,
            MemoryForm::DestinationSource1Alias => 14,
            MemoryForm::DestinationIs4Alias => 9,
            MemoryForm::Source1Is4Alias => 2,
            MemoryForm::FsAddr32Sib => 12,
            MemoryForm::RipRelative => 9,
        }
    }

    const fn base(self) -> Option<u8> {
        match self.form {
            MemoryForm::Low | MemoryForm::AllAlias | MemoryForm::Source1Is4Alias => Some(7),
            MemoryForm::High
            | MemoryForm::DestinationSource1Alias
            | MemoryForm::DestinationIs4Alias => Some(11),
            MemoryForm::FsAddr32Sib => Some(14),
            MemoryForm::RipRelative => None,
        }
    }

    fn spec(self) -> (VecElementType, X86FmaKind, bool) {
        let elem = if self.opcode & 1 == 0 {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let (kind, scalar) = match self.opcode {
            0x5C | 0x5D => (X86FmaKind::AddSub, false),
            0x5E | 0x5F => (X86FmaKind::SubAdd, false),
            0x68 | 0x69 => (X86FmaKind::Add, false),
            0x6A | 0x6B => (X86FmaKind::Add, true),
            0x6C | 0x6D => (X86FmaKind::Sub, false),
            0x6E | 0x6F => (X86FmaKind::Sub, true),
            0x78 | 0x79 => (X86FmaKind::NegativeMultiplyAdd, false),
            0x7A | 0x7B => (X86FmaKind::NegativeMultiplyAdd, true),
            0x7C | 0x7D => (X86FmaKind::NegativeMultiplySub, false),
            0x7E | 0x7F => (X86FmaKind::NegativeMultiplySub, true),
            _ => unreachable!("FMA4 test opcode"),
        };
        (elem, kind, scalar)
    }

    fn width(self) -> VecWidth {
        if self.spec().2 || !self.encoded_256 {
            VecWidth::V128
        } else {
            VecWidth::V256
        }
    }

    fn memory_size(self) -> u32 {
        if self.spec().2 {
            self.spec().0.bytes() as u32
        } else {
            self.width().bytes()
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| {
                *candidate != self.destination()
                    && *candidate != self.source1()
                    && *candidate != self.is4()
            })
            .expect("three FMA4 operands leave at least thirteen scratch registers")
    }

    fn vex_p1(self) -> u8 {
        (u8::from(self.w) << 7)
            | (((!self.source1()) & 0x0F) << 3)
            | (u8::from(self.encoded_256) << 2)
            | 1
    }

    fn bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let reg = (destination & 7) << 3;
        let is4 = (self.is4() << 4) | (self.ignored_low & 0x0F);
        match self.form {
            MemoryForm::FsAddr32Sib => {
                // FS addr32 [r14d+r15d*2+0x44332211].
                vec![
                    0x64,
                    0x67,
                    0xC4,
                    (if destination < 8 { 0x80 } else { 0 }) | 3,
                    self.vex_p1(),
                    self.opcode,
                    0x80 | reg | 4,
                    0x7E,
                    0x11,
                    0x22,
                    0x33,
                    0x44,
                    is4,
                ]
            }
            MemoryForm::RipRelative => {
                let mut bytes = vec![
                    0xC4,
                    (if destination < 8 { 0x80 } else { 0 }) | 0x60 | 3,
                    self.vex_p1(),
                    self.opcode,
                    reg | 5,
                ];
                bytes.extend_from_slice(&(DISP as i32).to_le_bytes());
                bytes.push(is4);
                bytes
            }
            _ => {
                let base = self.base().unwrap();
                vec![
                    0xC4,
                    (if destination < 8 { 0x80 } else { 0 })
                        | 0x40
                        | (if base < 8 { 0x20 } else { 0 })
                        | 3,
                    self.vex_p1(),
                    self.opcode,
                    0x40 | reg | (base & 7),
                    DISP as u8,
                    is4,
                ]
            }
        }
    }

    fn register_bytes(self) -> [u8; 6] {
        let destination = self.destination();
        let scratch = self.scratch();
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 3,
            self.vex_p1(),
            self.opcode,
            0xC0 | ((destination & 7) << 3) | (scratch & 7),
            (self.is4() << 4) | (self.ignored_low & 0x0F),
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("FMA4 test width"),
    }))
}

fn all_cases() -> Vec<Fma4MemoryCase> {
    let mut cases = Vec::with_capacity(OPCODES.len() * 2 * 2 * MemoryForm::ALL.len());
    let mut ordinal = 0usize;
    for opcode in OPCODES {
        for w in [false, true] {
            for encoded_256 in [false, true] {
                for form in MemoryForm::ALL {
                    cases.push(Fma4MemoryCase {
                        opcode,
                        w,
                        encoded_256,
                        form,
                        ignored_low: ordinal as u8 & 0x0F,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<Fma4MemoryCase> {
    all_cases()
        .into_iter()
        .filter(|case| MemoryForm::NATIVE.contains(&case.form))
        .collect()
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("FMA4 instruction provenance"),
    );
    function
}

fn lift_case(case: Fma4MemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
        .expect("FMA4 memory load")
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
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

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexFma4MemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_fma4_memory_sequence(
        block,
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_graph(function: &SmirFunction, case: Fma4MemoryCase) {
    let index = sequence_index(function);
    let block = &function.blocks[0];
    let sequence = classified_sequence(function, true)
        .unwrap_or_else(|| panic!("unclassified exact FMA4 graph: {case:?}"));
    let (elem, kind, scalar) = case.spec();
    assert_eq!(sequence.consumed, if scalar { 4 } else { 3 }, "{case:?}");
    assert_eq!(sequence.encoding.width, case.width(), "{case:?}");
    assert_eq!(sequence.encoding.encoded_256, case.encoded_256, "{case:?}");
    assert_eq!(sequence.encoding.elem, elem, "{case:?}");
    assert_eq!(sequence.encoding.kind, kind, "{case:?}");
    assert_eq!(sequence.encoding.scalar, scalar, "{case:?}");
    assert_eq!(
        sequence.encoding.destination,
        case.destination(),
        "{case:?}"
    );
    assert_eq!(sequence.encoding.source1, case.source1(), "{case:?}");
    assert_eq!(sequence.encoding.is4, case.is4(), "{case:?}");
    assert_eq!(sequence.encoding.scratch, case.scratch(), "{case:?}");
    assert_eq!(sequence.encoding.opcode, case.opcode, "{case:?}");
    assert_eq!(sequence.encoding.w, case.w, "{case:?}");
    assert_eq!(
        sequence.encoding.memory_size,
        case.memory_size(),
        "{case:?}"
    );
    assert_eq!(
        sequence.encoding.register_instruction.as_slice(),
        case.register_bytes(),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");

    let fma_offset = if scalar { 2 } else { 1 };
    let memory_vector = if scalar {
        let loaded_scalar = match block.ops[index].kind {
            OpKind::Load {
                dst,
                width,
                sign: SignExtend::Zero,
                ..
            } => {
                assert_eq!(
                    width,
                    if elem == VecElementType::F32 {
                        MemWidth::B4
                    } else {
                        MemWidth::B8
                    },
                    "{case:?}"
                );
                dst
            }
            ref other => panic!("{case:?}: expected scalar Load, got {other:?}"),
        };
        match block.ops[index + 1].kind {
            OpKind::VBroadcast {
                dst,
                scalar: actual_scalar,
                elem: actual_elem,
                lanes: 1,
            } => {
                assert_eq!(actual_scalar, loaded_scalar, "{case:?}");
                assert_eq!(actual_elem, elem, "{case:?}");
                dst
            }
            ref other => panic!("{case:?}: expected scalar VBroadcast, got {other:?}"),
        }
    } else {
        match block.ops[index].kind {
            OpKind::VLoad { dst, width, .. } => {
                assert_eq!(width, case.width(), "{case:?}");
                dst
            }
            ref other => panic!("{case:?}: expected packed VLoad, got {other:?}"),
        }
    };
    let fma = &block.ops[index + fma_offset];
    let OpKind::X86Fma(X86FmaOp {
        dst: raw,
        src1,
        src2,
        src3,
        mask,
        elem: actual_elem,
        kind: actual_kind,
        order,
        round,
        lanes,
    }) = fma.kind
    else {
        panic!("{case:?}: expected X86Fma, got {:?}", fma.kind)
    };
    assert_eq!(src1, vector(case.source1(), case.width()), "{case:?}");
    assert_eq!(
        src2,
        if case.w {
            vector(case.is4(), case.width())
        } else {
            memory_vector
        },
        "{case:?}"
    );
    assert_eq!(
        src3,
        if case.w {
            memory_vector
        } else {
            vector(case.is4(), case.width())
        },
        "{case:?}"
    );
    assert_eq!(mask, None, "{case:?}");
    assert_eq!(actual_elem, elem, "{case:?}");
    assert_eq!(actual_kind, kind, "{case:?}");
    assert_eq!(order, X86FmaOrder::Order123, "{case:?}");
    assert_eq!(round, FpRoundMode::Dynamic, "{case:?}");
    assert_eq!(
        lanes,
        if scalar {
            1
        } else {
            case.width().lanes(elem) as u8
        },
        "{case:?}"
    );
    assert_eq!(
        fma.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F3A,
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: case.width(),
            w: case.w,
        }),
        "{case:?}"
    );
    assert!(matches!(
        block.ops[index + fma_offset + 1].kind,
        OpKind::VMov {
            dst,
            src,
            width,
        } if dst == vector(case.destination(), case.width())
            && src == raw
            && width == case.width()
    ));
}

fn lower(
    function: &SmirFunction,
    case: Fma4MemoryCase,
) -> (Vec<u8>, usize, X86JitVexFma4MemorySequence) {
    let excluded = HashMap::new();
    let sequence = classified_sequence(function, true).expect("classified FMA4 sequence");
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(requirements.needs_fma4, "{case:?}");
    assert!(!requirements.needs_xop, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx") && x86_host_has_fma4(),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed FMA4 lowering failed: {error:?}"));
    assert!(lowered.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize helper-backed FMA4"),
        lowered.entry_offset,
        sequence,
    )
}

#[test]
fn all_1_920_family_w_l_alias_address_and_optimization_cells_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 20 * 2 * 2 * 8);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_graph(&function, case);
            let (code, _, sequence) = lower(&function, case);
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.register_bytes(),
                "{level:?} {case:?}"
            );
            assert!(
                code.windows(6)
                    .any(|window| window == case.register_bytes()),
                "{level:?} {case:?}: missing {:02X?}",
                case.register_bytes()
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector index"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector scratch transfer"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 1_920);
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed FMA4 graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed FMA4 graph"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed FMA4 graph"
    );
}

fn replace_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated FMA4 metadata"),
    );
}

#[test]
fn classifier_gate_and_lowerer_fail_closed_for_every_common_graph_invariant() {
    let case = Fma4MemoryCase {
        opcode: 0x68,
        w: false,
        encoded_256: true,
        form: MemoryForm::High,
        ignored_low: 5,
    };
    let base = lift_case(case);
    let index = sequence_index(&base);
    assert_eq!(index, 0);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let raw = match base.blocks[0].ops[1].kind {
        OpKind::X86Fma(X86FmaOp { dst, .. }) => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();
    malformed.push(("missing metadata", missing_metadata));

    for (name, byte_index, xor) in [
        ("encoded destination", 4usize, 0x08u8),
        ("encoded source1", 2, 0x08),
        ("encoded W", 2, 0x80),
        ("encoded L", 2, 0x04),
        ("encoded opcode", 3, 0x01),
        ("encoded is4", 6, 0x10),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    malformed.push(("load hint", load_hint));

    let mut load_architectural = base.clone();
    if let OpKind::VLoad { dst, .. } = &mut load_architectural.blocks[0].ops[0].kind {
        *dst = vector(4, VecWidth::V256);
    }
    malformed.push(("architectural load result", load_architectural));

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", load_width));

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address", virtual_address));

    let mut loaded_escape = base.clone();
    loaded_escape.blocks[0].ops.push(SmirOp::new(
        OpId(0x7000),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: loaded,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value escapes", loaded_escape));

    let mut fma_pc = base.clone();
    fma_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("FMA PC", fma_pc));

    let mut fma_hint = base.clone();
    fma_hint.blocks[0].ops[1].x86_hint = None;
    malformed.push(("FMA hint", fma_hint));

    let source_mutations: [(&str, fn(&mut X86FmaOp)); 3] = [
        ("FMA source1", |operation: &mut X86FmaOp| {
            operation.src1 = vector(4, VecWidth::V256)
        }),
        ("FMA source2", |operation: &mut X86FmaOp| {
            operation.src2 = vector(4, VecWidth::V256)
        }),
        ("FMA source3", |operation: &mut X86FmaOp| {
            operation.src3 = vector(4, VecWidth::V256)
        }),
    ];
    for (name, mutate) in source_mutations {
        let mut function = base.clone();
        if let OpKind::X86Fma(operation) = &mut function.blocks[0].ops[1].kind {
            mutate(operation);
        }
        malformed.push((name, function));
    }

    let mut masked = base.clone();
    if let OpKind::X86Fma(operation) = &mut masked.blocks[0].ops[1].kind {
        operation.mask = Some(VReg::Arch(ArchReg::X86(X86Reg::K(1))));
    }
    malformed.push(("FMA mask", masked));

    let mut wrong_element = base.clone();
    if let OpKind::X86Fma(operation) = &mut wrong_element.blocks[0].ops[1].kind {
        operation.elem = VecElementType::F64;
    }
    malformed.push(("FMA element", wrong_element));

    let mut wrong_kind = base.clone();
    if let OpKind::X86Fma(operation) = &mut wrong_kind.blocks[0].ops[1].kind {
        operation.kind = X86FmaKind::Sub;
    }
    malformed.push(("FMA kind", wrong_kind));

    let mut wrong_order = base.clone();
    if let OpKind::X86Fma(operation) = &mut wrong_order.blocks[0].ops[1].kind {
        operation.order = X86FmaOrder::Order231;
    }
    malformed.push(("FMA order", wrong_order));

    let mut wrong_round = base.clone();
    if let OpKind::X86Fma(operation) = &mut wrong_round.blocks[0].ops[1].kind {
        operation.round = FpRoundMode::RoundUp;
    }
    malformed.push(("FMA rounding", wrong_round));

    let mut wrong_lanes = base.clone();
    if let OpKind::X86Fma(operation) = &mut wrong_lanes.blocks[0].ops[1].kind {
        operation.lanes -= 1;
    }
    malformed.push(("FMA lanes", wrong_lanes));

    let mut raw_escape = base.clone();
    raw_escape.blocks[0].ops.push(SmirOp::new(
        OpId(0x7001),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: raw,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("raw value escapes", raw_escape));

    let mut result_pc = base.clone();
    result_pc.blocks[0].ops[2].guest_pc += 1;
    malformed.push(("result PC", result_pc));

    let mut result_hint = base.clone();
    result_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    malformed.push(("result hint", result_hint));

    let mut result_destination = base.clone();
    if let OpKind::VMov { dst, .. } = &mut result_destination.blocks[0].ops[2].kind {
        *dst = vector(4, VecWidth::V256);
    }
    malformed.push(("result destination", result_destination));

    let mut result_source = base.clone();
    if let OpKind::VMov { src, .. } = &mut result_source.blocks[0].ops[2].kind {
        *src = loaded;
    }
    malformed.push(("result source", result_source));

    let mut result_width = base.clone();
    if let OpKind::VMov { width, .. } = &mut result_width.blocks[0].ops[2].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("result width", result_width));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7002), PC, OpKind::Nop));
    malformed.push(("same-PC tail", same_pc_tail));

    let mut missing_result = base;
    missing_result.blocks[0].ops.pop();
    malformed.push(("missing result", missing_result));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[test]
fn scalar_load_and_broadcast_invariants_are_fail_closed() {
    let case = Fma4MemoryCase {
        opcode: 0x7F,
        w: true,
        encoded_256: true,
        form: MemoryForm::Low,
        ignored_low: 0x0F,
    };
    let base = lift_case(case);
    let scalar = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let loaded = match base.blocks[0].ops[1].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B4;
    }
    malformed.push(("scalar load width", load_width));

    let mut load_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut load_sign.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("scalar load extension", load_sign));

    let mut scalar_escape = base.clone();
    scalar_escape.blocks[0].ops.push(SmirOp::new(
        OpId(0x7100),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: scalar,
            width: VecWidth::V128,
        },
    ));
    malformed.push(("scalar escapes", scalar_escape));

    let mut broadcast_pc = base.clone();
    broadcast_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("broadcast PC", broadcast_pc));

    let mut broadcast_hint = base.clone();
    broadcast_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(
        crate::smir::ir::ops::X86VecAlign::Unaligned,
    ));
    malformed.push(("broadcast hint", broadcast_hint));

    let mut broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut broadcast_scalar.blocks[0].ops[1].kind {
        *scalar = VReg::Virtual(VirtualId(0xFFFF));
    }
    malformed.push(("broadcast scalar", broadcast_scalar));

    let mut broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut broadcast_element.blocks[0].ops[1].kind {
        *elem = VecElementType::F32;
    }
    malformed.push(("broadcast element", broadcast_element));

    let mut broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }
    malformed.push(("broadcast lanes", broadcast_lanes));

    let mut loaded_escape = base;
    loaded_escape.blocks[0].ops.push(SmirOp::new(
        OpId(0x7101),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V128),
            src: loaded,
            width: VecWidth::V128,
        },
    ));
    malformed.push(("broadcast value escapes", loaded_escape));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}
