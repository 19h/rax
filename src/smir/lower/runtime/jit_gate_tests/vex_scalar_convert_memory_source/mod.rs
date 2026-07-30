//! Exact helper-backed VEX scalar-conversion memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FpRoundMode, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexScalarConvertMemoryEncoding,
    X86VexScalarConvertMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitVexScalarConvertMemorySequence, X86NativeReplayFeatureRequirements,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_scalar_convert_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_ZMM_OFFSET, X86_STATE_PTR_AT_RBP,
};
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

mod semantics;

const PC: u64 = 0x5A2A_2D2C;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConvertKind {
    F32ToF64,
    F64ToF32,
    I32Or64ToF32,
    I32Or64ToF64,
    F32ToI32Or64,
    F64ToI32Or64,
    F32ToI32Or64Truncate,
    F64ToI32Or64Truncate,
}

impl ConvertKind {
    const ALL: [Self; 8] = [
        Self::F32ToF64,
        Self::F64ToF32,
        Self::I32Or64ToF32,
        Self::I32Or64ToF64,
        Self::F32ToI32Or64,
        Self::F64ToI32Or64,
        Self::F32ToI32Or64Truncate,
        Self::F64ToI32Or64Truncate,
    ];

    const fn pp(self) -> u8 {
        match self {
            Self::F32ToF64
            | Self::I32Or64ToF32
            | Self::F32ToI32Or64
            | Self::F32ToI32Or64Truncate => 2,
            Self::F64ToF32
            | Self::I32Or64ToF64
            | Self::F64ToI32Or64
            | Self::F64ToI32Or64Truncate => 3,
        }
    }

    const fn opcode(self) -> u8 {
        match self {
            Self::F32ToF64 | Self::F64ToF32 => 0x5A,
            Self::I32Or64ToF32 | Self::I32Or64ToF64 => 0x2A,
            Self::F32ToI32Or64 | Self::F64ToI32Or64 => 0x2D,
            Self::F32ToI32Or64Truncate | Self::F64ToI32Or64Truncate => 0x2C,
        }
    }

    const fn has_merge(self) -> bool {
        matches!(
            self,
            Self::F32ToF64 | Self::F64ToF32 | Self::I32Or64ToF32 | Self::I32Or64ToF64
        )
    }

    const fn is_fp_convert(self) -> bool {
        matches!(self, Self::F32ToF64 | Self::F64ToF32)
    }

    const fn is_int_to_fp(self) -> bool {
        matches!(self, Self::I32Or64ToF32 | Self::I32Or64ToF64)
    }

    const fn is_fp_to_int(self) -> bool {
        !self.is_fp_convert() && !self.is_int_to_fp()
    }

    const fn source_element(self) -> Option<VecElementType> {
        match self {
            Self::F32ToF64 | Self::F32ToI32Or64 | Self::F32ToI32Or64Truncate => {
                Some(VecElementType::F32)
            }
            Self::F64ToF32 | Self::F64ToI32Or64 | Self::F64ToI32Or64Truncate => {
                Some(VecElementType::F64)
            }
            Self::I32Or64ToF32 | Self::I32Or64ToF64 => None,
        }
    }

    const fn destination_element(self) -> Option<VecElementType> {
        match self {
            Self::F32ToF64 | Self::I32Or64ToF64 => Some(VecElementType::F64),
            Self::F64ToF32 | Self::I32Or64ToF32 => Some(VecElementType::F32),
            _ => None,
        }
    }

    const fn truncate(self) -> bool {
        matches!(
            self,
            Self::F32ToI32Or64Truncate | Self::F64ToI32Or64Truncate
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4 { w: bool, encoded_x_clear: bool },
}

impl VexForm {
    const SCANNER_FORMS: [Self; 3] = [
        Self::C5,
        Self::C4 {
            w: false,
            encoded_x_clear: false,
        },
        Self::C4 {
            w: true,
            encoded_x_clear: true,
        },
    ];

    const fn w(self) -> bool {
        match self {
            Self::C5 => false,
            Self::C4 { w, .. } => w,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConvertCase {
    kind: ConvertKind,
    form: VexForm,
    destination: u8,
    merge: u8,
    base: u8,
}

impl ConvertCase {
    fn int_width(self) -> OpWidth {
        if self.form.w() {
            OpWidth::W64
        } else {
            OpWidth::W32
        }
    }

    fn memory_size(self) -> u32 {
        if self.kind.is_int_to_fp() {
            self.int_width().bytes()
        } else {
            self.kind
                .source_element()
                .expect("FP-source conversion has an element type")
                .bytes()
        }
    }

    fn scratch(self) -> Option<u8> {
        if self.kind.is_fp_convert() {
            Some(
                (0..8)
                    .find(|candidate| *candidate != self.destination && *candidate != self.merge)
                    .expect("two vector operands leave six low XMM scratch registers"),
            )
        } else if self.kind.is_fp_to_int() {
            Some(0)
        } else {
            None
        }
    }

    fn register_source(self) -> u8 {
        self.scratch().unwrap_or(0)
    }

    fn vex_p0(self, register_source: Option<u8>) -> u8 {
        let VexForm::C4 {
            encoded_x_clear, ..
        } = self.form
        else {
            unreachable!("C5 has no three-byte VEX P0")
        };
        let source_or_base = register_source.unwrap_or(self.base);
        (if self.destination < 8 { 0x80 } else { 0 })
            | (if encoded_x_clear { 0 } else { 0x40 })
            | (if source_or_base < 8 { 0x20 } else { 0 })
            | 1
    }

    fn vex_p1(self) -> u8 {
        let encoded_vvvv = if self.kind.has_merge() {
            ((!self.merge) & 15) << 3
        } else {
            0x78
        };
        (u8::from(self.form.w()) << 7) | encoded_vvvv | self.kind.pp()
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.merge < 16 && self.base < 16);
        let modrm = 0x40 | ((self.destination & 7) << 3) | (self.base & 7);
        let mut bytes = match self.form {
            VexForm::C5 => {
                assert!(self.base < 8, "C5 has no VEX.B extension");
                vec![
                    0xC5,
                    (if self.destination < 8 { 0x80 } else { 0 }) | (self.vex_p1() & 0x7F),
                    self.kind.opcode(),
                    modrm,
                ]
            }
            VexForm::C4 { .. } => vec![
                0xC4,
                self.vex_p0(None),
                self.vex_p1(),
                self.kind.opcode(),
                modrm,
            ],
        };
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(DISP as u8);
        bytes
    }

    fn register_bytes_with_destination(self, destination: u8) -> Vec<u8> {
        let mut rewritten = self;
        rewritten.destination = destination;
        let source = self.register_source();
        let modrm = 0xC0 | ((destination & 7) << 3) | source;
        match self.form {
            VexForm::C5 => vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 }) | (self.vex_p1() & 0x7F),
                self.kind.opcode(),
                modrm,
            ],
            VexForm::C4 { .. } => vec![
                0xC4,
                rewritten.vex_p0(Some(source)),
                self.vex_p1(),
                self.kind.opcode(),
                modrm,
            ],
        }
    }

    fn register_bytes(self) -> Vec<u8> {
        self.register_bytes_with_destination(self.destination)
    }

    fn expected_kind(self) -> X86VexScalarConvertMemoryKind {
        if self.kind.is_fp_convert() {
            X86VexScalarConvertMemoryKind::FpConvert {
                from: self.kind.source_element().unwrap(),
                to: self.kind.destination_element().unwrap(),
            }
        } else if self.kind.is_int_to_fp() {
            X86VexScalarConvertMemoryKind::IntToFp {
                elem: self.kind.destination_element().unwrap(),
                int_width: self.int_width(),
            }
        } else {
            X86VexScalarConvertMemoryKind::FpToInt {
                elem: self.kind.source_element().unwrap(),
                int_width: self.int_width(),
                truncate: self.kind.truncate(),
            }
        }
    }

    fn expected_encoding(self) -> X86VexScalarConvertMemoryEncoding {
        X86VexScalarConvertMemoryEncoding {
            kind: self.expected_kind(),
            destination: self.destination,
            merge: self.kind.has_merge().then_some(self.merge),
            vector_scratch: self.scratch(),
            memory_size: self.memory_size(),
            w: self.form.w(),
            pp: self.kind.pp(),
            opcode: self.kind.opcode(),
            register_instruction: X86InstructionBytes::new(&self.register_bytes()).unwrap(),
        }
    }

    fn expected_replay_bytes(self) -> Vec<u8> {
        if !self.kind.is_fp_to_int() || !matches!(self.destination, 4 | 5) {
            return self.register_bytes();
        }

        let mut bytes = vec![0x50, 0x51];
        bytes.extend_from_slice(&self.register_bytes_with_destination(0));
        bytes.extend_from_slice(&[
            0x48,
            0x8B,
            0x4D,
            X86_STATE_PTR_AT_RBP as u8,
            0x48,
            0x89,
            0x41,
            self.destination * 8,
        ]);
        if self.destination == 5 {
            bytes.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]);
        }
        bytes.extend_from_slice(&[0x59, 0x58]);
        bytes
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn classified_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitVexScalarConvertMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_scalar_convert_memory_sequence(
        block,
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexScalarConvertMemorySequence> {
    classified_at(function, 0, allow_mem)
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits source metadata"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: ConvertCase) {
    let block = &function.blocks[0];
    assert_eq!(block.ops.len(), 2, "{case:?}");
    let loaded = match &block.ops[0].kind {
        OpKind::Load {
            dst: loaded @ VReg::Virtual(_),
            width,
            sign,
            ..
        } => {
            assert_eq!(
                *width,
                if case.memory_size() == 4 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                },
                "{case:?}"
            );
            assert_eq!(
                *sign,
                if case.kind.is_int_to_fp() {
                    SignExtend::Sign
                } else {
                    SignExtend::Zero
                },
                "{case:?}"
            );
            assert_eq!(block.ops[0].x86_hint, None, "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected leading scalar Load, got {other:?}"),
    };
    assert_eq!(block.ops[1].guest_pc, PC, "{case:?}");
    assert_eq!(
        block.ops[1].x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: if case.kind.pp() == 2 {
                X86SsePrefix::Rep
            } else {
                X86SsePrefix::Repne
            },
            opcode: case.kind.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        }),
        "{case:?}"
    );

    match (&block.ops[1].kind, case.expected_kind()) {
        (
            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing,
                round,
                suppress_exceptions,
                zero_upper,
            },
            X86VexScalarConvertMemoryKind::FpConvert {
                from: expected_from,
                to: expected_to,
            },
        ) => {
            assert_eq!(*dst, xmm(case.destination), "{case:?}");
            assert_eq!(*merge, xmm(case.merge), "{case:?}");
            assert_eq!(*src, loaded, "{case:?}");
            assert_eq!(*mask, None, "{case:?}");
            assert_eq!(*from, expected_from, "{case:?}");
            assert_eq!(*to, expected_to, "{case:?}");
            assert!(!*mask_zeroing, "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert!(!*suppress_exceptions, "{case:?}");
            assert!(*zero_upper, "{case:?}");
        }
        (
            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem,
                int_width,
                signed,
                round,
                suppress_exceptions,
                zero_upper,
            },
            X86VexScalarConvertMemoryKind::IntToFp {
                elem: expected_elem,
                int_width: expected_width,
            },
        ) => {
            assert_eq!(*dst, xmm(case.destination), "{case:?}");
            assert_eq!(*merge, xmm(case.merge), "{case:?}");
            assert_eq!(*src, loaded, "{case:?}");
            assert_eq!(*elem, expected_elem, "{case:?}");
            assert_eq!(*int_width, expected_width, "{case:?}");
            assert!(*signed, "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert!(!*suppress_exceptions, "{case:?}");
            assert!(*zero_upper, "{case:?}");
        }
        (
            OpKind::X86FpToInt {
                dst,
                src,
                elem,
                int_width,
                signed,
                truncate,
                round,
                suppress_exceptions,
            },
            X86VexScalarConvertMemoryKind::FpToInt {
                elem: expected_elem,
                int_width: expected_width,
                truncate: expected_truncate,
            },
        ) => {
            assert_eq!(*dst, x86(X86Reg::gpr(case.destination)), "{case:?}");
            assert_eq!(*src, loaded, "{case:?}");
            assert_eq!(*elem, expected_elem, "{case:?}");
            assert_eq!(*int_width, expected_width, "{case:?}");
            assert!(*signed, "{case:?}");
            assert_eq!(*truncate, expected_truncate, "{case:?}");
            assert_eq!(
                *round,
                if expected_truncate {
                    FpRoundMode::RoundTowardZero
                } else {
                    FpRoundMode::Dynamic
                },
                "{case:?}"
            );
            assert!(!*suppress_exceptions, "{case:?}");
        }
        (actual, expected) => panic!("{case:?}: {actual:?} does not match {expected:?}"),
    }

    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexScalarConvertMemorySequence {
            consumed: 2,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lift_case(case: ConvertCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn assert_feature_requirements(function: &SmirFunction, case: ConvertCase) {
    let excluded = HashMap::new();
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

    let mut expected = X86NativeReplayFeatureRequirements::default();
    expected.any = true;
    expected.all_spans_support_avx_ymm16 = true;
    expected.needs_avx = true;
    assert_eq!(
        x86_native_replay_feature_requirements(function, &excluded),
        expected,
        "{case:?}"
    );
}

fn lower(function: &SmirFunction, case: ConvertCase) -> (Vec<u8>, usize) {
    assert_exact_lift_and_sequence(function, case);
    assert_feature_requirements(function, case);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed scalar conversion failed: {error:?}"));
    assert!(result.relocations.is_empty());
    let code = lowerer
        .finalize()
        .expect("finalize helper-backed VEX scalar conversion");
    let expected = case.expected_replay_bytes();
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{case:?}: missing replay {expected:02X?}"
    );
    let scratch_offset = X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes();
    assert!(
        code.windows(scratch_offset.len())
            .any(|window| window == scratch_offset),
        "{case:?}: helper scratch offset absent"
    );
    if case.kind.is_int_to_fp() {
        let mut expected_load = Vec::new();
        if case.int_width() == OpWidth::W64 {
            expected_load.push(0x48);
        }
        expected_load.extend_from_slice(&[0x8B, 0x80]);
        expected_load.extend_from_slice(&scratch_offset);
        assert!(
            code.windows(expected_load.len())
                .any(|window| window == expected_load),
            "{case:?}: integer transfer absent"
        );
    }
    (code, result.entry_offset)
}

#[test]
fn all_1632_scanner_cells_admit_and_lower_exactly_at_o0_o1_o2() {
    let mut cells = 0usize;
    let mut lowered = 0usize;
    for kind in ConvertKind::ALL {
        for form in VexForm::SCANNER_FORMS {
            for destination in 0..8 {
                let merges: &[u8] = if kind.has_merge() {
                    &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
                } else {
                    &[0]
                };
                for &merge in merges {
                    let case = ConvertCase {
                        kind,
                        form,
                        destination,
                        merge,
                        base: 2,
                    };
                    cells += 1;
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        lower(&function, case);
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cells, 1_632);
    assert_eq!(lowered, 1_632 * LEVELS.len());
}

#[test]
fn high_operands_full_address_shapes_and_ignored_vex_fields_remain_exact() {
    let cases: &[(ConvertCase, &[u8])] = &[
        (
            ConvertCase {
                kind: ConvertKind::F64ToF32,
                form: VexForm::C5,
                destination: 9,
                merge: 2,
                base: 5,
            },
            &[0x64, 0xC5, 0x6B, 0x5A, 0x0D, 0x11, 0x22, 0x33, 0x44],
        ),
        (
            ConvertCase {
                kind: ConvertKind::I32Or64ToF64,
                form: VexForm::C4 {
                    w: true,
                    encoded_x_clear: true,
                },
                destination: 9,
                merge: 2,
                base: 12,
            },
            &[0x65, 0xC4, 0x01, 0xEB, 0x2A, 0x4C, 0xEC, 0x20],
        ),
        (
            ConvertCase {
                kind: ConvertKind::F32ToI32Or64,
                form: VexForm::C4 {
                    w: false,
                    encoded_x_clear: false,
                },
                destination: 14,
                merge: 0,
                base: 5,
            },
            &[
                0x67, 0xC4, 0x61, 0x7A, 0x2D, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
            ],
        ),
        (
            ConvertCase {
                kind: ConvertKind::F32ToF64,
                form: VexForm::C4 {
                    w: true,
                    encoded_x_clear: false,
                },
                destination: 0,
                merge: 15,
                base: 5,
            },
            &[0x65, 0xC4, 0xE1, 0x82, 0x5A, 0x45, 0x00],
        ),
    ];
    let mut lowered = 0usize;
    for &(case, bytes) in cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            assert_exact_lift_and_sequence(&function, case);
            lower(&function, case);
            lowered += 1;
        }
    }
    assert_eq!(lowered, cases.len() * LEVELS.len());
}

#[test]
fn vex_l1_and_reserved_or_nonexact_source_images_fail_closed() {
    let mut rejected = 0usize;
    for kind in ConvertKind::ALL {
        for form in VexForm::SCANNER_FORMS {
            let case = ConvertCase {
                kind,
                form,
                destination: 7,
                merge: if kind.has_merge() { 15 } else { 0 },
                base: 2,
            };
            let mut l1 = case.bytes();
            let p1 = if matches!(form, VexForm::C5) { 1 } else { 2 };
            l1[p1] |= 0x04;
            let function = optimize(lift_bytes(&l1), OptLevel::O2);
            assert_eq!(classified_sequence(&function, true), None, "{case:?}");
            assert!(
                !is_native_clobber_safe_excluding(&function, &HashMap::new(), true),
                "{case:?}"
            );
            rejected += 1;
        }
    }

    let case = ConvertCase {
        kind: ConvertKind::F64ToI32Or64,
        form: VexForm::C4 {
            w: true,
            encoded_x_clear: true,
        },
        destination: 5,
        merge: 0,
        base: 11,
    };
    let valid = case.bytes();
    let mut invalid = Vec::new();
    for (index, xor) in [(1, 0x03), (2, 0x02), (3, 0x10), (4, 0xC0)] {
        let mut bytes = valid.clone();
        bytes[index] ^= xor;
        invalid.push(bytes);
    }
    let mut reserved_vvvv = valid.clone();
    reserved_vvvv[2] &= !0x08;
    invalid.push(reserved_vvvv);
    let mut trailing = valid.clone();
    trailing.push(0);
    invalid.push(trailing);
    let mut forbidden_prefix = valid.clone();
    forbidden_prefix.insert(0, 0x66);
    invalid.push(forbidden_prefix);
    for bytes in invalid {
        let metadata = X86InstructionBytes::new(&bytes);
        assert!(
            metadata
                .and_then(|instruction| instruction.vex_scalar_convert_memory_encoding())
                .is_none(),
            "{bytes:02X?}"
        );
        rejected += 1;
    }
    assert_eq!(
        rejected,
        ConvertKind::ALL.len() * VexForm::SCANNER_FORMS.len() + 7
    );
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: sequence classifier admitted malformed IR"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed IR"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed IR"
    );
}

fn loaded_virtual(function: &SmirFunction) -> VReg {
    match function.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    }
}

fn common_malformed_functions(case: ConvertCase) -> Vec<(&'static str, SmirFunction)> {
    let base = lift_case(case);
    let loaded = loaded_virtual(&base);
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata
        .x86_instruction_bytes
        .remove(&(BlockId(0), PC));
    malformed.push(("missing source bytes", missing_metadata));

    let mut register_metadata = base.clone();
    register_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&case.register_bytes()).unwrap(),
    );
    malformed.push(("register-form source bytes", register_metadata));

    let mut trailing_metadata = base.clone();
    let mut trailing = case.bytes();
    trailing.push(0);
    trailing_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&trailing).unwrap(),
    );
    malformed.push(("trailing source byte", trailing_metadata));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x10,
        width: VecWidth::V128,
        w: false,
    });
    malformed.push(("invented load hint", load_hint));

    let mut wrong_width = base.clone();
    if let OpKind::Load { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = if case.memory_size() == 4 {
            MemWidth::B8
        } else {
            MemWidth::B4
        };
    }
    malformed.push(("load width", wrong_width));

    let mut wrong_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut wrong_sign.blocks[0].ops[0].kind {
        *sign = if case.kind.is_int_to_fp() {
            SignExtend::Zero
        } else {
            SignExtend::Sign
        };
    }
    malformed.push(("load extension", wrong_sign));

    let mut architectural_load = base.clone();
    if let OpKind::Load { dst, .. } = &mut architectural_load.blocks[0].ops[0].kind {
        *dst = x86(X86Reg::Rax);
    }
    malformed.push(("architectural load destination", architectural_load));

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFF),
        PC + 1,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value escapes", external_use));

    let mut duplicate_definition = base.clone();
    duplicate_definition.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFE),
        PC + 1,
        OpKind::Mov {
            dst: loaded,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("loaded value redefined", duplicate_definition));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut wrong_hint = base.clone();
    wrong_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::Rep,
        opcode: case.kind.opcode(),
        width: VecWidth::V128,
        w: case.form.w(),
    });
    malformed.push(("consumer provenance hint", wrong_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFD), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    malformed
}

#[test]
fn classifier_gate_and_lowerer_reject_every_common_graph_and_provenance_mutation() {
    let cases = [
        ConvertCase {
            kind: ConvertKind::F32ToF64,
            form: VexForm::C4 {
                w: true,
                encoded_x_clear: true,
            },
            destination: 9,
            merge: 10,
            base: 11,
        },
        ConvertCase {
            kind: ConvertKind::I32Or64ToF64,
            form: VexForm::C4 {
                w: true,
                encoded_x_clear: false,
            },
            destination: 12,
            merge: 13,
            base: 14,
        },
        ConvertCase {
            kind: ConvertKind::F32ToI32Or64,
            form: VexForm::C4 {
                w: false,
                encoded_x_clear: true,
            },
            destination: 5,
            merge: 0,
            base: 11,
        },
    ];
    let mut rejected = 0usize;
    for case in cases {
        for (name, function) in common_malformed_functions(case) {
            assert_rejected(name, &function);
            rejected += 1;
        }

        let base = lift_case(case);
        let mut same_pc_head = base;
        same_pc_head.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0x7FFC), PC, OpKind::Nop));
        assert_eq!(classified_at(&same_pc_head, 1, true), None, "{case:?}");
        assert_rejected("unconsumed same-PC head", &same_pc_head);
        rejected += 1;
    }
    assert_eq!(rejected, cases.len() * 14);
}

#[test]
fn classifier_rejects_each_consumer_semantic_and_hint_field_mutation() {
    let fp_case = ConvertCase {
        kind: ConvertKind::F32ToF64,
        form: VexForm::C4 {
            w: true,
            encoded_x_clear: true,
        },
        destination: 9,
        merge: 10,
        base: 11,
    };
    let mut fp_mutations = Vec::new();
    let fp_base = lift_case(fp_case);
    macro_rules! mutate_fp {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = fp_base.clone();
            let OpKind::X86FpConvert { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            fp_mutations.push(($name, function));
        }};
    }
    mutate_fp!("FP destination", dst, xmm(8));
    mutate_fp!("FP merge", merge, xmm(11));
    mutate_fp!("FP source", src, x86(X86Reg::Rax));
    mutate_fp!("FP mask", mask, Some(x86(X86Reg::K(1))));
    mutate_fp!("FP source element", from, VecElementType::F64);
    mutate_fp!("FP destination element", to, VecElementType::F32);
    mutate_fp!("FP mask zeroing", mask_zeroing, true);
    mutate_fp!("FP rounding", round, FpRoundMode::RoundNearest);
    mutate_fp!("FP exception suppression", suppress_exceptions, true);
    mutate_fp!("FP upper zeroing", zero_upper, false);

    let int_case = ConvertCase {
        kind: ConvertKind::I32Or64ToF64,
        form: VexForm::C4 {
            w: true,
            encoded_x_clear: false,
        },
        destination: 12,
        merge: 13,
        base: 14,
    };
    let mut int_mutations = Vec::new();
    let int_base = lift_case(int_case);
    macro_rules! mutate_int {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = int_base.clone();
            let OpKind::X86IntToFp { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            int_mutations.push(($name, function));
        }};
    }
    mutate_int!("integer destination", dst, xmm(11));
    mutate_int!("integer merge", merge, xmm(12));
    mutate_int!("integer source", src, x86(X86Reg::Rax));
    mutate_int!("integer FP element", elem, VecElementType::F32);
    mutate_int!("integer width", int_width, OpWidth::W32);
    mutate_int!("integer signedness", signed, false);
    mutate_int!("integer rounding", round, FpRoundMode::RoundDown);
    mutate_int!("integer exception suppression", suppress_exceptions, true);
    mutate_int!("integer upper zeroing", zero_upper, false);

    let fp_to_int_case = ConvertCase {
        kind: ConvertKind::F32ToI32Or64,
        form: VexForm::C4 {
            w: false,
            encoded_x_clear: true,
        },
        destination: 5,
        merge: 0,
        base: 11,
    };
    let mut fp_to_int_mutations = Vec::new();
    let fp_to_int_base = lift_case(fp_to_int_case);
    macro_rules! mutate_fp_to_int {
        ($name:literal, $field:ident, $value:expr) => {{
            let mut function = fp_to_int_base.clone();
            let OpKind::X86FpToInt { $field, .. } = &mut function.blocks[0].ops[1].kind else {
                unreachable!()
            };
            *$field = $value;
            fp_to_int_mutations.push(($name, function));
        }};
    }
    mutate_fp_to_int!("FP-to-int destination", dst, x86(X86Reg::Rax));
    mutate_fp_to_int!("FP-to-int source", src, x86(X86Reg::Rcx));
    mutate_fp_to_int!("FP-to-int element", elem, VecElementType::F64);
    mutate_fp_to_int!("FP-to-int width", int_width, OpWidth::W64);
    mutate_fp_to_int!("FP-to-int signedness", signed, false);
    mutate_fp_to_int!("FP-to-int truncation", truncate, true);
    mutate_fp_to_int!("FP-to-int rounding", round, FpRoundMode::RoundTowardZero);
    mutate_fp_to_int!("FP-to-int exception suppression", suppress_exceptions, true);

    let mut rejected = 0usize;
    for (name, function) in fp_mutations
        .into_iter()
        .chain(int_mutations)
        .chain(fp_to_int_mutations)
    {
        assert_rejected(name, &function);
        rejected += 1;
    }
    assert_eq!(rejected, 27);

    let mut hint_mutations = Vec::new();
    for (name, hint) in [
        (
            "hint map",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::Rep,
                opcode: 0x5A,
                width: VecWidth::V128,
                w: true,
            },
        ),
        (
            "hint prefix",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Repne,
                opcode: 0x5A,
                width: VecWidth::V128,
                w: true,
            },
        ),
        (
            "hint opcode",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x2A,
                width: VecWidth::V128,
                w: true,
            },
        ),
        (
            "hint width",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x5A,
                width: VecWidth::V256,
                w: true,
            },
        ),
        (
            "hint W",
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0x5A,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        let mut function = fp_base.clone();
        function.blocks[0].ops[1].x86_hint = Some(hint);
        hint_mutations.push((name, function));
    }
    let mut wrong_kind = fp_base;
    wrong_kind.blocks[0].ops[1].kind = OpKind::Nop;
    hint_mutations.push(("consumer operation kind", wrong_kind));
    for (name, function) in hint_mutations {
        assert_rejected(name, &function);
    }
}

#[test]
fn rsp_rbp_destinations_and_integer_transfer_widths_are_byte_exact() {
    let cases = [
        ConvertCase {
            kind: ConvertKind::F32ToI32Or64,
            form: VexForm::C5,
            destination: 4,
            merge: 0,
            base: 7,
        },
        ConvertCase {
            kind: ConvertKind::F64ToI32Or64Truncate,
            form: VexForm::C4 {
                w: true,
                encoded_x_clear: true,
            },
            destination: 5,
            merge: 0,
            base: 11,
        },
        ConvertCase {
            kind: ConvertKind::I32Or64ToF32,
            form: VexForm::C5,
            destination: 9,
            merge: 10,
            base: 4,
        },
        ConvertCase {
            kind: ConvertKind::I32Or64ToF64,
            form: VexForm::C4 {
                w: true,
                encoded_x_clear: false,
            },
            destination: 15,
            merge: 15,
            base: 12,
        },
    ];
    for level in [OptLevel::O0, OptLevel::O2] {
        for case in cases {
            lower(&optimize(lift_case(case), level), case);
        }
    }
}

#[test]
fn excluded_regions_contribute_no_features_and_aarch64_admission_stays_closed() {
    let case = ConvertCase {
        kind: ConvertKind::F64ToF32,
        form: VexForm::C4 {
            w: true,
            encoded_x_clear: true,
        },
        destination: 15,
        merge: 14,
        base: 11,
    };
    let function = lift_case(case);
    let excluded = HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &HashMap::new()
    ));
    assert!(is_x86_aarch64_native_clobber_safe_excluding(
        &function, &excluded
    ));

    let upper = X86_GUEST_ZMM_OFFSET + i32::from(case.destination) * 64 + 32;
    let (code, _) = lower(&function, case);
    assert!(
        code.windows(4)
            .any(|window| window == (upper as u32).to_le_bytes())
    );
}
