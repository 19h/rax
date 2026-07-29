//! Exact helper-backed VEX VCOMI/VUCOMI memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, X86JitVexFpFlagCompareMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_fp_flag_compare_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0x2E2F;
const DISP: i64 = 0x20;
const STATUS_FLAGS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    F32,
    F64,
}

impl Format {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    const fn elem(self) -> VecElementType {
        match self {
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::F32 => 0,
            Self::F64 => 1,
        }
    }

    const fn memory_size(self) -> u32 {
        match self {
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    const fn bit_mask(self) -> u64 {
        match self {
            Self::F32 => u32::MAX as u64,
            Self::F64 => u64::MAX,
        }
    }

    const fn sign_mask(self) -> u64 {
        match self {
            Self::F32 => 1 << 31,
            Self::F64 => 1 << 63,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    C5,
    C4W0,
    C4W1,
}

impl EncodingForm {
    const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];

    const fn w(self) -> bool {
        matches!(self, Self::C4W1)
    }

    const fn ordinal(self) -> usize {
        match self {
            Self::C5 => 0,
            Self::C4W0 => 1,
            Self::C4W1 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlagCompareMemoryCase {
    format: Format,
    signaling: bool,
    form: EncodingForm,
    source1: u8,
    base: u8,
}

impl FlagCompareMemoryCase {
    const fn opcode(self) -> u8 {
        if self.signaling { 0x2F } else { 0x2E }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.source1)
            .expect("one source leaves at least fifteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let modrm = 0x40 | ((self.source1 & 7) << 3) | (self.base & 7);
        match self.form {
            EncodingForm::C5 => {
                assert!(self.base < 8);
                vec![
                    0xC5,
                    (if self.source1 < 8 { 0x80 } else { 0 }) | 0x78 | self.format.pp(),
                    self.opcode(),
                    modrm,
                    DISP as u8,
                ]
            }
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if self.source1 < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if self.base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7) | 0x78 | self.format.pp(),
                self.opcode(),
                modrm,
                DISP as u8,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let scratch = self.scratch();
        let modrm = 0xC0 | ((self.source1 & 7) << 3) | scratch;
        if !self.form.w() {
            vec![
                0xC5,
                (if self.source1 < 8 { 0x80 } else { 0 }) | 0x78 | self.format.pp(),
                self.opcode(),
                modrm,
            ]
        } else {
            vec![
                0xC4,
                (if self.source1 < 8 { 0x80 } else { 0 }) | 0x60 | 1,
                0x80 | 0x78 | self.format.pp(),
                self.opcode(),
                modrm,
            ]
        }
    }
}

fn all_cases() -> Vec<FlagCompareMemoryCase> {
    let mut cases = Vec::new();
    for format in Format::ALL {
        for signaling in [false, true] {
            for form in EncodingForm::ALL {
                let base = match form {
                    EncodingForm::C5 => 3,
                    EncodingForm::C4W0 => 11,
                    EncodingForm::C4W1 => 14,
                };
                for source1 in 0..16 {
                    cases.push(FlagCompareMemoryCase {
                        format,
                        signaling,
                        form,
                        source1,
                        base,
                    });
                }
            }
        }
    }
    cases
}

fn scanner_cases() -> Vec<FlagCompareMemoryCase> {
    let mut cases = Vec::new();
    for format in Format::ALL {
        for signaling in [false, true] {
            for form in EncodingForm::ALL {
                for source1 in 0..8 {
                    cases.push(FlagCompareMemoryCase {
                        format,
                        signaling,
                        form,
                        source1,
                        base: 2,
                    });
                }
            }
        }
    }
    cases
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn expected_address(case: FlagCompareMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
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

fn classified_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitVexFpFlagCompareMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_fp_flag_compare_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits metadata"),
    );
    function
}

fn lift_case(case: FlagCompareMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_sequence(function: &SmirFunction, case: FlagCompareMemoryCase) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 3, "{case:?}");
    let loaded_scalar = match &ops[0].kind {
        OpKind::Load {
            dst: scalar @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(
                *width,
                if case.format == Format::F32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                },
                "{case:?}"
            );
            assert_eq!(ops[0].x86_hint, None, "{case:?}");
            *scalar
        }
        other => panic!("{case:?}: expected scalar Load, got {other:?}"),
    };
    let source2 = match ops[1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: 1,
        } => {
            assert_eq!(scalar, loaded_scalar, "{case:?}");
            assert_eq!(elem, case.format.elem(), "{case:?}");
            assert_eq!(ops[1].x86_hint, None, "{case:?}");
            vector
        }
        ref other => panic!("{case:?}: expected source broadcast, got {other:?}"),
    };
    assert!(
        matches!(
            ops[2].kind,
            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
                suppress_exceptions: false,
            } if src1 == x86(X86Reg::Xmm(case.source1))
                && src2 == source2
                && elem == case.format.elem()
                && signaling == case.signaling
        ),
        "{case:?}: {:?}",
        ops[2].kind
    );
    assert_eq!(
        ops[2].x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: if case.format == Format::F32 {
                X86SsePrefix::None
            } else {
                X86SsePrefix::OpSize
            },
            opcode: case.opcode(),
            width: VecWidth::V128,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexFpFlagCompareMemorySequence {
            consumed: 3,
            memory_size: case.format.memory_size(),
            source1: case.source1,
            elem: case.format.elem(),
            signaling: case.signaling,
            w: case.form.w(),
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize, X86JitVexFpFlagCompareMemorySequence) {
    let sequence = classified_sequence(function, true).expect("classified VEX flag compare");
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

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx")
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX flag compare failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX flag compare"),
        result.entry_offset,
        sequence,
    )
}

#[test]
fn all_288_scanner_encoding_and_optimization_cells_admit_and_lower() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 96);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_sequence(&function, case);
            let (code, _, _) = lower(&function);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector scratch index"
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xB9, case.format.memory_size() as u8, 0, 0, 0]),
                "{level:?} {case:?}: missing memory byte size"
            );
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 288);
}

#[test]
fn llvm_23_memory_encodings_match_the_generators() {
    for (case, expected) in [
        (
            FlagCompareMemoryCase {
                format: Format::F32,
                signaling: false,
                form: EncodingForm::C5,
                source1: 1,
                base: 7,
            },
            &[0xC5, 0xF8, 0x2E, 0x4F, 0x20][..],
        ),
        (
            FlagCompareMemoryCase {
                format: Format::F32,
                signaling: true,
                form: EncodingForm::C5,
                source1: 2,
                base: 7,
            },
            &[0xC5, 0xF8, 0x2F, 0x57, 0x20][..],
        ),
        (
            FlagCompareMemoryCase {
                format: Format::F64,
                signaling: false,
                form: EncodingForm::C5,
                source1: 3,
                base: 7,
            },
            &[0xC5, 0xF9, 0x2E, 0x5F, 0x20][..],
        ),
        (
            FlagCompareMemoryCase {
                format: Format::F64,
                signaling: true,
                form: EncodingForm::C5,
                source1: 4,
                base: 7,
            },
            &[0xC5, 0xF9, 0x2F, 0x67, 0x20][..],
        ),
    ] {
        assert_eq!(case.bytes(), expected, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_high_register_and_addr32_shapes_admit() {
    let encodings: &[&[u8]] = &[
        // vucomiss xmm1,[rip+0x44332211]
        &[0xC5, 0xF8, 0x2E, 0x0D, 0x11, 0x22, 0x33, 0x44],
        // vcomiss xmm3,fs:[rcx*4+0x44332211]
        &[0x64, 0xC5, 0xF8, 0x2F, 0x1C, 0x8D, 0x11, 0x22, 0x33, 0x44],
        // vcomisd xmm14,fs:addr32 [r14d+r15d*2+0x44332211]
        &[
            0x64, 0x67, 0xC4, 0x01, 0xF9, 0x2F, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ],
    ];
    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let (_, _, sequence) = lower(&function);
            assert!(matches!(sequence.memory_size, 4 | 8));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert_eq!(
        classified_sequence(function, true),
        None,
        "{name}: classifier admitted malformed flag-compare graph"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed flag-compare graph"
    );
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_and_gate_fail_closed_for_graph_hint_ssa_and_provenance_mutations() {
    let case = FlagCompareMemoryCase {
        format: Format::F64,
        signaling: true,
        form: EncodingForm::C4W1,
        source1: 15,
        base: 11,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert_exact_sequence(&base, case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let source2 = match base.blocks[0].ops[1].kind {
        OpKind::VBroadcast { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut malformed = Vec::new();

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("load hint", load_hint));

    let mut load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B4;
    }
    malformed.push(("load width", load_width));

    let mut load_sign = base.clone();
    if let OpKind::Load { sign, .. } = &mut load_sign.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("load sign", load_sign));

    let mut broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut broadcast_scalar.blocks[0].ops[1].kind {
        *scalar = source2;
    }
    malformed.push(("broadcast scalar", broadcast_scalar));

    let mut broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut broadcast_element.blocks[0].ops[1].kind {
        *elem = VecElementType::F32;
    }
    malformed.push(("broadcast element", broadcast_element));

    let mut broadcast_lanes = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut broadcast_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }
    malformed.push(("broadcast lanes", broadcast_lanes));

    let mut broadcast_hint = base.clone();
    broadcast_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("broadcast hint", broadcast_hint));

    let mut compare_source1 = base.clone();
    if let OpKind::X86FpCompare { src1, .. } = &mut compare_source1.blocks[0].ops[2].kind {
        *src1 = x86(X86Reg::Xmm(14));
    }
    malformed.push(("compare source1", compare_source1));

    let mut compare_source2 = base.clone();
    if let OpKind::X86FpCompare { src2, .. } = &mut compare_source2.blocks[0].ops[2].kind {
        *src2 = loaded;
    }
    malformed.push(("compare source2", compare_source2));

    let mut compare_element = base.clone();
    if let OpKind::X86FpCompare { elem, .. } = &mut compare_element.blocks[0].ops[2].kind {
        *elem = VecElementType::F32;
    }
    malformed.push(("compare element", compare_element));

    let mut compare_kind = base.clone();
    if let OpKind::X86FpCompare { signaling, .. } = &mut compare_kind.blocks[0].ops[2].kind {
        *signaling = false;
    }
    malformed.push(("compare kind", compare_kind));

    let mut suppress = base.clone();
    if let OpKind::X86FpCompare {
        suppress_exceptions,
        ..
    } = &mut suppress.blocks[0].ops[2].kind
    {
        *suppress_exceptions = true;
    }
    malformed.push(("exception suppression", suppress));

    let mut compare_hint = base.clone();
    compare_hint.blocks[0].ops[2].x86_hint = None;
    malformed.push(("compare hint", compare_hint));

    let mut loaded_external_use = base.clone();
    loaded_external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF0),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFF0)),
            src: SrcOperand::Reg(loaded),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    ));
    malformed.push(("loaded external use", loaded_external_use));

    let mut source_external_use = base.clone();
    source_external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF1),
        PC + 1,
        OpKind::X86FpCompare {
            src1: x86(X86Reg::Xmm(15)),
            src2: source2,
            elem: VecElementType::F64,
            signaling: true,
            suppress_exceptions: false,
        },
    ));
    malformed.push(("source vector external use", source_external_use));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFF2),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFF2)),
            src: SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    ));
    malformed.push(("same-PC tail", same_pc_tail));

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();
    malformed.push(("missing instruction bytes", missing_bytes));

    for (name, byte_index, xor) in [
        ("encoded map", 1, 0x03),
        ("encoded prefix", 2, 0x01),
        ("encoded L", 2, 0x04),
        ("encoded opcode", 3, 0x01),
        ("encoded source1", 4, 0x08),
        ("encoded vvvv", 2, 0x08),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        replace_instruction_bytes(&mut function, &bytes);
        malformed.push((name, function));
    }

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn scalar(words: &[u64; 8], format: Format) -> u64 {
    words[0] & format.bit_mask()
}

fn set_scalar(words: &mut [u64; 8], format: Format, value: u64) {
    words[0] = (words[0] & !format.bit_mask()) | (value & format.bit_mask());
}

fn is_nan(bits: u64, format: Format) -> bool {
    match format {
        Format::F32 => bits & 0x7F80_0000 == 0x7F80_0000 && bits & 0x007F_FFFF != 0,
        Format::F64 => {
            bits & 0x7FF0_0000_0000_0000 == 0x7FF0_0000_0000_0000
                && bits & 0x000F_FFFF_FFFF_FFFF != 0
        }
    }
}

fn is_snan(bits: u64, format: Format) -> bool {
    is_nan(bits, format)
        && match format {
            Format::F32 => bits & 0x0040_0000 == 0,
            Format::F64 => bits & 0x0008_0000_0000_0000 == 0,
        }
}

fn is_denormal(bits: u64, format: Format) -> bool {
    match format {
        Format::F32 => bits & 0x7F80_0000 == 0 && bits & 0x007F_FFFF != 0,
        Format::F64 => bits & 0x7FF0_0000_0000_0000 == 0 && bits & 0x000F_FFFF_FFFF_FFFF != 0,
    }
}

fn apply_daz(bits: u64, format: Format, mxcsr: u32) -> (u64, u32) {
    if !is_denormal(bits, format) {
        (bits, 0)
    } else if mxcsr & (1 << 6) != 0 {
        (bits & format.sign_mask(), 0)
    } else {
        (bits, 1 << 1)
    }
}

fn independent_compare(
    case: FlagCompareMemoryCase,
    first_raw: u64,
    second_raw: u64,
    mxcsr: u32,
) -> (u64, u32) {
    let (first, first_status) = apply_daz(first_raw & case.format.bit_mask(), case.format, mxcsr);
    let (second, second_status) =
        apply_daz(second_raw & case.format.bit_mask(), case.format, mxcsr);
    let first_nan = is_nan(first, case.format);
    let second_nan = is_nan(second, case.format);
    let invalid = is_snan(first, case.format)
        || is_snan(second, case.format)
        || (case.signaling && (first_nan || second_nan));
    let status = if first_nan || second_nan {
        u32::from(invalid)
    } else {
        first_status | second_status
    };
    let flags = if first_nan || second_nan {
        (1 << 6) | (1 << 2) | 1
    } else {
        let ordering = match case.format {
            Format::F32 => f32::from_bits(first as u32).partial_cmp(&f32::from_bits(second as u32)),
            Format::F64 => f64::from_bits(first).partial_cmp(&f64::from_bits(second)),
        }
        .expect("non-NaN operands have a total partial ordering");
        match ordering {
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Equal => 1 << 6,
            std::cmp::Ordering::Greater => 0,
        }
    };
    (flags, status)
}

fn values(format: Format) -> [(u64, u64); 10] {
    match format {
        Format::F32 => [
            (0x3F80_0000, 0x3F80_0000),
            (0x3F80_0000, 0x4000_0000),
            (0x4000_0000, 0x3F80_0000),
            (0x0000_0000, 0x8000_0000),
            (0x7FC0_0001, 0x3F80_0000),
            (0x7F80_0001, 0x3F80_0000),
            (0x0000_0001, 0x0000_0000),
            (0x8000_0001, 0x8000_0000),
            (0x7F80_0000, 0x7F80_0000),
            (0xBF80_0000, 0xBF80_0000),
        ],
        Format::F64 => [
            (0x3FF0_0000_0000_0000, 0x3FF0_0000_0000_0000),
            (0x3FF0_0000_0000_0000, 0x4000_0000_0000_0000),
            (0x4000_0000_0000_0000, 0x3FF0_0000_0000_0000),
            (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
            (0x7FF8_0000_0000_0001, 0x3FF0_0000_0000_0000),
            (0x7FF0_0000_0000_0001, 0x3FF0_0000_0000_0000),
            (0x0000_0000_0000_0001, 0x0000_0000_0000_0000),
            (0x8000_0000_0000_0001, 0x8000_0000_0000_0000),
            (0x7FF0_0000_0000_0000, 0x7FF0_0000_0000_0000),
            (0xBFF0_0000_0000_0000, 0xBFF0_0000_0000_0000),
        ],
    }
}

#[test]
fn independent_oracle_covers_truth_nan_daz_denormal_and_memory_footprints() {
    let mut checked = 0usize;
    for format in Format::ALL {
        for signaling in [false, true] {
            let case = FlagCompareMemoryCase {
                format,
                signaling,
                form: EncodingForm::C5,
                source1: 1,
                base: 3,
            };
            for (index, (first, second)) in values(format).into_iter().enumerate() {
                for daz in [false, true] {
                    let mxcsr = 0x1F80 | (u32::from(daz) << 6);
                    let (flags, status) = independent_compare(case, first, second, mxcsr);
                    assert_eq!(flags & !STATUS_FLAGS, 0, "{case:?} value {index}");
                    if index == 4 {
                        assert_eq!(status & 1 != 0, signaling, "{case:?}");
                    }
                    if index == 5 {
                        assert_eq!(status & 1, 1, "{case:?}");
                    }
                    if matches!(index, 6 | 7) {
                        assert_eq!(status & (1 << 1) != 0, !daz, "{case:?}");
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 2 * 2 * 10 * 2);

    for format in Format::ALL {
        let case = FlagCompareMemoryCase {
            format,
            signaling: true,
            form: EncodingForm::C5,
            source1: 1,
            base: 3,
        };
        let (first, second) = values(format)[2];
        let expected = independent_compare(case, first, second, 0x1F80);
        let mut source = [0u64; 8];
        set_scalar(&mut source, format, second);
        let mut changed_bytes = words_to_bytes(source);
        changed_bytes[format.memory_size() as usize..].fill(0xFF);
        let changed_above_footprint = bytes_to_words(changed_bytes);
        assert_eq!(
            expected,
            independent_compare(case, first, scalar(&source, format), 0x1F80)
        );
        assert_eq!(
            expected,
            independent_compare(
                case,
                first,
                scalar(&changed_above_footprint, format),
                0x1F80,
            )
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct FlagCompareMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn flag_compare_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut FlagCompareMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 8)
    {
        return 0;
    }
    let source = words_to_bytes(context.value);
    let mut scratch = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    scratch[..size as usize].copy_from_slice(&source[..size as usize]);
    state.vector_scratch = bytes_to_words(scratch);
    1
}

#[cfg(target_arch = "x86_64")]
fn patterned_vector(shift: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x0123_4567_89AB_CDEFu64.rotate_left(((word * 9 + shift) % 64) as u32)
            ^ (shift as u64).wrapping_mul(0x0101_0101_0101_0101)
    })
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: FlagCompareMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = patterned_vector(index * 5 + ordinal);
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: FlagCompareMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let first = scalar(&registers.zmm[usize::from(case.source1)], case.format);
    let second = scalar(&source2, case.format);
    let (flags, status) = independent_compare(case, first, second, registers.mxcsr);
    registers.rflags = (registers.rflags & !STATUS_FLAGS) | flags;
    registers.mxcsr |= status;
    let source = words_to_bytes(source2);
    let mut scratch = [0; 64];
    scratch[..case.format.memory_size() as usize]
        .copy_from_slice(&source[..case.format.memory_size() as usize]);
    registers.vector_scratch = bytes_to_words(scratch);
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: FlagCompareMemoryCase,
    level: OptLevel,
) {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let bytes = words_to_bytes(source2);
    memory.load(
        address as usize,
        &bytes[..case.format.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );
    context.flags.materialize_all();

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    for (index, value) in expected.zmm.iter().enumerate() {
        assert_eq!(
            &x86.xmm[index][..8],
            value,
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(x86.k, expected.k, "{level:?} {case:?}: masks");
    let actual_rflags =
        (initial.rflags & !STATUS_FLAGS) | (context.flags.materialized.to_rflags() & STATUS_FLAGS);
    assert_eq!(actual_rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_flag_compares_match_model_interpreter_and_precise_helper_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX FP flag-compare memory differential: host lacks AVX");
        return;
    }

    let cases = all_cases();
    assert_eq!(cases.len(), 192);
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut newly_observed_status = 0u32;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for (level_index, level) in DIFFERENTIAL_LEVELS.into_iter().enumerate() {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let variant =
                (ordinal + level_index * 7 + case.form.ordinal()) % values(case.format).len();
            let (first, second) = values(case.format)[variant];
            let mut source2 = patterned_vector(ordinal.wrapping_mul(7).wrapping_add(3));
            set_scalar(&mut source2, case.format, second);

            let mut context = FlagCompareMemoryContext {
                value: source2,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            set_scalar(
                &mut registers.zmm[usize::from(case.source1)],
                case.format,
                first,
            );
            let daz_ftz = if (ordinal / 10 + level_index) & 1 == 0 {
                0
            } else {
                (1 << 6) | (1 << 15)
            };
            registers.mxcsr = 0x1F80 | (1 << 5) | (((ordinal as u32) & 3) << 13) | daz_ftz;
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut FlagCompareMemoryContext) as u64;
            registers.vec_load_fn = flag_compare_load_helper as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, source2);
            newly_observed_status |= expected.mxcsr & !initial.mxcsr;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.format.memory_size(),
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function, &initial, &expected, source2, address, case, level,
            );
            successes += 1;

            let mut context = FlagCompareMemoryContext {
                value: source2,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            set_scalar(
                &mut registers.zmm[usize::from(case.source1)],
                case.format,
                first,
            );
            registers.mxcsr = initial.mxcsr;
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut FlagCompareMemoryContext) as u64;
            registers.vec_load_fn = flag_compare_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
            assert_eq!(context.last_addr, address, "fault {level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.format.memory_size(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }

    assert!(expected_executions > 0);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    assert_eq!(newly_observed_status & 0x03, 0x03);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX FP flag-compare memory cases"
    );
}
