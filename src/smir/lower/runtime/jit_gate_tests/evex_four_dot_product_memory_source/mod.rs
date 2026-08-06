//! Exact helper-backed AVX-512 4VNNIW whole-tuple memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, BlockId, FunctionId, SourceArch, VReg, VecWidth, X86Reg};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::x86_host_has_avx5124vnniw;
use crate::smir::lower::runtime::{
    X86JitEvexFourDotProductMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_four_dot_product_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x4D00;
const MEMORY_ADDRESS: u64 = 0x2000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

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
struct FourDotProductMemoryCase {
    saturating: bool,
    destination: u8,
    source_index: u8,
    control: MaskControl,
}

impl FourDotProductMemoryCase {
    const fn opcode(self) -> u8 {
        if self.saturating { 0x53 } else { 0x52 }
    }

    const fn source_base(self) -> u8 {
        self.source_index & !3
    }

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    fn bytes(self) -> [u8; 6] {
        memory_encoding(self, 2)
    }

    fn stack_instruction(self) -> [u8; 7] {
        stack_encoding(self)
    }
}

fn memory_encoding(case: FourDotProductMemoryCase, base: u8) -> [u8; 6] {
    assert!(case.destination < 32 && case.source_index < 32 && base < 16);
    assert!(!case.zeroing() || case.mask() != 0);
    let p0 = 0x02
        | (u8::from(case.destination & 8 == 0) << 7)
        | 0x40
        | (u8::from(base & 8 == 0) << 5)
        | (u8::from(case.destination & 16 == 0) << 4);
    let p1 = (((!case.source_index) & 0x0F) << 3) | 0x07;
    let p2 = (u8::from(case.zeroing()) << 7)
        | 0x40
        | (u8::from(case.source_index & 16 == 0) << 3)
        | case.mask();
    [
        0x62,
        p0,
        p1,
        p2,
        case.opcode(),
        ((case.destination & 7) << 3) | (base & 7),
    ]
}

fn stack_encoding(case: FourDotProductMemoryCase) -> [u8; 7] {
    let bytes = memory_encoding(case, 4);
    [
        bytes[0],
        bytes[1] | 0x20,
        bytes[2] | 0x04,
        bytes[3],
        bytes[4],
        bytes[5],
        0x24,
    ]
}

fn vector(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)))
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
        X86InstructionBytes::new(bytes).expect("4VNNIW provenance"),
    );
    function
}

fn lift_case(case: FourDotProductMemoryCase) -> SmirFunction {
    function_from_bytes(&case.bytes())
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
) -> Option<X86JitEvexFourDotProductMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_four_dot_product_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: FourDotProductMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx5124vnniw, "{case:?}");
    assert!(!requirements.needs_avx5124fmaps, "{case:?}");
    assert!(requirements.has_k16_opmask_span, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512er, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f") && x86_host_has_avx5124vnniw(),
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
        .unwrap_or_else(|error| panic!("{case:?}: 4VNNIW lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize helper-backed 4VNNIW"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<FourDotProductMemoryCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for saturating in [false, true] {
        for source_index in [0, 1, 15, 16, 31] {
            for control in MaskControl::ALL {
                let source_base = source_index & !3;
                let destination = match ordinal % 4 {
                    0 => 1,
                    1 => 17,
                    2 => 30,
                    _ => source_base,
                };
                cases.push(FourDotProductMemoryCase {
                    saturating,
                    destination,
                    source_index,
                    control,
                });
                ordinal += 1;
            }
        }
    }
    cases
}
