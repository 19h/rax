//! Exact helper-backed EVEX VPSHUFB/VPMADDUBSW/VPMADDWD memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, VReg, VecElementType,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexBwShuffleMaddKind, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexBwShuffleMaddMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_bw_shuffle_madd_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7F20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    ByteShuffle,
    MultiplyAddUnsignedBytes,
    MultiplyAddWords,
}

impl Kind {
    const ALL: [Self; 3] = [
        Self::ByteShuffle,
        Self::MultiplyAddUnsignedBytes,
        Self::MultiplyAddWords,
    ];

    const fn map_opcode(self) -> (u8, u8) {
        match self {
            Self::ByteShuffle => (2, 0x00),
            Self::MultiplyAddUnsignedBytes => (2, 0x04),
            Self::MultiplyAddWords => (1, 0xF5),
        }
    }

    const fn classified(self) -> X86EvexBwShuffleMaddKind {
        match self {
            Self::ByteShuffle => X86EvexBwShuffleMaddKind::ByteShuffle,
            Self::MultiplyAddUnsignedBytes => X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes,
            Self::MultiplyAddWords => X86EvexBwShuffleMaddKind::MultiplyAddWords,
        }
    }

    const fn result_elem(self) -> VecElementType {
        match self {
            Self::ByteShuffle => VecElementType::I8,
            Self::MultiplyAddUnsignedBytes => VecElementType::I16,
            Self::MultiplyAddWords => VecElementType::I32,
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
            Self::Zero => (5, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BwMemoryCase {
    kind: Kind,
    width: VecWidth,
    destination: u8,
    source1: u8,
    control: MaskControl,
    w: bool,
}

impl BwMemoryCase {
    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!("EVEX AVX-512BW vector width"),
        }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 32 && self.source1 < 32);
        let (map, opcode) = self.kind.map_opcode();
        vec![
            0x62,
            0x60 | map
                | (u8::from(self.destination & 8 == 0) << 7)
                | (u8::from(self.destination & 16 == 0) << 4),
            (u8::from(self.w) << 7) | (((!self.source1) & 0x0F) << 3) | 0x05,
            (u8::from(self.zeroing()) << 7)
                | (self.ll() << 5)
                | (u8::from(self.source1 < 16) << 3)
                | self.mask(),
            opcode,
            ((self.destination & 7) << 3) | 3,
        ]
    }
}

fn x86(register: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(register))
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
        X86InstructionBytes::new(bytes).expect("EVEX AVX-512BW memory provenance"),
    );
    function
}

fn lift_case(case: BwMemoryCase) -> SmirFunction {
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
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .expect("EVEX AVX-512BW Full Mem VLoad")
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
) -> Option<X86JitEvexBwShuffleMaddMemorySequence> {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_bw_shuffle_madd_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: BwMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512vbmi, "{case:?}");
    assert!(!requirements.needs_avx512vbmi2, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX AVX-512BW memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX AVX-512BW memory replay"),
        result.entry_offset,
    )
}

#[test]
fn all_162_scanner_cells_lift_optimize_admit_and_lower_exactly() {
    let mut lowered = 0usize;
    for kind in Kind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for source1 in [0u8, 1, 15] {
                for control in MaskControl::ALL {
                    for w in [false, true] {
                        let case = BwMemoryCase {
                            kind,
                            width,
                            destination: 0,
                            source1,
                            control,
                            w,
                        };
                        let function = optimize(lift_case(case), OptLevel::O2);
                        let matched = sequence(&function, true)
                            .unwrap_or_else(|| panic!("{case:?}: missing exact sequence"));
                        assert_eq!(matched.encoding.kind, kind.classified(), "{case:?}");
                        assert_eq!(matched.encoding.width, width, "{case:?}");
                        assert_eq!(matched.encoding.destination, 0, "{case:?}");
                        assert_eq!(matched.encoding.source1, source1, "{case:?}");
                        assert_eq!(
                            matched.encoding.writemask,
                            (case.mask() != 0).then_some(case.mask()),
                            "{case:?}"
                        );
                        assert_eq!(matched.encoding.zeroing, case.zeroing(), "{case:?}");
                        assert_eq!(matched.encoding.w, w, "{case:?}");
                        assert_eq!(matched.memory_size, width.bytes(), "{case:?}");
                        assert_eq!(matched.address_offset, 0, "{case:?}");
                        assert!(sequence(&function, false).is_none(), "{case:?}");

                        let replay = matched.encoding.register_instruction;
                        let (code, _) = lower(&function, case);
                        assert!(
                            code.windows(replay.as_slice().len())
                                .any(|window| window == replay.as_slice()),
                            "{case:?}: missing replay {replay:?}"
                        );
                        assert!(
                            code.windows(4).any(|window| {
                                window
                                    == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET
                                        .to_le_bytes()
                            }),
                            "{case:?}: missing vector scratch"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 162);
}

fn representative_cases() -> [BwMemoryCase; 9] {
    [
        BwMemoryCase {
            kind: Kind::ByteShuffle,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            control: MaskControl::None,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::ByteShuffle,
            width: VecWidth::V256,
            destination: 9,
            source1: 9,
            control: MaskControl::Merge,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::ByteShuffle,
            width: VecWidth::V512,
            destination: 25,
            source1: 26,
            control: MaskControl::Zero,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddUnsignedBytes,
            width: VecWidth::V128,
            destination: 31,
            source1: 31,
            control: MaskControl::Merge,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddUnsignedBytes,
            width: VecWidth::V256,
            destination: 17,
            source1: 18,
            control: MaskControl::Zero,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddUnsignedBytes,
            width: VecWidth::V512,
            destination: 0,
            source1: 1,
            control: MaskControl::None,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddWords,
            width: VecWidth::V128,
            destination: 9,
            source1: 14,
            control: MaskControl::Zero,
            w: true,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddWords,
            width: VecWidth::V256,
            destination: 25,
            source1: 25,
            control: MaskControl::None,
            w: false,
        },
        BwMemoryCase {
            kind: Kind::MultiplyAddWords,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            control: MaskControl::Merge,
            w: false,
        },
    ]
}

#[test]
fn every_kind_width_alias_mask_wig_and_optimizer_profile_has_one_complete_tuple_load() {
    let mut admitted = 0usize;
    for case in representative_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_eq!(
                function.blocks[0]
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::VLoad { .. }))
                    .count(),
                1,
                "{level:?} {case:?}"
            );
            assert!(
                !function.blocks[0]
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
                "{level:?} {case:?}"
            );
            let matched = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: missing exact sequence"));
            assert_eq!(matched.encoding.kind, case.kind.classified());
            assert_eq!(matched.encoding.width, case.width);
            assert_eq!(matched.encoding.destination, case.destination);
            assert_eq!(matched.encoding.source1, case.source1);
            assert_eq!(matched.encoding.w, case.w);
            let replay = matched.encoding.register_instruction;
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(replay.as_slice().len())
                    .any(|window| window == replay.as_slice())
            );
            admitted += 1;
        }
    }
    assert_eq!(admitted, representative_cases().len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact matcher admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted malformed sequence"
    );
}

#[test]
fn matcher_fails_closed_for_provenance_graph_fault_mask_and_boundary_mutations() {
    let case = BwMemoryCase {
        kind: Kind::MultiplyAddWords,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        control: MaskControl::Merge,
        w: true,
    };
    let base = optimize(lift_case(case), OptLevel::O0);
    let index = sequence_index(&base);
    let loaded = match base.blocks[0].ops[index].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_provenance = base.clone();
    let mut wrong_bytes = case.bytes();
    wrong_bytes[4] = 0x04;
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_bytes).unwrap(),
    );

    let mut wrong_hint = base.clone();
    wrong_hint.blocks[0].ops[index].x86_hint = None;

    let mut virtual_address = base.clone();
    match &mut virtual_address.blocks[0].ops[index].kind {
        OpKind::VLoad { addr, .. } => {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
        _ => unreachable!(),
    }

    let mut wrong_source = base.clone();
    let semantic = wrong_source.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VDotProduct { .. }))
        .unwrap();
    match &mut semantic.kind {
        OpKind::VDotProduct { src1, .. } => {
            *src1 = x86(X86Reg::Zmm(19));
        }
        _ => unreachable!(),
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[index + 1].guest_pc += 1;

    let mut wrong_mask_lane = base.clone();
    let insert = wrong_mask_lane.blocks[0]
        .ops
        .iter_mut()
        .rev()
        .find(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
        .expect("masked result has lane insertions");
    match &mut insert.kind {
        OpKind::VInsertLane { lane, .. } => *lane = 0,
        _ => unreachable!(),
    }

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F00), PC, OpKind::Nop));

    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F01),
        PC + 1,
        OpKind::VMov {
            dst: x86(X86Reg::Zmm(0)),
            src: loaded,
            width: VecWidth::V512,
        },
    ));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("opcode provenance differs", wrong_provenance),
        ("load hint differs", wrong_hint),
        ("address contains virtual register", virtual_address),
        ("semantic source differs", wrong_source),
        ("semantic child PC differs", wrong_pc),
        ("mask lane differs", wrong_mask_lane),
        ("same-PC operation follows sequence", same_pc_tail),
        ("loaded temporary escapes sequence", external_use),
    ] {
        assert_rejected(name, &function);
    }
    assert!(sequence(&base, false).is_none());
}

fn assert_address_controls(kind: Kind) {
    let case = BwMemoryCase {
        kind,
        width: VecWidth::V512,
        destination: 9,
        source1: 14,
        control: MaskControl::Merge,
        w: true,
    };
    let vector = case.bytes();
    let mut rip = vector.clone();
    rip[5] = (rip[5] & 0x38) | 0x05;
    rip.splice(6..6, 0x20i32.to_le_bytes());
    let mut addr32 = vector.clone();
    addr32.insert(0, 0x67);
    let mut fs = vector.clone();
    fs.insert(0, 0x64);
    let mut gs_addr32 = vector.clone();
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32.splice(6..6, [0x8B, 0x02]);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let address_cases = [
        (
            "RIP+disp32",
            rip.clone(),
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + rip.len() as u64),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS Full Mem",
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
            "GS addr32 SIB compressed Full Mem",
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 128,
            })),
        ),
    ];
    for (name, bytes, expected_address) in address_cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| {
                    matches!(&op.kind, OpKind::VLoad { addr, .. } if addr == &expected_address)
                }),
                "{kind:?} {name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true).unwrap_or_else(|| panic!("{kind:?} {name} {level:?}"));
        }
    }

    let mut apx = case.bytes();
    apx[5] = (apx[5] & 0x38) | 0x04;
    apx.push(0x48); // [r16+r17*2] after APX extensions
    apx[1] |= 0x08; // EVEX.B4
    apx[2] &= !0x04; // EVEX.X4 / !U
    let expected = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(
            function.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(&op.kind, OpKind::VLoad { addr, .. } if addr == &expected))
        );
        sequence(&function, true).unwrap_or_else(|| panic!("{kind:?} APX {level:?}"));
    }
}

#[test]
fn segment_addr32_rip_compressed_tuple_and_apx_b4_x4_addresses_remain_exact() {
    for kind in Kind::ALL {
        assert_address_controls(kind);
    }
}

#[test]
fn avx_only_vector_bridge_is_rejected() {
    let case = representative_cases()[2];
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only bridge must reject EVEX AVX-512BW memory replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
