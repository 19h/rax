//! Exact helper-backed writemasked EVEX packed-move memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId, SourceArch, VReg, VecElementType, VecWidth};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedMoveMemoryKind, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedMoveMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_move_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0xB109;
const MEMORY_ADDRESS: u64 = 0x4000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MoveSpec {
    name: &'static str,
    pp: u8,
    w: bool,
    load_opcode: u8,
    store_opcode: u8,
    elem: VecElementType,
    aligned: bool,
    needs_avx512bw: bool,
}

impl MoveSpec {
    const fn opcode(self, direction: Direction) -> u8 {
        match direction {
            Direction::Load => self.load_opcode,
            Direction::Store => self.store_opcode,
        }
    }

    const fn stack_opcode(self, direction: Direction) -> u8 {
        match (self.aligned, direction) {
            (true, Direction::Load)
                if matches!(self.elem, VecElementType::F32 | VecElementType::F64) =>
            {
                0x10
            }
            (true, Direction::Store)
                if matches!(self.elem, VecElementType::F32 | VecElementType::F64) =>
            {
                0x11
            }
            _ => self.opcode(direction),
        }
    }

    const fn stack_pp(self) -> u8 {
        if self.aligned && matches!(self.elem, VecElementType::I32 | VecElementType::I64) {
            2
        } else {
            self.pp
        }
    }
}

const SPECS: [MoveSpec; 10] = [
    MoveSpec {
        name: "VMOVUPS",
        pp: 0,
        w: false,
        load_opcode: 0x10,
        store_opcode: 0x11,
        elem: VecElementType::F32,
        aligned: false,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVUPD",
        pp: 1,
        w: true,
        load_opcode: 0x10,
        store_opcode: 0x11,
        elem: VecElementType::F64,
        aligned: false,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVAPS",
        pp: 0,
        w: false,
        load_opcode: 0x28,
        store_opcode: 0x29,
        elem: VecElementType::F32,
        aligned: true,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVAPD",
        pp: 1,
        w: true,
        load_opcode: 0x28,
        store_opcode: 0x29,
        elem: VecElementType::F64,
        aligned: true,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVDQA32",
        pp: 1,
        w: false,
        load_opcode: 0x6F,
        store_opcode: 0x7F,
        elem: VecElementType::I32,
        aligned: true,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVDQA64",
        pp: 1,
        w: true,
        load_opcode: 0x6F,
        store_opcode: 0x7F,
        elem: VecElementType::I64,
        aligned: true,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVDQU8",
        pp: 3,
        w: false,
        load_opcode: 0x6F,
        store_opcode: 0x7F,
        elem: VecElementType::I8,
        aligned: false,
        needs_avx512bw: true,
    },
    MoveSpec {
        name: "VMOVDQU16",
        pp: 3,
        w: true,
        load_opcode: 0x6F,
        store_opcode: 0x7F,
        elem: VecElementType::I16,
        aligned: false,
        needs_avx512bw: true,
    },
    MoveSpec {
        name: "VMOVDQU32",
        pp: 2,
        w: false,
        load_opcode: 0x6F,
        store_opcode: 0x7F,
        elem: VecElementType::I32,
        aligned: false,
        needs_avx512bw: false,
    },
    MoveSpec {
        name: "VMOVDQU64",
        pp: 2,
        w: true,
        load_opcode: 0x6F,
        store_opcode: 0x7F,
        elem: VecElementType::I64,
        aligned: false,
        needs_avx512bw: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Load,
    Store,
}

impl Direction {
    const ALL: [Self; 2] = [Self::Load, Self::Store];

    const fn kind(self) -> X86EvexPackedMoveMemoryKind {
        match self {
            Self::Load => X86EvexPackedMoveMemoryKind::Load,
            Self::Store => X86EvexPackedMoveMemoryKind::Store,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    Merge,
    Zero,
}

impl MaskControl {
    const fn zeroing(self) -> bool {
        matches!(self, Self::Zero)
    }

    const fn valid_for(self, direction: Direction) -> bool {
        !(matches!(direction, Direction::Store) && self.zeroing())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedMoveMemoryCase {
    spec: MoveSpec,
    direction: Direction,
    width: VecWidth,
    vector: u8,
    base: u8,
    mask: u8,
    control: MaskControl,
}

impl PackedMoveMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn zeroing(self) -> bool {
        self.control.zeroing()
    }

    const fn lanes(self) -> usize {
        self.width.lanes(self.spec.elem) as usize
    }

    const fn lane_bytes(self) -> usize {
        self.spec.elem.bytes() as usize
    }

    fn bytes(self) -> [u8; 6] {
        memory_encoding(self, self.base)
    }

    fn stack_instruction(self) -> [u8; 7] {
        let bytes = self.bytes();
        let p2 = if self.direction == Direction::Store {
            bytes[3] & !0x87
        } else {
            bytes[3]
        };
        [
            0x62,
            (bytes[1] & 0x97) | 0x60,
            (bytes[2] & !3) | self.spec.stack_pp() | 0x04,
            p2,
            self.spec.stack_opcode(self.direction),
            (bytes[5] & 0x38) | 0x04,
            0x24,
        ]
    }
}

fn memory_encoding(case: PackedMoveMemoryCase, base: u8) -> [u8; 6] {
    assert!(case.vector < 32 && base < 16 && case.mask != 0 && case.mask < 8);
    assert!(!matches!(base & 7, 4 | 5));
    assert!(case.control.valid_for(case.direction));
    [
        0x62,
        1 | (u8::from(case.vector & 8 == 0) << 7)
            | 0x40
            | (u8::from(base & 8 == 0) << 5)
            | (u8::from(case.vector & 16 == 0) << 4),
        (u8::from(case.spec.w) << 7) | 0x7C | case.spec.pp,
        (u8::from(case.zeroing()) << 7) | (case.ll() << 5) | 0x08 | case.mask,
        case.spec.opcode(case.direction),
        ((case.vector & 7) << 3) | (base & 7),
    ]
}

fn function_from_bytes(bytes: &[u8], label: impl std::fmt::Debug) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{label:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{label:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("EVEX packed-move memory provenance"),
    );
    function
}

fn lift_case(case: PackedMoveMemoryCase) -> SmirFunction {
    function_from_bytes(&case.bytes(), case)
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

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexPackedMoveMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_move_memory_sequence(
        &function.blocks[0],
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn all_cases() -> Vec<PackedMoveMemoryCase> {
    let mut cases = Vec::with_capacity(90);
    let mut ordinal = 0usize;
    for spec in SPECS {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for direction in Direction::ALL {
                for control in [MaskControl::Merge, MaskControl::Zero] {
                    if !control.valid_for(direction) {
                        continue;
                    }
                    cases.push(PackedMoveMemoryCase {
                        spec,
                        direction,
                        width,
                        vector: [1, 17, 30][ordinal % 3],
                        base: 2,
                        mask: [1, 3, 7][ordinal % 3],
                        control,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    assert_eq!(cases.len(), 90);
    cases
}

fn lower(function: &SmirFunction, case: PackedMoveMemoryCase) -> (Vec<u8>, usize) {
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

    let uses_k16 = !case.spec.needs_avx512bw && case.lanes() <= 16;
    assert_eq!(
        x86_native_vector_uses_k16_opmasks_excluding(function, &excluded),
        uses_k16,
        "{case:?}"
    );
    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512bw, case.spec.needs_avx512bw,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert_eq!(requirements.has_k16_opmask_span, case.lanes() <= 16);
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
            && (!case.spec.needs_avx512bw || std::is_x86_feature_detected!("avx512bw"))
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
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(uses_k16);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX packed-move memory: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize EVEX packed-move memory"),
        result.entry_offset,
    )
}
