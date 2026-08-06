//! Exact helper-backed EVEX scalar and vector-chunk extraction to memory.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SourceArch, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexChunkExtractMemoryEncoding,
    X86EvexScalarExtractMemoryEncoding, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitEvexExtractMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_extract_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0xE6E9_0019;
pub(super) const MEMORY_ADDRESS: u64 = 0x3000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScalarShape {
    pub(super) opcode: u8,
    pub(super) w: bool,
    pub(super) elem: VecElementType,
    pub(super) memory_width: MemWidth,
    pub(super) lane_mask: u8,
    pub(super) needs_avx512bw: bool,
    pub(super) needs_avx512dq: bool,
}

pub(super) const SCALAR_SHAPES: [ScalarShape; 8] = [
    ScalarShape {
        opcode: 0x14,
        w: false,
        elem: VecElementType::I8,
        memory_width: MemWidth::B1,
        lane_mask: 0x0F,
        needs_avx512bw: true,
        needs_avx512dq: false,
    },
    ScalarShape {
        opcode: 0x14,
        w: true,
        elem: VecElementType::I8,
        memory_width: MemWidth::B1,
        lane_mask: 0x0F,
        needs_avx512bw: true,
        needs_avx512dq: false,
    },
    ScalarShape {
        opcode: 0x15,
        w: false,
        elem: VecElementType::I16,
        memory_width: MemWidth::B2,
        lane_mask: 0x07,
        needs_avx512bw: true,
        needs_avx512dq: false,
    },
    ScalarShape {
        opcode: 0x15,
        w: true,
        elem: VecElementType::I16,
        memory_width: MemWidth::B2,
        lane_mask: 0x07,
        needs_avx512bw: true,
        needs_avx512dq: false,
    },
    ScalarShape {
        opcode: 0x16,
        w: false,
        elem: VecElementType::I32,
        memory_width: MemWidth::B4,
        lane_mask: 0x03,
        needs_avx512bw: false,
        needs_avx512dq: true,
    },
    ScalarShape {
        opcode: 0x16,
        w: true,
        elem: VecElementType::I64,
        memory_width: MemWidth::B8,
        lane_mask: 0x01,
        needs_avx512bw: false,
        needs_avx512dq: true,
    },
    ScalarShape {
        opcode: 0x17,
        w: false,
        elem: VecElementType::I32,
        memory_width: MemWidth::B4,
        lane_mask: 0x03,
        needs_avx512bw: false,
        needs_avx512dq: false,
    },
    ScalarShape {
        opcode: 0x17,
        w: true,
        elem: VecElementType::I32,
        memory_width: MemWidth::B4,
        lane_mask: 0x03,
        needs_avx512bw: false,
        needs_avx512dq: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChunkShape {
    pub(super) opcode: u8,
    pub(super) w: bool,
    pub(super) source_width: VecWidth,
}

impl ChunkShape {
    pub(super) const fn chunk_width(self) -> VecWidth {
        if matches!(self.opcode, 0x1B | 0x3B) {
            VecWidth::V256
        } else {
            VecWidth::V128
        }
    }

    pub(super) const fn elem(self) -> VecElementType {
        match (self.opcode < 0x30, self.w) {
            (true, false) => VecElementType::F32,
            (true, true) => VecElementType::F64,
            (false, false) => VecElementType::I32,
            (false, true) => VecElementType::I64,
        }
    }

    pub(super) const fn ll(self) -> u8 {
        match self.source_width {
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    pub(super) const fn needs_avx512dq(self) -> bool {
        self.w != matches!(self.opcode, 0x1B | 0x3B)
    }
}

pub(super) const CHUNK_SHAPES: [ChunkShape; 12] = [
    ChunkShape {
        opcode: 0x19,
        w: false,
        source_width: VecWidth::V256,
    },
    ChunkShape {
        opcode: 0x19,
        w: false,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x19,
        w: true,
        source_width: VecWidth::V256,
    },
    ChunkShape {
        opcode: 0x19,
        w: true,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x39,
        w: false,
        source_width: VecWidth::V256,
    },
    ChunkShape {
        opcode: 0x39,
        w: false,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x39,
        w: true,
        source_width: VecWidth::V256,
    },
    ChunkShape {
        opcode: 0x39,
        w: true,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x1B,
        w: false,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x1B,
        w: true,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x3B,
        w: false,
        source_width: VecWidth::V512,
    },
    ChunkShape {
        opcode: 0x3B,
        w: true,
        source_width: VecWidth::V512,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExtractMemoryCase {
    Scalar {
        shape: ScalarShape,
        source: u8,
        immediate: u8,
    },
    Chunk {
        shape: ChunkShape,
        source: u8,
        writemask: Option<u8>,
        immediate: u8,
    },
}

impl ExtractMemoryCase {
    pub(super) const fn source(self) -> u8 {
        match self {
            Self::Scalar { source, .. } | Self::Chunk { source, .. } => source,
        }
    }

    pub(super) const fn writemask(self) -> Option<u8> {
        match self {
            Self::Scalar { .. } => None,
            Self::Chunk { writemask, .. } => writemask,
        }
    }

    pub(super) const fn immediate(self) -> u8 {
        match self {
            Self::Scalar { immediate, .. } | Self::Chunk { immediate, .. } => immediate,
        }
    }

    pub(super) const fn elem(self) -> VecElementType {
        match self {
            Self::Scalar { shape, .. } => shape.elem,
            Self::Chunk { shape, .. } => shape.elem(),
        }
    }

    pub(super) const fn memory_size(self) -> u32 {
        match self {
            Self::Scalar { shape, .. } => shape.memory_width.bytes(),
            Self::Chunk { shape, .. } => shape.chunk_width().bytes(),
        }
    }

    pub(super) const fn needs_avx512vl(self) -> bool {
        matches!(
            self,
            Self::Chunk {
                shape: ChunkShape {
                    source_width: VecWidth::V256,
                    ..
                },
                ..
            }
        )
    }

    pub(super) const fn needs_avx512dq(self) -> bool {
        match self {
            Self::Scalar { shape, .. } => shape.needs_avx512dq,
            Self::Chunk { shape, .. } => shape.needs_avx512dq(),
        }
    }

    pub(super) fn lanes(self) -> usize {
        match self {
            Self::Scalar { .. } => 1,
            Self::Chunk { shape, .. } => shape.chunk_width().lanes(shape.elem()) as usize,
        }
    }

    pub(super) fn selected_first_lane(self) -> usize {
        match self {
            Self::Scalar {
                shape, immediate, ..
            } => usize::from(immediate & shape.lane_mask),
            Self::Chunk {
                shape, immediate, ..
            } => {
                let chunks = shape.source_width.bytes() / shape.chunk_width().bytes();
                usize::from(immediate & (chunks as u8 - 1)) * self.lanes()
            }
        }
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        memory_encoding(self, false)
    }

    pub(super) fn expected_replay(self) -> X86InstructionBytes {
        let bytes = self.bytes();
        let p0 = (bytes[1] & 0x97) | 0x60;
        let p1 = bytes[2] | 0x04;
        match self {
            Self::Scalar { .. } => X86InstructionBytes::new(&[
                0x62,
                p0,
                p1,
                bytes[3],
                bytes[4],
                0xC0 | (bytes[5] & 0x38),
                self.immediate(),
            ])
            .unwrap(),
            Self::Chunk { .. } => X86InstructionBytes::new(&[
                0x62,
                p0,
                p1,
                bytes[3],
                bytes[4],
                (bytes[5] & 0x38) | 4,
                0x24,
                self.immediate(),
            ])
            .unwrap(),
        }
    }
}

pub(super) fn memory_encoding(case: ExtractMemoryCase, sib: bool) -> Vec<u8> {
    assert!(case.source() < 32);
    let (opcode, w, ll, mask) = match case {
        ExtractMemoryCase::Scalar { shape, .. } => (shape.opcode, shape.w, 0, 0),
        ExtractMemoryCase::Chunk {
            shape, writemask, ..
        } => (shape.opcode, shape.w, shape.ll(), writemask.unwrap_or(0)),
    };
    let mut p0 = 0xF3;
    if case.source() & 8 != 0 {
        p0 &= !0x80;
    }
    if case.source() & 16 != 0 {
        p0 &= !0x10;
    }
    let mut bytes = vec![
        0x62,
        p0,
        0x7D | (u8::from(w) << 7),
        (ll << 5) | 0x08 | mask,
        opcode,
        ((case.source() & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are varied independently in tests.
        bytes.push(0x48);
    }
    bytes.push(case.immediate());
    bytes
}

pub(super) fn lift_bytes(bytes: &[u8]) -> SmirFunction {
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
        X86InstructionBytes::new(bytes).expect("EVEX extraction instruction provenance"),
    );
    function
}

pub(super) fn lift_case(case: ExtractMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

pub(super) fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
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

pub(super) fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexExtractMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_extract_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn configured_lowerer(avx_only: bool) -> X86_64Lowerer {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(avx_only);
    lowerer.set_narrow_vector_opmask_helpers(false);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
}

pub(super) fn lower(function: &SmirFunction, case: ExtractMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(!x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any && requirements.needs_avx);
    assert!(requirements.needs_avx512bw);
    assert_eq!(requirements.needs_avx512vl, case.needs_avx512vl());
    assert_eq!(requirements.needs_avx512dq, case.needs_avx512dq());
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.writemask().is_some() && case.lanes() <= 16
    );
    assert!(!requirements.all_spans_support_avx_ymm16);

    let mut lowerer = configured_lowerer(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX extraction lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize EVEX extraction replay"),
        result.entry_offset,
    )
}

pub(super) fn all_cases() -> Vec<ExtractMemoryCase> {
    let mut cases = Vec::with_capacity(32);
    for (index, shape) in SCALAR_SHAPES.into_iter().enumerate() {
        cases.push(ExtractMemoryCase::Scalar {
            shape,
            source: [0, 9, 17, 31][index & 3],
            immediate: (index as u8).wrapping_mul(0x25) ^ 0xA5,
        });
    }
    for (index, shape) in CHUNK_SHAPES.into_iter().enumerate() {
        for writemask in [None, Some((index as u8 % 7) + 1)] {
            cases.push(ExtractMemoryCase::Chunk {
                shape,
                source: [0, 9, 17, 31][index & 3],
                writemask,
                immediate: (index as u8).wrapping_mul(0x35) ^ 0xD3,
            });
        }
    }
    cases
}

#[test]
fn all_32_evex_extract_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 32);
    let mut lowerings = 0usize;
    for case in cases {
        let bytes = case.bytes();
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let expected_replay = case.expected_replay();
        match case {
            ExtractMemoryCase::Scalar {
                shape,
                source,
                immediate,
            } => {
                let encoding = instruction
                    .evex_scalar_extract_memory_encoding()
                    .unwrap_or_else(|| panic!("{case:?}"));
                assert_eq!(encoding.source, source);
                assert_eq!(encoding.lane, immediate & shape.lane_mask);
                assert_eq!(encoding.elem, shape.elem);
                assert_eq!(encoding.memory_width, shape.memory_width);
                assert_eq!(encoding.register_instruction, expected_replay);
                assert_eq!(encoding.needs_avx512bw, shape.needs_avx512bw);
                assert_eq!(encoding.needs_avx512dq, shape.needs_avx512dq);
            }
            ExtractMemoryCase::Chunk {
                shape,
                source,
                writemask,
                ..
            } => {
                let encoding = instruction
                    .evex_chunk_extract_memory_encoding()
                    .unwrap_or_else(|| panic!("{case:?}"));
                assert_eq!(encoding.source, source);
                assert_eq!(encoding.source_width, shape.source_width);
                assert_eq!(encoding.chunk_width, shape.chunk_width());
                assert_eq!(encoding.elem, shape.elem());
                assert_eq!(encoding.writemask, writemask);
                assert_eq!(encoding.stack_instruction, expected_replay);
            }
        }

        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.consumed(), function.blocks[0].ops.len());
            match (case, exact) {
                (ExtractMemoryCase::Scalar { .. }, X86JitEvexExtractMemorySequence::Scalar(s)) => {
                    assert_eq!(s.address_offset, 1);
                }
                (ExtractMemoryCase::Chunk { .. }, X86JitEvexExtractMemorySequence::Chunk(s)) => {
                    assert_eq!(s.address_offset, 2 + 2 * case.lanes());
                    let expected_ops = 3
                        + 2 * case.lanes()
                        + usize::from(case.writemask().is_some())
                            * (4 + 6 * case.lanes() - usize::from(level == OptLevel::O2));
                    assert_eq!(s.consumed, expected_ops, "{level:?} {case:?}");
                }
                _ => panic!("sequence kind mismatch: {level:?} {case:?}"),
            }

            let (code, _) = lower(&function, case);
            assert!(
                code.windows(expected_replay.as_slice().len())
                    .any(|window| window == expected_replay.as_slice()),
                "{level:?} {case:?}: missing {:02X?} in {} bytes",
                expected_replay.as_slice(),
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 32 * LEVELS.len());
}

#[test]
fn evex_extract_matcher_rejects_graph_provenance_address_and_escape_mutations() {
    let case = ExtractMemoryCase::Chunk {
        shape: CHUNK_SHAPES[11],
        source: 17,
        writemask: Some(3),
        immediate: 0xA5,
    };
    let valid = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&valid, true).is_some());

    let mut wrong_store = valid.clone();
    let store = wrong_store.blocks[0]
        .ops
        .iter_mut()
        .rfind(|op| matches!(op.kind, OpKind::VStore { .. }))
        .unwrap();
    let OpKind::VStore { width, .. } = &mut store.kind else {
        unreachable!()
    };
    *width = VecWidth::V128;
    assert!(sequence(&wrong_store, true).is_none());

    let mut wrong_load_address = valid.clone();
    let load = wrong_load_address.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .unwrap();
    let OpKind::VLoad { addr, .. } = &mut load.kind else {
        unreachable!()
    };
    *addr = Address::Absolute(MEMORY_ADDRESS + 1);
    assert!(sequence(&wrong_load_address, true).is_none());

    let mut wrong_provenance = valid.clone();
    let scalar = ExtractMemoryCase::Scalar {
        shape: SCALAR_SHAPES[0],
        source: 17,
        immediate: 0xA5,
    };
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&scalar.bytes()).unwrap(),
    );
    assert!(sequence(&wrong_provenance, true).is_none());

    let mut wrong_source = valid.clone();
    let extract = wrong_source.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .unwrap();
    let OpKind::VExtractLane { vec, .. } = &mut extract.kind else {
        unreachable!()
    };
    *vec = VReg::Arch(ArchReg::X86(X86Reg::Zmm(18)));
    assert!(sequence(&wrong_source, true).is_none());

    let mut escaped = valid.clone();
    let leaked = escaped.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .find(|register| matches!(register, VReg::Virtual(_)))
        .unwrap();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(u16::MAX),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(u32::MAX)),
            src: SrcOperand::Reg(leaked),
            width: OpWidth::W64,
        },
    ));
    assert!(sequence(&escaped, true).is_none());
}

#[test]
fn evex_extract_segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let case = ExtractMemoryCase::Chunk {
        shape: CHUNK_SHAPES[11],
        source: 17,
        writemask: Some(3),
        immediate: 0xA5,
    };
    let mut rip = case.bytes();
    let immediate = rip.pop().unwrap();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    rip.push(immediate);
    let mut addr32 = case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes();
    fs.insert(0, 0x64);
    for (name, bytes) in [("RIP", rip), ("addr32", addr32), ("FS", fs)] {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let mut apx = memory_encoding(case, true);
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    let base = lift_bytes(&apx);
    assert!(matches!(base.blocks[0].ops[0].kind, OpKind::X86RequireApx));
    let mut missing_guard = base.clone();
    missing_guard.blocks[0].ops.remove(0);
    assert!(sequence(&missing_guard, true).is_none());
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        sequence(&function, true)
            .unwrap_or_else(|| panic!("APX {level:?}: {:#?}", function.blocks[0].ops));
        lower(&function, case);
    }
}

#[test]
fn evex_extract_full_vector_bridge_rejects_avx_only_lowering() {
    let case = all_cases()
        .into_iter()
        .find(|case| case.writemask().is_some())
        .unwrap();
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());
    let mut lowerer = configured_lowerer(true);
    assert!(matches!(
        lowerer.lower_function(&function),
        Err(LowerError::InvalidOperand { .. })
    ));
}
