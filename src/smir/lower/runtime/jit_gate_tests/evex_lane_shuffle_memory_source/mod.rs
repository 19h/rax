//! Exact helper-backed EVEX VPSHUFD/VPSHUFHW/VPSHUFLW memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SourceArch,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexLaneShuffleKind, X86EvexLaneShuffleMemoryReplay,
    X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexLaneShuffleMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_lane_shuffle_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x70_60;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShuffleKind {
    Dword,
    HighWord,
    LowWord,
}

impl ShuffleKind {
    const ALL: [Self; 3] = [Self::Dword, Self::HighWord, Self::LowWord];

    const fn pp(self) -> u8 {
        match self {
            Self::Dword => 1,
            Self::HighWord => 2,
            Self::LowWord => 3,
        }
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::Dword => VecElementType::I32,
            Self::HighWord | Self::LowWord => VecElementType::I16,
        }
    }

    const fn classified(self) -> X86EvexLaneShuffleKind {
        match self {
            Self::Dword => X86EvexLaneShuffleKind::Dword,
            Self::HighWord => X86EvexLaneShuffleKind::HighWord,
            Self::LowWord => X86EvexLaneShuffleKind::LowWord,
        }
    }

    const fn supports_broadcast(self) -> bool {
        matches!(self, Self::Dword)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TupleKind {
    Full,
    Broadcast,
}

impl TupleKind {
    const fn is_broadcast(self) -> bool {
        matches!(self, Self::Broadcast)
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
struct LaneShuffleMemoryCase {
    kind: ShuffleKind,
    width: VecWidth,
    w: bool,
    destination: u8,
    control: MaskControl,
    tuple: TupleKind,
    immediate: u8,
}

impl LaneShuffleMemoryCase {
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

    fn assert_valid(self) {
        assert!(self.destination < 32);
        assert!(self.mask() < 8 && (!self.zeroing() || self.mask() != 0));
        assert!(!self.w || self.kind != ShuffleKind::Dword);
        assert!(!self.tuple.is_broadcast() || self.kind.supports_broadcast());
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination)
            .expect("one destination leaves a low vector scratch")
    }

    const fn memory_size(self) -> u32 {
        if self.tuple.is_broadcast() {
            4
        } else {
            self.width.bytes()
        }
    }

    fn expected_replay(self) -> Vec<u8> {
        if self.tuple.is_broadcast() {
            stack_encoding(self)
        } else {
            register_encoding(self, self.scratch())
        }
    }
}

fn memory_encoding(case: LaneShuffleMemoryCase, sib: bool) -> Vec<u8> {
    case.assert_valid();
    let p0 = 0x61
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | 0x7C | case.kind.pp();
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.tuple.is_broadcast()) << 4)
        | 0x08
        | case.mask();
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        0x70,
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2], with APX B4/X4 injected independently by tests.
        bytes.push(0x48);
    }
    bytes.push(case.immediate);
    bytes
}

fn register_encoding(case: LaneShuffleMemoryCase, scratch: u8) -> Vec<u8> {
    case.assert_valid();
    assert!(scratch < 16);
    let p0 = 0x41
        | if case.destination & 8 == 0 { 0x80 } else { 0 }
        | if case.destination & 16 == 0 { 0x10 } else { 0 }
        | if scratch & 8 == 0 { 0x20 } else { 0 };
    let p1 = (u8::from(case.w) << 7) | 0x7C | case.kind.pp();
    let p2 = (u8::from(case.zeroing()) << 7) | (case.ll() << 5) | 0x08 | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x70,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
        case.immediate,
    ]
}

fn stack_encoding(case: LaneShuffleMemoryCase) -> Vec<u8> {
    let mut bytes = memory_encoding(case, false);
    bytes[1] = (bytes[1] & 0x97) | 0x60;
    bytes[2] |= 0x04;
    bytes[5] = (bytes[5] & 0x38) | 0x04;
    bytes.insert(bytes.len() - 1, 0x24);
    bytes
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
        X86InstructionBytes::new(bytes).expect("EVEX lane-shuffle provenance"),
    );
    function
}

fn lift_case(case: LaneShuffleMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexLaneShuffleMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    x86_jit_evex_lane_shuffle_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: LaneShuffleMemoryCase) -> (Vec<u8>, usize) {
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
        requirements.has_k16_opmask_span,
        case.mask() != 0 && case.width.lanes(case.kind.elem()) <= 16,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_avx512bitalg, "{case:?}");
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
        .unwrap_or_else(|error| panic!("{case:?}: EVEX lane-shuffle lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX lane shuffle"),
        result.entry_offset,
    )
}

fn valid_forms(kind: ShuffleKind) -> Vec<(bool, TupleKind)> {
    match kind {
        ShuffleKind::Dword => vec![(false, TupleKind::Full), (false, TupleKind::Broadcast)],
        ShuffleKind::HighWord | ShuffleKind::LowWord => {
            vec![(false, TupleKind::Full), (true, TupleKind::Full)]
        }
    }
}

fn case_immediate(kind: ShuffleKind, width: VecWidth, w: bool, control: MaskControl) -> u8 {
    kind.pp()
        .wrapping_mul(0x39)
        .wrapping_add(width.bytes() as u8)
        .wrapping_add(u8::from(w).wrapping_mul(0xA7))
        .wrapping_add(control.fields().0.wrapping_mul(0x13))
}

fn all_cases() -> Vec<LaneShuffleMemoryCase> {
    let mut cases = Vec::new();
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (w, tuple) in valid_forms(kind) {
                for control in MaskControl::ALL {
                    let ordinal = cases.len() as u8;
                    cases.push(LaneShuffleMemoryCase {
                        kind,
                        width,
                        w,
                        destination: [1, 9, 17, 25][usize::from(ordinal & 3)],
                        control,
                        tuple,
                        immediate: case_immediate(kind, width, w, control),
                    });
                }
            }
        }
    }
    cases
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
fn all_54_lane_shuffle_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width);
            assert_eq!(exact.encoding.kind, case.kind.classified());
            assert_eq!(exact.encoding.destination, case.destination);
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing());
            assert_eq!(exact.encoding.immediate, case.immediate);
            assert_eq!(exact.encoding.w, case.w);
            assert_eq!(exact.encoding.memory_size, case.memory_size());
            assert_eq!(exact.consumed, function.blocks[0].ops.len());
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::VShuffle { .. }))
                    .count(),
                1,
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
    assert_eq!(lowerings, 54 * LEVELS.len());
}

#[test]
fn e4nf_lane_shuffle_graphs_always_preserve_one_exact_tuple_access() {
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let scalar_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| {
                    matches!(
                        op.kind,
                        OpKind::Load {
                            width: MemWidth::B4,
                            ..
                        }
                    )
                })
                .count();
            let vector_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VLoad { width, .. } if width == case.width))
                .count();
            let expected = if case.tuple.is_broadcast() {
                (1, 0)
            } else {
                (0, 1)
            };
            assert_eq!((scalar_loads, vector_loads), expected, "{level:?} {case:?}");
        }
    }
}

#[test]
fn lane_shuffle_sequence_fails_closed_for_provenance_selector_mask_and_ssa_mutations() {
    let case = LaneShuffleMemoryCase {
        kind: ShuffleKind::HighWord,
        width: VecWidth::V256,
        w: true,
        destination: 9,
        control: MaskControl::Zero,
        tuple: TupleKind::Full,
        immediate: 0xA5,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, false).is_none());

    let mut mutations = Vec::<(&str, SmirFunction)>::new();
    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    mutations.push(("missing provenance", missing_provenance));

    let mut wrong_immediate = function.clone();
    let mut bytes = case.bytes();
    *bytes.last_mut().unwrap() ^= 1;
    wrong_immediate
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("imm8 provenance", wrong_immediate));

    let mut spurious_apx_guard = function.clone();
    spurious_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0xFFF0), PC, OpKind::X86RequireApx));
    mutations.push(("spurious APX guard", spurious_apx_guard));

    let shuffle_index = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VShuffle { .. }))
        .unwrap();
    let first_selector = function.blocks[0]
        .ops
        .iter()
        .position(
            |op| matches!(op.kind, OpKind::Mov { src: SrcOperand::Imm(value), .. } if value != 0),
        )
        .unwrap();

    let mut wrong_selector = function.clone();
    if let OpKind::Mov {
        src: SrcOperand::Imm(value),
        ..
    } = &mut wrong_selector.blocks[0].ops[first_selector].kind
    {
        *value ^= 1;
    }
    mutations.push(("selector immediate", wrong_selector));

    let mut wrong_source = function.clone();
    if let OpKind::VShuffle { src1, .. } = &mut wrong_source.blocks[0].ops[shuffle_index].kind {
        *src1 = VReg::Arch(ArchReg::X86(X86Reg::Ymm(11)));
    }
    mutations.push(("shuffle source", wrong_source));

    let mut escaped_raw = function.clone();
    let raw = match escaped_raw.blocks[0].ops[shuffle_index].kind {
        OpKind::VShuffle { dst, .. } => dst,
        _ => unreachable!(),
    };
    escaped_raw.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF1),
        PC + 1,
        OpKind::VMov {
            dst: VReg::Virtual(VirtualId(0xFFF1)),
            src: raw,
            width: case.width,
        },
    ));
    mutations.push(("escaped raw result", escaped_raw));

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
        OpId(0xFFF2),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFF2)),
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
fn lane_shuffle_full_tuple_and_unmasked_commit_fail_closed_under_mutation() {
    let case = LaneShuffleMemoryCase {
        kind: ShuffleKind::LowWord,
        width: VecWidth::V512,
        w: true,
        destination: 17,
        control: MaskControl::None,
        tuple: TupleKind::Full,
        immediate: 0x1B,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let load_index = function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .unwrap();
    let commit_index = function.blocks[0]
        .ops
        .iter()
        .rposition(|op| matches!(op.kind, OpKind::VMov { .. }))
        .unwrap();

    let mut mutations = Vec::<(&str, SmirFunction)>::new();
    let mut wrong_kind = function.clone();
    let mut bytes = case.bytes();
    bytes[2] ^= 0x01;
    wrong_kind
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    mutations.push(("shuffle-kind provenance", wrong_kind));

    let mut missing_alignment = function.clone();
    missing_alignment.blocks[0].ops[load_index].x86_hint = None;
    mutations.push(("full tuple alignment hint", missing_alignment));

    let mut wrong_commit = function.clone();
    if let OpKind::VMov { dst, .. } = &mut wrong_commit.blocks[0].ops[commit_index].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Zmm(19)));
    }
    mutations.push(("unmasked destination commit", wrong_commit));

    for (name, mutated) in mutations {
        assert_rejected(name, &mutated);
    }
}

#[test]
fn lane_shuffle_segment_addr32_rip_disp8_and_apx_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let full = LaneShuffleMemoryCase {
        kind: ShuffleKind::HighWord,
        width: VecWidth::V512,
        w: true,
        destination: 17,
        control: MaskControl::Merge,
        tuple: TupleKind::Full,
        immediate: 0xE4,
    };
    let broadcast = LaneShuffleMemoryCase {
        kind: ShuffleKind::Dword,
        w: false,
        tuple: TupleKind::Broadcast,
        immediate: 0xA5,
        ..full
    };

    let mut rip = full.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    for byte in 0x20u32.to_le_bytes() {
        rip.insert(rip.len() - 1, byte);
    }
    let mut addr32 = full.bytes();
    addr32.insert(0, 0x67);
    let mut fs = broadcast.bytes();
    fs.insert(0, 0x64);
    let mut full_disp8 = full.bytes();
    full_disp8[5] = (full_disp8[5] & 0x38) | 0x43;
    full_disp8.insert(full_disp8.len() - 1, 0xFE);
    let mut broadcast_disp8 = broadcast.bytes();
    broadcast_disp8[5] = (broadcast_disp8[5] & 0x38) | 0x43;
    broadcast_disp8.insert(broadcast_disp8.len() - 1, 3);

    let address_cases = [
        (
            "RIP+disp32 full",
            full,
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 11),
            },
        ),
        (
            "addr32 full",
            full,
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS broadcast",
            broadcast,
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
            "compressed disp8 full",
            full,
            full_disp8,
            Address::BaseOffset {
                base: x86(X86Reg::Rbx),
                offset: -2 * i64::from(full.width.bytes()),
                disp_size: DispSize::Disp8,
            },
        ),
        (
            "compressed disp8 broadcast",
            broadcast,
            broadcast_disp8,
            Address::BaseOffset {
                base: x86(X86Reg::Rbx),
                offset: 3 * i64::from(MemWidth::B4.bytes()),
                disp_size: DispSize::Disp8,
            },
        ),
    ];

    for (name, case, bytes, expected_address) in address_cases {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::VLoad { addr, .. } | OpKind::Load { addr, .. } =>
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

    for case in [full, broadcast] {
        let mut apx = memory_encoding(case, true);
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
            assert!(matches!(
                function.blocks[0].ops.first().map(|op| &op.kind),
                Some(OpKind::X86RequireApx)
            ));
            assert!(function.blocks[0].ops.iter().any(|op| match &op.kind {
                OpKind::VLoad { addr, .. } | OpKind::Load { addr, .. } => addr == &expected_address,
                _ => false,
            }));
            sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {apx:02X?}"));
            lower(&function, case);
        }
        let mut missing_guard = optimize(base, OptLevel::O2);
        missing_guard.blocks[0].ops.remove(0);
        assert_rejected("APX address without its dynamic guard", &missing_guard);
    }
}

#[test]
fn lane_shuffle_rejects_the_avx_only_state_bridge() {
    for case in [
        LaneShuffleMemoryCase {
            kind: ShuffleKind::LowWord,
            width: VecWidth::V512,
            w: true,
            destination: 17,
            control: MaskControl::Zero,
            tuple: TupleKind::Full,
            immediate: 0xA5,
        },
        LaneShuffleMemoryCase {
            kind: ShuffleKind::Dword,
            tuple: TupleKind::Broadcast,
            w: false,
            ..all_cases()[0]
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_preserve_vector_mem_helpers(true);
        lowerer.set_avx_ymm16_vector_state(true);
        let error = lowerer
            .lower_function(&function)
            .expect_err("AVX-only state bridge must reject EVEX lane shuffles");
        assert!(
            format!("{error:?}").contains("AVX-only vector bridge"),
            "{case:?}: {error:?}"
        );
    }
}
