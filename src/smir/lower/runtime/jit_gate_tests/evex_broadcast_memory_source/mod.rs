//! Exact helper-backed EVEX memory-broadcast coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, SourceArch, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexBroadcastMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_broadcast_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0xB108;
const MEMORY_ADDRESS: u64 = 0x4000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    opcode: u8,
    w: bool,
    elem: VecElementType,
    source_lanes: u8,
    width: VecWidth,
    needs_avx512bw: bool,
    needs_avx512dq: bool,
}

impl Shape {
    const fn new(
        opcode: u8,
        w: bool,
        elem: VecElementType,
        source_lanes: u8,
        width: VecWidth,
        needs_avx512bw: bool,
        needs_avx512dq: bool,
    ) -> Self {
        Self {
            opcode,
            w,
            elem,
            source_lanes,
            width,
            needs_avx512bw,
            needs_avx512dq,
        }
    }

    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn memory_size(self) -> u32 {
        self.source_lanes as u32 * self.elem.bytes()
    }

    const fn destination_lanes(self) -> u32 {
        self.width.lanes(self.elem)
    }
}

const SHAPES: [Shape; 34] = [
    Shape::new(
        0x18,
        false,
        VecElementType::F32,
        1,
        VecWidth::V128,
        false,
        false,
    ),
    Shape::new(
        0x18,
        false,
        VecElementType::F32,
        1,
        VecWidth::V256,
        false,
        false,
    ),
    Shape::new(
        0x18,
        false,
        VecElementType::F32,
        1,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x19,
        false,
        VecElementType::F32,
        2,
        VecWidth::V256,
        false,
        true,
    ),
    Shape::new(
        0x19,
        false,
        VecElementType::F32,
        2,
        VecWidth::V512,
        false,
        true,
    ),
    Shape::new(
        0x19,
        true,
        VecElementType::F64,
        1,
        VecWidth::V256,
        false,
        false,
    ),
    Shape::new(
        0x19,
        true,
        VecElementType::F64,
        1,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x1A,
        false,
        VecElementType::F32,
        4,
        VecWidth::V256,
        false,
        false,
    ),
    Shape::new(
        0x1A,
        false,
        VecElementType::F32,
        4,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x1A,
        true,
        VecElementType::F64,
        2,
        VecWidth::V256,
        false,
        true,
    ),
    Shape::new(
        0x1A,
        true,
        VecElementType::F64,
        2,
        VecWidth::V512,
        false,
        true,
    ),
    Shape::new(
        0x1B,
        false,
        VecElementType::F32,
        8,
        VecWidth::V512,
        false,
        true,
    ),
    Shape::new(
        0x1B,
        true,
        VecElementType::F64,
        4,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x58,
        false,
        VecElementType::I32,
        1,
        VecWidth::V128,
        false,
        false,
    ),
    Shape::new(
        0x58,
        false,
        VecElementType::I32,
        1,
        VecWidth::V256,
        false,
        false,
    ),
    Shape::new(
        0x58,
        false,
        VecElementType::I32,
        1,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x59,
        false,
        VecElementType::I32,
        2,
        VecWidth::V128,
        false,
        true,
    ),
    Shape::new(
        0x59,
        false,
        VecElementType::I32,
        2,
        VecWidth::V256,
        false,
        true,
    ),
    Shape::new(
        0x59,
        false,
        VecElementType::I32,
        2,
        VecWidth::V512,
        false,
        true,
    ),
    Shape::new(
        0x59,
        true,
        VecElementType::I64,
        1,
        VecWidth::V128,
        false,
        false,
    ),
    Shape::new(
        0x59,
        true,
        VecElementType::I64,
        1,
        VecWidth::V256,
        false,
        false,
    ),
    Shape::new(
        0x59,
        true,
        VecElementType::I64,
        1,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x5A,
        false,
        VecElementType::I32,
        4,
        VecWidth::V256,
        false,
        false,
    ),
    Shape::new(
        0x5A,
        false,
        VecElementType::I32,
        4,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x5A,
        true,
        VecElementType::I64,
        2,
        VecWidth::V256,
        false,
        true,
    ),
    Shape::new(
        0x5A,
        true,
        VecElementType::I64,
        2,
        VecWidth::V512,
        false,
        true,
    ),
    Shape::new(
        0x5B,
        false,
        VecElementType::I32,
        8,
        VecWidth::V512,
        false,
        true,
    ),
    Shape::new(
        0x5B,
        true,
        VecElementType::I64,
        4,
        VecWidth::V512,
        false,
        false,
    ),
    Shape::new(
        0x78,
        false,
        VecElementType::I8,
        1,
        VecWidth::V128,
        true,
        false,
    ),
    Shape::new(
        0x78,
        false,
        VecElementType::I8,
        1,
        VecWidth::V256,
        true,
        false,
    ),
    Shape::new(
        0x78,
        false,
        VecElementType::I8,
        1,
        VecWidth::V512,
        true,
        false,
    ),
    Shape::new(
        0x79,
        false,
        VecElementType::I16,
        1,
        VecWidth::V128,
        true,
        false,
    ),
    Shape::new(
        0x79,
        false,
        VecElementType::I16,
        1,
        VecWidth::V256,
        true,
        false,
    ),
    Shape::new(
        0x79,
        false,
        VecElementType::I16,
        1,
        VecWidth::V512,
        true,
        false,
    ),
];

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
            Self::Zero => (3, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BroadcastMemoryCase {
    shape: Shape,
    destination: u8,
    base: u8,
    control: MaskControl,
}

impl BroadcastMemoryCase {
    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    fn bytes(self) -> [u8; 6] {
        memory_encoding(self, self.base)
    }

    fn stack_instruction(self) -> [u8; 7] {
        let bytes = self.bytes();
        [
            0x62,
            (bytes[1] & 0x97) | 0x60,
            bytes[2] | 0x04,
            bytes[3],
            bytes[4],
            (bytes[5] & 0x38) | 0x04,
            0x24,
        ]
    }
}

fn memory_encoding(case: BroadcastMemoryCase, base: u8) -> [u8; 6] {
    assert!(case.destination < 32 && base < 16);
    assert!(!matches!(base & 7, 4 | 5));
    let p0 = 2
        | (u8::from(case.destination & 8 == 0) << 7)
        | 0x40
        | (u8::from(base & 8 == 0) << 5)
        | (u8::from(case.destination & 16 == 0) << 4);
    let p1 = (u8::from(case.shape.w) << 7) | 0x7D;
    let p2 = (u8::from(case.zeroing()) << 7) | (case.shape.ll() << 5) | 0x08 | case.mask();
    [
        0x62,
        p0,
        p1,
        p2,
        case.shape.opcode,
        ((case.destination & 7) << 3) | (base & 7),
    ]
}

fn all_cases() -> Vec<BroadcastMemoryCase> {
    let mut cases = Vec::with_capacity(SHAPES.len() * MaskControl::ALL.len());
    let mut ordinal = 0usize;
    for shape in SHAPES {
        for control in MaskControl::ALL {
            cases.push(BroadcastMemoryCase {
                shape,
                destination: [1, 17, 30][ordinal % 3],
                base: 2,
                control,
            });
            ordinal += 1;
        }
    }
    cases
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("EVEX broadcast width"),
    }))
}

fn function_from_bytes(bytes: &[u8]) -> SmirFunction {
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
        X86InstructionBytes::new(bytes).expect("broadcast instruction provenance"),
    );
    function
}

fn lift_case(case: BroadcastMemoryCase) -> SmirFunction {
    function_from_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
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

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexBroadcastMemorySequence> {
    let block = &function.blocks[0];
    let index = usize::from(
        block
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(block);
    x86_jit_evex_broadcast_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn count_bytes(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn lower(function: &SmirFunction, case: BroadcastMemoryCase) -> (Vec<u8>, usize) {
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
        !case.shape.needs_avx512bw && case.shape.destination_lanes() <= 16,
        "{case:?}"
    );

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.shape.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512bw, case.shape.needs_avx512bw,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512dq, case.shape.needs_avx512dq,
        "{case:?}"
    );
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.shape.destination_lanes() <= 16,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512er, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(
        !case.shape.needs_avx512bw && case.shape.destination_lanes() <= 16,
    );
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX broadcast lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    let code = lowerer.finalize().expect("finalize EVEX broadcast replay");
    assert!(
        contains_bytes(&code, &case.stack_instruction()),
        "{case:?}: missing stack replay {:02X?}",
        case.stack_instruction()
    );
    let mut helper_index = vec![0xBA]; // mov edx, reserved vector-scratch index
    helper_index.extend_from_slice(&crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX.to_le_bytes());
    assert!(
        contains_bytes(&code, &helper_index),
        "{case:?}: missing reserved helper index"
    );
    let mut helper_size = vec![0xB9]; // mov ecx, complete source-tuple size
    helper_size.extend_from_slice(&case.shape.memory_size().to_le_bytes());
    assert!(
        contains_bytes(&code, &helper_size),
        "{case:?}: missing {}-byte helper size",
        case.shape.memory_size()
    );
    assert!(
        contains_bytes(&code, &[0x41, 0xB8, 1, 0, 0, 0]),
        "{case:?}: helper load must zero the unused scratch suffix"
    );
    assert!(
        contains_bytes(&code, &[0xC5, 0xFE, 0x7F, 0x44, 0x24, 0x08]),
        "{case:?}: missing VMOVDQU [rsp+8],ymm0 staging store"
    );
    let allocation = [0x48, 0x8D, 0x64, 0x24, 0xE0];
    let release = [0x48, 0x8D, 0x64, 0x24, 0x20];
    assert_eq!(
        count_bytes(&code, &allocation),
        if case.mask() == 0 { 1 } else { 2 },
        "{case:?}: active and suppressed paths must each own one 32-byte slot"
    );
    assert_eq!(
        count_bytes(&code, &release),
        1,
        "{case:?}: both paths must share one 32-byte release"
    );
    (code, result.entry_offset)
}
