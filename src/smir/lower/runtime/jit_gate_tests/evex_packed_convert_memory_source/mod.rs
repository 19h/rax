//! Exact helper-backed EVEX packed F16/F32/F64/I32/I64 conversion memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId, SourceArch, VReg, VecElementType, VecWidth};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexPackedConvertMemoryKind,
    X86EvexPackedConvertMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexPackedConvertMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_packed_convert_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

pub(super) const PC: u64 = 0x5A5B_7AE6;
pub(super) const MEMORY_ADDRESS: u64 = 0x2000;
pub(super) const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConvertSpec {
    pub(super) name: &'static str,
    pub(super) map: u8,
    pub(super) pp: u8,
    pub(super) w: bool,
    pub(super) opcode: u8,
    pub(super) kind: X86EvexPackedConvertMemoryKind,
}

impl ConvertSpec {
    const fn new(
        name: &'static str,
        pp: u8,
        w: bool,
        opcode: u8,
        kind: X86EvexPackedConvertMemoryKind,
    ) -> Self {
        Self {
            name,
            map: 1,
            pp,
            w,
            opcode,
            kind,
        }
    }

    const fn with_map(mut self, map: u8) -> Self {
        self.map = map;
        self
    }

    pub(super) const fn source_elem(self) -> VecElementType {
        match self.kind {
            X86EvexPackedConvertMemoryKind::FpPrecision { from, .. } => from,
            X86EvexPackedConvertMemoryKind::IntToFp { int_elem, .. } => int_elem,
            X86EvexPackedConvertMemoryKind::FpToInt { fp_elem, .. } => fp_elem,
        }
    }

    pub(super) const fn destination_elem(self) -> VecElementType {
        match self.kind {
            X86EvexPackedConvertMemoryKind::FpPrecision { to, .. } => to,
            X86EvexPackedConvertMemoryKind::IntToFp { fp_elem, .. } => fp_elem,
            X86EvexPackedConvertMemoryKind::FpToInt { int_elem, .. } => int_elem,
        }
    }

    pub(super) const fn needs_avx512dq(self) -> bool {
        match self.kind {
            X86EvexPackedConvertMemoryKind::FpPrecision { .. } => false,
            X86EvexPackedConvertMemoryKind::IntToFp { int_elem, .. }
            | X86EvexPackedConvertMemoryKind::FpToInt { int_elem, .. } => {
                matches!(int_elem, VecElementType::I64)
            }
        }
    }

    pub(super) const fn truncates(self) -> bool {
        matches!(
            self.kind,
            X86EvexPackedConvertMemoryKind::FpToInt { truncate: true, .. }
        )
    }
}

use VecElementType::{F16, F32, F64, I32, I64};
use X86EvexPackedConvertMemoryKind::{FpPrecision, FpToInt, IntToFp};

pub(super) const FP16_WIDEN_SPEC: ConvertSpec = ConvertSpec::new(
    "VCVTPH2PS",
    1,
    false,
    0x13,
    FpPrecision { from: F16, to: F32 },
)
.with_map(2);

pub(super) const SPECS: [ConvertSpec; 26] = [
    ConvertSpec::new(
        "VCVTPS2PD",
        0,
        false,
        0x5A,
        FpPrecision { from: F32, to: F64 },
    ),
    ConvertSpec::new(
        "VCVTPD2PS",
        1,
        true,
        0x5A,
        FpPrecision { from: F64, to: F32 },
    ),
    ConvertSpec::new(
        "VCVTDQ2PS",
        0,
        false,
        0x5B,
        IntToFp {
            int_elem: I32,
            fp_elem: F32,
            signed: true,
        },
    ),
    ConvertSpec::new(
        "VCVTQQ2PS",
        0,
        true,
        0x5B,
        IntToFp {
            int_elem: I64,
            fp_elem: F32,
            signed: true,
        },
    ),
    ConvertSpec::new(
        "VCVTDQ2PD",
        2,
        false,
        0xE6,
        IntToFp {
            int_elem: I32,
            fp_elem: F64,
            signed: true,
        },
    ),
    ConvertSpec::new(
        "VCVTQQ2PD",
        2,
        true,
        0xE6,
        IntToFp {
            int_elem: I64,
            fp_elem: F64,
            signed: true,
        },
    ),
    ConvertSpec::new(
        "VCVTUDQ2PS",
        3,
        false,
        0x7A,
        IntToFp {
            int_elem: I32,
            fp_elem: F32,
            signed: false,
        },
    ),
    ConvertSpec::new(
        "VCVTUQQ2PS",
        3,
        true,
        0x7A,
        IntToFp {
            int_elem: I64,
            fp_elem: F32,
            signed: false,
        },
    ),
    ConvertSpec::new(
        "VCVTUDQ2PD",
        2,
        false,
        0x7A,
        IntToFp {
            int_elem: I32,
            fp_elem: F64,
            signed: false,
        },
    ),
    ConvertSpec::new(
        "VCVTUQQ2PD",
        2,
        true,
        0x7A,
        IntToFp {
            int_elem: I64,
            fp_elem: F64,
            signed: false,
        },
    ),
    ConvertSpec::new(
        "VCVTPS2DQ",
        1,
        false,
        0x5B,
        FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: true,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPS2DQ",
        2,
        false,
        0x5B,
        FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: true,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPD2DQ",
        3,
        true,
        0xE6,
        FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: true,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPD2DQ",
        1,
        true,
        0xE6,
        FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: true,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPS2QQ",
        1,
        false,
        0x7B,
        FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: true,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPS2QQ",
        1,
        false,
        0x7A,
        FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: true,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPD2QQ",
        1,
        true,
        0x7B,
        FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: true,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPD2QQ",
        1,
        true,
        0x7A,
        FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: true,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPS2UDQ",
        0,
        false,
        0x79,
        FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: false,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPS2UDQ",
        0,
        false,
        0x78,
        FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: false,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPD2UDQ",
        0,
        true,
        0x79,
        FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: false,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPD2UDQ",
        0,
        true,
        0x78,
        FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: false,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPS2UQQ",
        1,
        false,
        0x79,
        FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: false,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPS2UQQ",
        1,
        false,
        0x78,
        FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: false,
            truncate: true,
        },
    ),
    ConvertSpec::new(
        "VCVTPD2UQQ",
        1,
        true,
        0x79,
        FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: false,
            truncate: false,
        },
    ),
    ConvertSpec::new(
        "VCVTTPD2UQQ",
        1,
        true,
        0x78,
        FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: false,
            truncate: true,
        },
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceForm {
    Vector,
    Broadcast,
}

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
pub(super) struct ConvertCase {
    pub(super) spec: ConvertSpec,
    pub(super) ll: u8,
    pub(super) destination: u8,
    pub(super) form: SourceForm,
    pub(super) control: MaskControl,
}

impl ConvertCase {
    pub(super) const fn operation_width(self) -> VecWidth {
        match self.ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!(),
        }
    }

    pub(super) fn widths(self) -> (u8, VecWidth, VecWidth) {
        let operation_bytes = self.operation_width().bytes();
        let source_bytes = self.spec.source_elem().bytes();
        let destination_bytes = self.spec.destination_elem().bytes();
        let (lanes, source_total, destination_total) = if destination_bytes >= source_bytes {
            let lanes = operation_bytes / destination_bytes;
            (lanes, lanes * source_bytes, operation_bytes)
        } else {
            let lanes = operation_bytes / source_bytes;
            (lanes, operation_bytes, lanes * destination_bytes)
        };
        (
            lanes as u8,
            exact_width(source_total),
            register_width(destination_total),
        )
    }

    pub(super) fn lanes(self) -> u8 {
        self.widths().0
    }

    pub(super) fn source_width(self) -> VecWidth {
        self.widths().1
    }

    pub(super) fn destination_width(self) -> VecWidth {
        self.widths().2
    }

    pub(super) const fn mask(self) -> u8 {
        self.control.fields().0
    }

    pub(super) const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    pub(super) const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    pub(super) fn memory_size(self) -> u32 {
        if self.broadcast() {
            self.spec.source_elem().bytes()
        } else {
            self.source_width().bytes()
        }
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        assert!(self.ll < 3 && self.destination < 32 && matches!(self.spec.map, 1 | 2));
        let p0 = self.spec.map
            | if self.destination & 8 == 0 { 0x80 } else { 0 }
            | if self.destination & 16 == 0 { 0x10 } else { 0 }
            | 0x60;
        vec![
            0x62,
            p0,
            (u8::from(self.spec.w) << 7) | 0x7C | self.spec.pp,
            (u8::from(self.zeroing()) << 7)
                | (self.ll << 5)
                | (u8::from(self.broadcast()) << 4)
                | 0x08
                | self.mask(),
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
        if self.broadcast() || self.mask() != 0 {
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
                memory[3] & !0x10,
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

fn register_width(bytes: u32) -> VecWidth {
    match bytes {
        0..=16 => VecWidth::V128,
        17..=32 => VecWidth::V256,
        _ => VecWidth::V512,
    }
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
        X86InstructionBytes::new(bytes).expect("packed conversion provenance"),
    );
    function
}

pub(super) fn lift_case(case: ConvertCase) -> SmirFunction {
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
) -> Option<X86JitEvexPackedConvertMemorySequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_packed_convert_memory_sequence(
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

pub(super) fn lower(function: &SmirFunction, case: ConvertCase) -> (Vec<u8>, usize) {
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
        requirements.needs_avx512dq,
        case.spec.needs_avx512dq(),
        "{case:?}"
    );
    assert_eq!(
        requirements.has_k16_opmask_span,
        case.mask() != 0,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512fp16, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed packed conversion: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    let code = lowerer.finalize().expect("finalize packed conversion");
    assert!(
        contains_bytes(&code, &case.expected_replay()),
        "{case:?}: missing replay {:02X?}",
        case.expected_replay()
    );
    (code, result.entry_offset)
}

pub(super) fn replay_kind(sequence: X86JitEvexPackedConvertMemorySequence) -> &'static str {
    match sequence.encoding.replay {
        X86EvexPackedConvertMemoryReplay::Vector { .. } => "vector",
        X86EvexPackedConvertMemoryReplay::Broadcast { .. } => "broadcast",
        X86EvexPackedConvertMemoryReplay::MaskedVector { .. } => "masked-vector",
    }
}
