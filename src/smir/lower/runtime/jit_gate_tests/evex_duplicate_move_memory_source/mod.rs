//! Exact helper-backed EVEX VMOVSLDUP/VMOVSHDUP/VMOVDDUP memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId, SourceArch, VReg, VecElementType, VecWidth};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexDuplicateMoveMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_duplicate_move_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0xB110;
const MEMORY_ADDRESS: u64 = 0x4000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DuplicateKind {
    LowF32,
    HighF32,
    EvenF64,
}

impl DuplicateKind {
    const ALL: [Self; 3] = [Self::LowF32, Self::HighF32, Self::EvenF64];

    const fn name(self) -> &'static str {
        match self {
            Self::LowF32 => "VMOVSLDUP",
            Self::HighF32 => "VMOVSHDUP",
            Self::EvenF64 => "VMOVDDUP",
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::LowF32 | Self::EvenF64 => 0x12,
            Self::HighF32 => 0x16,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::LowF32 | Self::HighF32 => 2,
            Self::EvenF64 => 3,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::EvenF64)
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::LowF32 | Self::HighF32 => VecElementType::F32,
            Self::EvenF64 => VecElementType::F64,
        }
    }

    const fn high(self) -> bool {
        matches!(self, Self::HighF32)
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
struct DuplicateMemoryCase {
    kind: DuplicateKind,
    width: VecWidth,
    destination: u8,
    base: u8,
    control: MaskControl,
}

impl DuplicateMemoryCase {
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

    const fn lanes(self) -> usize {
        self.width.lanes(self.kind.elem()) as usize
    }

    const fn memory_size(self) -> u32 {
        if matches!(self.kind, DuplicateKind::EvenF64) && matches!(self.width, VecWidth::V128) {
            8
        } else {
            self.width.bytes()
        }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination)
            .expect("one destination leaves a low vector scratch")
    }

    fn bytes(self) -> [u8; 6] {
        memory_encoding(self, self.base)
    }

    fn register_instruction(self) -> [u8; 6] {
        let bytes = self.bytes();
        let scratch = self.scratch();
        [
            0x62,
            (bytes[1] & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            bytes[2] | 0x04,
            bytes[3] & !0x10,
            bytes[4],
            0xC0 | (bytes[5] & 0x38) | (scratch & 7),
        ]
    }
}

fn memory_encoding(case: DuplicateMemoryCase, base: u8) -> [u8; 6] {
    assert!(case.destination < 32 && base < 16 && case.mask() < 8);
    assert!(!matches!(base & 7, 4 | 5));
    [
        0x62,
        1 | (u8::from(case.destination & 8 == 0) << 7)
            | 0x40
            | (u8::from(base & 8 == 0) << 5)
            | (u8::from(case.destination & 16 == 0) << 4),
        (u8::from(case.kind.w()) << 7) | 0x7C | case.kind.pp(),
        (u8::from(case.zeroing()) << 7) | (case.ll() << 5) | 0x08 | case.mask(),
        case.kind.opcode(),
        ((case.destination & 7) << 3) | (base & 7),
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
        X86InstructionBytes::new(bytes).expect("EVEX duplicate-move provenance"),
    );
    function
}

fn lift_case(case: DuplicateMemoryCase) -> SmirFunction {
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
) -> Option<X86JitEvexDuplicateMoveMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_duplicate_move_memory_sequence(
        &function.blocks[0],
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn all_cases() -> Vec<DuplicateMemoryCase> {
    let mut cases = Vec::with_capacity(27);
    let mut ordinal = 0usize;
    for kind in DuplicateKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in MaskControl::ALL {
                cases.push(DuplicateMemoryCase {
                    kind,
                    width,
                    destination: [1, 9, 17, 25][ordinal & 3],
                    base: 2,
                    control,
                });
                ordinal += 1;
            }
        }
    }
    assert_eq!(cases.len(), 27);
    cases
}

fn lower(function: &SmirFunction, case: DuplicateMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(requirements.has_k16_opmask_span, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx512f")
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
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX duplicate move memory: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize EVEX duplicate-move memory"),
        result.entry_offset,
    )
}
