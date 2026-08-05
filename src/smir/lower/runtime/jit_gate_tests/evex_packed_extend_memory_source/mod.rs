//! Exact helper-backed EVEX packed widening-move memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId, SourceArch, VReg, VecElementType, VecWidth};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedExtendMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedExtendMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_extend_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0x2030_2535;
pub(super) const MEMORY_ADDRESS: u64 = 0x2000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExtendSpec {
    pub(super) name: &'static str,
    pub(super) opcode: u8,
    pub(super) source_elem: VecElementType,
    pub(super) destination_elem: VecElementType,
    pub(super) signed: bool,
}

impl ExtendSpec {
    const fn new(
        name: &'static str,
        opcode: u8,
        source_elem: VecElementType,
        destination_elem: VecElementType,
        signed: bool,
    ) -> Self {
        Self {
            name,
            opcode,
            source_elem,
            destination_elem,
            signed,
        }
    }

    pub(super) const fn w_values(self) -> &'static [bool] {
        if matches!(self.opcode, 0x25 | 0x35) {
            &[false]
        } else {
            &[false, true]
        }
    }

    pub(super) const fn instruction_needs_avx512bw(self) -> bool {
        matches!(self.opcode, 0x20 | 0x30)
    }
}

use VecElementType::{I8, I16, I32, I64};

pub(super) const SPECS: [ExtendSpec; 12] = [
    ExtendSpec::new("VPMOVSXBW", 0x20, I8, I16, true),
    ExtendSpec::new("VPMOVSXBD", 0x21, I8, I32, true),
    ExtendSpec::new("VPMOVSXBQ", 0x22, I8, I64, true),
    ExtendSpec::new("VPMOVSXWD", 0x23, I16, I32, true),
    ExtendSpec::new("VPMOVSXWQ", 0x24, I16, I64, true),
    ExtendSpec::new("VPMOVSXDQ", 0x25, I32, I64, true),
    ExtendSpec::new("VPMOVZXBW", 0x30, I8, I16, false),
    ExtendSpec::new("VPMOVZXBD", 0x31, I8, I32, false),
    ExtendSpec::new("VPMOVZXBQ", 0x32, I8, I64, false),
    ExtendSpec::new("VPMOVZXWD", 0x33, I16, I32, false),
    ExtendSpec::new("VPMOVZXWQ", 0x34, I16, I64, false),
    ExtendSpec::new("VPMOVZXDQ", 0x35, I32, I64, false),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    pub(super) const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    pub(super) const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (1, false),
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExtendCase {
    pub(super) spec: ExtendSpec,
    pub(super) w: bool,
    pub(super) ll: u8,
    pub(super) destination: u8,
    pub(super) control: MaskControl,
}

impl ExtendCase {
    pub(super) const fn width(self) -> VecWidth {
        match self.ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!(),
        }
    }

    pub(super) fn lanes(self) -> u8 {
        self.width().lanes(self.spec.destination_elem) as u8
    }

    pub(super) fn memory_size(self) -> u32 {
        u32::from(self.lanes()) * self.spec.source_elem.bytes()
    }

    pub(super) fn source_width(self) -> VecWidth {
        exact_width(self.memory_size())
    }

    pub(super) const fn mask(self) -> u8 {
        self.control.fields().0
    }

    pub(super) const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        assert!(self.ll < 3 && self.destination < 32);
        assert!(self.spec.w_values().contains(&self.w));
        let p0 = 2
            | if self.destination & 8 == 0 { 0x80 } else { 0 }
            | if self.destination & 16 == 0 { 0x10 } else { 0 }
            | 0x60;
        vec![
            0x62,
            p0,
            (u8::from(self.w) << 7) | 0x7D,
            (u8::from(self.zeroing()) << 7) | (self.ll << 5) | 0x08 | self.mask(),
            self.spec.opcode,
            ((self.destination & 7) << 3) | 2,
        ]
    }

    pub(super) fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination)
            .expect("one destination leaves a low vector scratch")
    }

    pub(super) fn expected_replay(self) -> Vec<u8> {
        let memory = self.bytes();
        if self.mask() != 0 {
            vec![
                0x62,
                (memory[1] & 0x97) | 0x60,
                memory[2] | 0x04,
                memory[3],
                memory[4],
                (memory[5] & 0x38) | 0x04,
                0x24,
            ]
        } else {
            let scratch = self.scratch();
            vec![
                0x62,
                (memory[1] & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                memory[2] | 0x04,
                memory[3],
                memory[4],
                0xC0 | (memory[5] & 0x38) | (scratch & 7),
            ]
        }
    }
}

fn exact_width(bytes: u32) -> VecWidth {
    match bytes {
        0..=8 => VecWidth::V64,
        9..=16 => VecWidth::V128,
        17..=32 => VecWidth::V256,
        _ => VecWidth::V512,
    }
}

pub(super) fn all_cases() -> Vec<ExtendCase> {
    let mut cases = Vec::with_capacity(198);
    for spec in SPECS {
        for &w in spec.w_values() {
            for ll in 0..=2 {
                for control in MaskControl::ALL {
                    cases.push(ExtendCase {
                        spec,
                        w,
                        ll,
                        destination: 0,
                        control,
                    });
                }
            }
        }
    }
    assert_eq!(cases.len(), 198);
    cases
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
        X86InstructionBytes::new(bytes).expect("EVEX packed-extension provenance"),
    );
    function
}

pub(super) fn lift_case(case: ExtendCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

pub(super) fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

pub(super) fn virtual_counts(
    function: &SmirFunction,
) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
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
) -> Option<X86JitEvexPackedExtendMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_extend_memory_sequence(
        &function.blocks[0],
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

pub(super) fn lower(function: &SmirFunction, case: ExtendCase) -> (Vec<u8>, usize) {
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
    assert_eq!(requirements.needs_avx512vl, case.ll != 2, "{case:?}");
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.mask() != 0,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed packed extension: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    let code = lowerer.finalize().expect("finalize EVEX packed extension");
    assert!(
        contains_bytes(&code, &case.expected_replay()),
        "{case:?}: missing replay {:02X?}",
        case.expected_replay()
    );
    (code, result.entry_offset)
}

pub(super) fn replay_kind(sequence: X86JitEvexPackedExtendMemorySequence) -> &'static str {
    match sequence.encoding.replay {
        X86EvexPackedExtendMemoryReplay::Vector { .. } => "vector",
        X86EvexPackedExtendMemoryReplay::MaskedVector { .. } => "masked-vector",
    }
}
