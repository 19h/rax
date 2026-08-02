//! Exact helper-backed EVEX packed shared-count shift memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, ShiftOp, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexSharedCountShiftMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_shared_count_shift_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod semantics;

#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0xCE00;
const DISP8: i32 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftKind {
    opcode: u8,
    w: bool,
    elem: VecElementType,
    shift: ShiftOp,
}

impl ShiftKind {
    // Twelve encoding kinds cover both W values for each WIG word mnemonic
    // plus the six fixed/semantic-W doubleword and quadword forms.
    const ALL: [Self; 12] = [
        Self::new(0xD1, false, VecElementType::I16, ShiftOp::Lsr),
        Self::new(0xD1, true, VecElementType::I16, ShiftOp::Lsr),
        Self::new(0xD2, false, VecElementType::I32, ShiftOp::Lsr),
        Self::new(0xD3, true, VecElementType::I64, ShiftOp::Lsr),
        Self::new(0xE1, false, VecElementType::I16, ShiftOp::Asr),
        Self::new(0xE1, true, VecElementType::I16, ShiftOp::Asr),
        Self::new(0xE2, false, VecElementType::I32, ShiftOp::Asr),
        Self::new(0xE2, true, VecElementType::I64, ShiftOp::Asr),
        Self::new(0xF1, false, VecElementType::I16, ShiftOp::Lsl),
        Self::new(0xF1, true, VecElementType::I16, ShiftOp::Lsl),
        Self::new(0xF2, false, VecElementType::I32, ShiftOp::Lsl),
        Self::new(0xF3, true, VecElementType::I64, ShiftOp::Lsl),
    ];

    const fn new(opcode: u8, w: bool, elem: VecElementType, shift: ShiftOp) -> Self {
        Self {
            opcode,
            w,
            elem,
            shift,
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

    const fn mask(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Merge | Self::Zero => 3,
        }
    }

    const fn zeroing(self) -> bool {
        matches!(self, Self::Zero)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftMemoryCase {
    kind: ShiftKind,
    width: VecWidth,
    destination: u8,
    source: u8,
    control: MaskControl,
}

impl ShiftMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn mask(self) -> u8 {
        self.control.mask()
    }

    const fn zeroing(self) -> bool {
        self.control.zeroing()
    }

    const fn compressed_displacement(self) -> i32 {
        DISP8 * 16
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding(
            self.kind,
            self.width,
            self.destination,
            self.source,
            self.mask(),
            self.zeroing(),
            false,
            false,
        )
    }

    fn scratch(self) -> u8 {
        scratch_index(self.destination, self.source)
    }

    fn expected_replay(self) -> Vec<u8> {
        register_encoding(
            self.kind,
            self.width,
            self.destination,
            self.source,
            self.mask(),
            self.zeroing(),
            self.scratch(),
        )
    }
}

const fn vector_ll(width: VecWidth) -> u8 {
    match width {
        VecWidth::V128 => 0,
        VecWidth::V256 => 1,
        VecWidth::V512 => 2,
        _ => unreachable!(),
    }
}

fn scratch_index(destination: u8, source: u8) -> u8 {
    (0..16)
        .find(|candidate| *candidate != destination && *candidate != source)
        .expect("two operands leave at least fourteen low scratch registers")
}

#[allow(clippy::too_many_arguments)]
fn memory_encoding(
    kind: ShiftKind,
    width: VecWidth,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    apx_base: bool,
    apx_index: bool,
) -> Vec<u8> {
    assert!(destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0x61;
    if destination & 8 == 0 {
        p0 |= 0x80;
    }
    if destination & 16 == 0 {
        p0 |= 0x10;
    }
    if apx_base {
        p0 |= 0x08;
    }
    let mut p1 = (u8::from(kind.w) << 7) | (((!source) & 0x0F) << 3) | 0x05;
    if apx_index {
        p1 &= !0x04;
    }
    let p2 = (u8::from(zeroing) << 7)
        | (vector_ll(width) << 5)
        | (if source & 16 == 0 { 0x08 } else { 0 })
        | mask;
    vec![
        0x62,
        p0,
        p1,
        p2,
        kind.opcode,
        0x40 | ((destination & 7) << 3) | 0x04,
        0x48, // [RAX + RCX*2 + disp8]
        DISP8 as u8,
    ]
}

#[allow(clippy::too_many_arguments)]
fn register_encoding(
    kind: ShiftKind,
    width: VecWidth,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    scratch: u8,
) -> Vec<u8> {
    let mut p0 = 0x41;
    if scratch & 8 == 0 {
        p0 |= 0x20;
    }
    if destination & 8 == 0 {
        p0 |= 0x80;
    }
    if destination & 16 == 0 {
        p0 |= 0x10;
    }
    let p1 = (u8::from(kind.w) << 7) | (((!source) & 0x0F) << 3) | 0x05;
    let p2 = (u8::from(zeroing) << 7)
        | (vector_ll(width) << 5)
        | (if source & 16 == 0 { 0x08 } else { 0 })
        | mask;
    vec![
        0x62,
        p0,
        p1,
        p2,
        kind.opcode,
        0xC0 | ((destination & 7) << 3) | (scratch & 7),
    ]
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
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
        X86InstructionBytes::new(bytes).expect("EVEX shared-count shift fits metadata"),
    );
    function
}

fn lift_case(case: ShiftMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
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
) -> Option<X86JitEvexSharedCountShiftMemorySequence> {
    sequence_at(function, sequence_index(function), allow_mem)
}

fn sequence_at(
    function: &SmirFunction,
    index: usize,
    allow_mem: bool,
) -> Option<X86JitEvexSharedCountShiftMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_shared_count_shift_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: ShiftMemoryCase) -> (Vec<u8>, usize) {
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
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
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
        .unwrap_or_else(|error| panic!("{case:?}: shared-count shift memory lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed shared-count shift"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<ShiftMemoryCase> {
    let mut cases = Vec::new();
    for kind in ShiftKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (destination, source) in [(0, 0), (9, 10), (17, 18)] {
                for control in MaskControl::ALL {
                    cases.push(ShiftMemoryCase {
                        kind,
                        width,
                        destination,
                        source,
                        control,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn llvm_23_replay_byte_anchors_cover_all_nine_shared_count_shift_mnemonics() {
    // Independent encodings from LLVM 23.0.0git:
    // llvm-mc -triple=x86_64 -x86-asm-syntax=intel -show-encoding
    let anchors: [(ShiftMemoryCase, &[u8]); 9] = [
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[8],
                width: VecWidth::V128,
                destination: 17,
                source: 18,
                control: MaskControl::None,
            },
            &[0x62, 0xE1, 0x6D, 0x00, 0xF1, 0xC8],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[10],
                width: VecWidth::V256,
                destination: 17,
                source: 18,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE1, 0x6D, 0x23, 0xF2, 0xC8],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[11],
                width: VecWidth::V512,
                destination: 20,
                source: 21,
                control: MaskControl::Zero,
            },
            &[0x62, 0xE1, 0xD5, 0xC3, 0xF3, 0xE0],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[4],
                width: VecWidth::V512,
                destination: 31,
                source: 30,
                control: MaskControl::Zero,
            },
            &[0x62, 0x61, 0x0D, 0xC3, 0xE1, 0xF8],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[6],
                width: VecWidth::V128,
                destination: 23,
                source: 22,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE1, 0x4D, 0x03, 0xE2, 0xF8],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[7],
                width: VecWidth::V256,
                destination: 17,
                source: 18,
                control: MaskControl::None,
            },
            &[0x62, 0xE1, 0xED, 0x20, 0xE2, 0xC8],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[0],
                width: VecWidth::V512,
                destination: 20,
                source: 21,
                control: MaskControl::None,
            },
            &[0x62, 0xE1, 0x55, 0x40, 0xD1, 0xE0],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[2],
                width: VecWidth::V128,
                destination: 9,
                source: 10,
                control: MaskControl::Zero,
            },
            &[0x62, 0x71, 0x2D, 0x8B, 0xD2, 0xC8],
        ),
        (
            ShiftMemoryCase {
                kind: ShiftKind::ALL[3],
                width: VecWidth::V256,
                destination: 17,
                source: 18,
                control: MaskControl::Merge,
            },
            &[0x62, 0xE1, 0xED, 0x23, 0xD3, 0xC8],
        ),
    ];

    for (case, expected) in anchors {
        let encoding = X86InstructionBytes::new(&case.bytes())
            .unwrap()
            .evex_shared_count_shift_memory_encoding()
            .unwrap_or_else(|| panic!("{case:?}"));
        assert_eq!(
            encoding.register_instruction.as_slice(),
            expected,
            "{case:?}"
        );
    }
}

#[test]
fn shared_count_classifier_exhausts_2_211_840_operand_control_and_apx_cells() {
    let mut accepted = 0usize;
    for kind in ShiftKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for destination in 0..32u8 {
                for source in 0..32u8 {
                    for mask in 0..8u8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            for apx_base in [false, true] {
                                for apx_index in [false, true] {
                                    let bytes = memory_encoding(
                                        kind,
                                        width,
                                        destination,
                                        source,
                                        mask,
                                        zeroing,
                                        apx_base,
                                        apx_index,
                                    );
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_shared_count_shift_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.width, width, "{bytes:02X?}");
                                    assert_eq!(encoding.elem, kind.elem, "{bytes:02X?}");
                                    assert_eq!(encoding.shift, kind.shift, "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.source, source, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.writemask,
                                        (mask != 0).then_some(mask),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(encoding.zeroing, zeroing, "{bytes:02X?}");
                                    assert_eq!(encoding.w, kind.w, "{bytes:02X?}");
                                    assert_ne!(encoding.scratch, destination, "{bytes:02X?}");
                                    assert_ne!(encoding.scratch, source, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.needs_avx512vl,
                                        width != VecWidth::V512,
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.register_instruction.as_slice(),
                                        register_encoding(
                                            kind,
                                            width,
                                            destination,
                                            source,
                                            mask,
                                            zeroing,
                                            encoding.scratch,
                                        ),
                                        "{bytes:02X?}"
                                    );
                                    accepted += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 2_211_840);
}

#[test]
fn shared_count_classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[6],
        width: VecWidth::V128,
        destination: 1,
        source: 2,
        control: MaskControl::Merge,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !7) | 2;
    malformed.push(wrong_map);
    let mut wrong_pp = valid.clone();
    wrong_pp[2] = (wrong_pp[2] & !3) | 2;
    malformed.push(wrong_pp);
    let mut wrong_opcode = valid.clone();
    wrong_opcode[4] = 0xD4;
    malformed.push(wrong_opcode);
    let mut embedded_control = valid.clone();
    embedded_control[3] |= 0x10;
    malformed.push(embedded_control);
    let mut reserved_ll = valid.clone();
    reserved_ll[3] |= 0x60;
    malformed.push(reserved_ll);
    let mut zero_k0 = valid.clone();
    zero_k0[3] = (zero_k0[3] & !7) | 0x80;
    malformed.push(zero_k0);
    let mut wrong_dword_w = ShiftMemoryCase {
        kind: ShiftKind::ALL[2],
        ..case
    }
    .bytes();
    wrong_dword_w[2] |= 0x80;
    malformed.push(wrong_dword_w);
    let mut wrong_qword_w = ShiftMemoryCase {
        kind: ShiftKind::ALL[3],
        ..case
    }
    .bytes();
    wrong_qword_w[2] &= !0x80;
    malformed.push(wrong_qword_w);

    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_shared_count_shift_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_shared_count_shift_memory_encoding()
            .is_some()
    );
    for forbidden in [0x66, 0xF0, 0xF2, 0xF3, 0x40] {
        let mut bytes = vec![forbidden];
        bytes.extend_from_slice(&valid);
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_shared_count_shift_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_324_shared_count_memory_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 324);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(exact.encoding.elem, case.kind.elem, "{level:?} {case:?}");
            assert_eq!(exact.encoding.shift, case.kind.shift, "{level:?} {case:?}");
            assert_eq!(exact.encoding.w, case.kind.w, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.source, case.source, "{level:?} {case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{level:?} {case:?}");
            assert_eq!(exact.memory_size, 16, "{level:?} {case:?}");
            assert_eq!(
                exact.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");

            let (code, _) = lower(&function, case);
            let expected = case.expected_replay();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {} bytes",
                code.len()
            );
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 324 * LEVELS.len());
}

#[test]
fn fs_gs_address_size_and_rip_relative_sources_remain_helper_only() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[7],
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        control: MaskControl::Merge,
    };
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let prefix_cases = [
        (
            &[0x64][..],
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(rax),
                index: Some(rcx),
                scale: 2,
                disp: i64::from(case.compressed_displacement()),
            },
        ),
        (
            &[0x65][..],
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                base: Some(rax),
                index: Some(rcx),
                scale: 2,
                disp: i64::from(case.compressed_displacement()),
            },
        ),
        (
            &[0x67][..],
            Address::X86Addr32(Box::new(Address::BaseIndexScale {
                base: Some(rax),
                index: rcx,
                scale: 2,
                disp: case.compressed_displacement(),
                disp_size: DispSize::Disp8,
            })),
        ),
        (
            &[0x64, 0x67][..],
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(rax),
                index: Some(rcx),
                scale: 2,
                disp: i64::from(case.compressed_displacement()),
            })),
        ),
    ];
    for (prefixes, expected_address) in prefix_cases {
        let mut bytes = prefixes.to_vec();
        bytes.extend_from_slice(&case.bytes());
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(function.blocks[0].ops.iter().any(|op| matches!(
                &op.kind,
                OpKind::VLoad { addr, width: VecWidth::V128, .. } if addr == &expected_address
            )));
            assert!(
                sequence(&function, true).is_some(),
                "{level:?} {bytes:02X?}: {:#?}",
                function.blocks[0].ops
            );
            let (code, _) = lower(&function, case);
            let replay = case.expected_replay();
            assert!(
                code.windows(replay.len()).any(|window| window == replay),
                "{level:?} {bytes:02X?}: helper-only prefix leaked into replay"
            );
        }
    }

    let mut rip_bytes = case.bytes();
    rip_bytes.truncate(6);
    rip_bytes[5] = (rip_bytes[5] & 0x38) | 5;
    rip_bytes.extend_from_slice(&0x4433_2211i32.to_le_bytes());
    let expected_address = Address::PcRel {
        offset: 0x4433_2211,
        disp_size: DispSize::Disp32,
        base: Some(PC + rip_bytes.len() as u64),
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&rip_bytes), level);
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::VLoad { addr, width: VecWidth::V128, .. } if addr == &expected_address
        )));
        assert!(sequence(&function, true).is_some(), "{level:?}");
        lower(&function, case);
    }
}

#[test]
fn mem128_disp8_scaling_and_apx_b4_x4_guards_are_exact() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[11],
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        control: MaskControl::None,
    };
    for (apx_base, apx_index, expected_base, expected_index) in [
        (true, false, X86Reg::R16, X86Reg::Rcx),
        (false, true, X86Reg::Rax, X86Reg::R17),
        (true, true, X86Reg::R16, X86Reg::R17),
    ] {
        let bytes = memory_encoding(
            case.kind,
            case.width,
            case.destination,
            case.source,
            case.mask(),
            case.zeroing(),
            apx_base,
            apx_index,
        );
        let expected_address = Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::X86(expected_base))),
            index: VReg::Arch(ArchReg::X86(expected_index)),
            scale: 2,
            disp: case.compressed_displacement(),
            disp_size: DispSize::Disp8,
        };
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(matches!(
                function.blocks[0].ops.first().map(|op| &op.kind),
                Some(OpKind::X86RequireApx)
            ));
            assert!(function.blocks[0].ops.iter().any(|op| matches!(
                &op.kind,
                OpKind::VLoad { addr, width: VecWidth::V128, .. } if addr == &expected_address
            )));
            assert_eq!(sequence_index(&function), 1, "{level:?}");
            assert!(sequence(&function, true).is_some(), "{level:?}");
            lower(&function, case);

            let mut missing_guard = function.clone();
            assert!(matches!(
                missing_guard.blocks[0].ops.remove(0).kind,
                OpKind::X86RequireApx
            ));
            assert!(
                sequence_at(&missing_guard, 0, true).is_none(),
                "{level:?}: APX address admitted without its dynamic guard"
            );
        }
    }

    let mut fs_addr32 = vec![0x64, 0x67];
    fs_addr32.extend_from_slice(&memory_encoding(
        case.kind,
        case.width,
        case.destination,
        case.source,
        case.mask(),
        case.zeroing(),
        true,
        true,
    ));
    let expected_address = Address::X86Addr32(Box::new(Address::SegmentRel {
        segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
        base: Some(VReg::Arch(ArchReg::X86(X86Reg::R16))),
        index: Some(VReg::Arch(ArchReg::X86(X86Reg::R17))),
        scale: 2,
        disp: i64::from(case.compressed_displacement()),
    }));
    for level in LEVELS {
        let function = optimize(lift_bytes(&fs_addr32), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(function.blocks[0].ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::VLoad { addr, width: VecWidth::V128, .. } if addr == &expected_address
        )));
        assert!(sequence(&function, true).is_some(), "{level:?}");
        lower(&function, case);
    }
}

#[test]
fn avx_only_vector_state_bridge_rejects_shared_count_replay() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[11],
        width: VecWidth::V512,
        destination: 17,
        source: 18,
        control: MaskControl::Merge,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only state bridge must reject AVX-512 shared-count replay");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}

#[test]
fn exact_sequence_rejects_mutated_graph_provenance_and_frontiers() {
    let case = ShiftMemoryCase {
        kind: ShiftKind::ALL[6],
        width: VecWidth::V256,
        destination: 17,
        source: 18,
        control: MaskControl::Zero,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&function, true).is_some());

    let reject = |name: &str, mutated: &SmirFunction| {
        assert!(
            sequence(mutated, true).is_none(),
            "{name}: {:#?}",
            mutated.blocks[0].ops
        );
    };

    let mut wrong_shift = function.clone();
    if let Some(OpKind::X86PackedShift { shift, .. }) = wrong_shift.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::X86PackedShift { .. }))
        .map(|op| &mut op.kind)
    {
        *shift = ShiftOp::Lsl;
    }
    reject("shift", &wrong_shift);

    let mut wrong_source = function.clone();
    if let Some(OpKind::X86PackedShift { src, .. }) = wrong_source.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::X86PackedShift { .. }))
        .map(|op| &mut op.kind)
    {
        *src = VReg::Arch(ArchReg::X86(X86Reg::Ymm(19)));
    }
    reject("source", &wrong_source);

    let mut wrong_load_width = function.clone();
    if let Some(OpKind::VLoad { width, .. }) = wrong_load_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .map(|op| &mut op.kind)
    {
        *width = VecWidth::V256;
    }
    reject("load width", &wrong_load_width);

    let mut wrong_lane = function.clone();
    if let Some(OpKind::VExtractLane { lane, .. }) = wrong_lane.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .map(|op| &mut op.kind)
    {
        *lane = 1;
    }
    reject("count lane", &wrong_lane);

    let mut hinted = function.clone();
    hinted.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .unwrap()
        .x86_hint = None;
    reject("load hint", &hinted);

    let mut missing_provenance = function.clone();
    missing_provenance.x86_instruction_bytes.clear();
    reject("provenance", &missing_provenance);

    let mut wrong_provenance = function.clone();
    let other = ShiftMemoryCase {
        kind: ShiftKind::ALL[2],
        ..case
    };
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&other.bytes()).unwrap(),
    );
    reject("wrong provenance", &wrong_provenance);

    let mut tail = function.clone();
    tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xFFFF),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFF)),
            src: crate::smir::ir::types::SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    ));
    reject("same-PC tail", &tail);

    let prefix_op = SmirOp::new(
        OpId(0xFFFE),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFE)),
            src: crate::smir::ir::types::SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        },
    );
    let mut same_pc_prefix = function.clone();
    same_pc_prefix.blocks[0].ops.insert(0, prefix_op.clone());
    assert!(
        sequence_at(&same_pc_prefix, 1, true).is_none(),
        "same-PC prefix admitted a semantic suffix"
    );

    let mut spurious_apx_guard = function.clone();
    spurious_apx_guard.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0xFFFD), PC, OpKind::X86RequireApx));
    assert!(
        sequence_at(&spurious_apx_guard, 1, true).is_none(),
        "ordinary address admitted with a spurious dynamic APX guard"
    );

    let mut previous_instruction = function;
    let mut previous_op = prefix_op;
    previous_op.guest_pc = PC - 1;
    previous_instruction.blocks[0].ops.insert(0, previous_op);
    assert!(
        sequence_at(&previous_instruction, 1, true).is_some(),
        "a preceding instruction must not block an exact frontier"
    );
}
