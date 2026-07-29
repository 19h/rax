//! Exact helper-backed VEX VSHUFPS/VSHUFPD memory-source coverage.

use super::*;
#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};
use crate::smir::lower::runtime::{
    X86JitVexFpShuffleMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_fp_shuffle_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0xC6A5;
const DISP: i64 = 0x20;
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

    const fn immediates(self) -> [u8; 6] {
        match self {
            Self::F32 => [0x00, 0x1B, 0x4E, 0xA5, 0xE4, 0xFF],
            // High immediate bits are ignored for the shorter binary64 forms.
            Self::F64 => [0x00, 0x03, 0x0A, 0xA5, 0xF0, 0xFF],
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShuffleCase {
    format: Format,
    width: VecWidth,
    form: EncodingForm,
    immediate: u8,
}

impl ShuffleCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Destination/source1 occupy XMM/YMM0/1, forcing scratch 2.
            EncodingForm::C5 => (0, 1, 3),
            // A high destination plus source1 0 forces scratch 1.
            EncodingForm::C4W0 => (15, 0, 11),
            // Aliased high destination/source1 force scratch 0.
            EncodingForm::C4W1 => (9, 9, 11),
        }
    }

    const fn destination(self) -> u8 {
        self.operands().0
    }

    const fn source1(self) -> u8 {
        self.operands().1
    }

    const fn base(self) -> u8 {
        self.operands().2
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination() && *index != self.source1())
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source1, base) = self.operands();
        let l = u8::from(self.width == VecWidth::V256);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        let mut bytes = match self.form {
            EncodingForm::C5 => vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | (l << 2)
                    | self.format.pp(),
                0xC6,
                modrm,
                DISP as u8,
            ],
            EncodingForm::C4W0 | EncodingForm::C4W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(self.form.w()) << 7)
                    | (((!source1) & 0x0F) << 3)
                    | (l << 2)
                    | self.format.pp(),
                0xC6,
                modrm,
                DISP as u8,
            ],
        };
        bytes.push(self.immediate);
        bytes
    }

    fn emitted_shuffle_bytes(self) -> [u8; 5] {
        let destination = self.destination();
        let source1 = self.source1();
        [
            0xC5,
            (if destination < 8 { 0x80 } else { 0 })
                | (((!source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | self.format.pp(),
            0xC6,
            0xC0 | ((destination & 7) << 3) | self.scratch(),
            self.immediate,
        ]
    }
}

fn all_cases() -> Vec<ShuffleCase> {
    let mut cases = Vec::new();
    for format in Format::ALL {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in EncodingForm::ALL {
                for immediate in format.immediates() {
                    cases.push(ShuffleCase {
                        format,
                        width,
                        form,
                        immediate,
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

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX VSHUFPS/VSHUFPD have only 128-/256-bit forms"),
    })
}

fn expected_address(case: ShuffleCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
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
) -> Option<X86JitVexFpShuffleMemorySequence> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_vex_fp_shuffle_memory_sequence(
        block,
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: ShuffleCase) {
    let block = &function.blocks[0];
    let lanes = case.width.lanes(case.format.elem()) as usize;
    assert_eq!(block.ops.len(), 4 + lanes * 2, "{case:?}");
    let loaded = match &block.ops[0].kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected leading virtual VLoad, got {other:?}"),
    };
    assert_eq!(
        block.ops[0].x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        "{case:?}"
    );
    assert!(
        block.ops[1..]
            .iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none()),
        "{case:?}"
    );
    let OpKind::VShuffle {
        dst,
        src1,
        src2,
        elem,
        lanes: shuffled_lanes,
        ..
    } = block.ops.last().unwrap().kind
    else {
        panic!("{case:?}: expected final VShuffle")
    };
    assert_eq!(dst, vector(case.destination(), case.width), "{case:?}");
    assert_eq!(src1, vector(case.source1(), case.width), "{case:?}");
    assert_eq!(src2, Some(loaded), "{case:?}");
    assert_eq!(elem, case.format.elem(), "{case:?}");
    assert_eq!(usize::from(shuffled_lanes), lanes, "{case:?}");

    assert_eq!(
        classified_sequence(function, true),
        Some(X86JitVexFpShuffleMemorySequence {
            consumed: block.ops.len(),
            memory_size: case.width.bytes(),
            destination: case.destination(),
            source1: case.source1(),
            width: case.width,
            elem: case.format.elem(),
            immediate: case.immediate,
        }),
        "{case:?}"
    );
    assert_eq!(classified_sequence(function, false), None, "{case:?}");
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

fn lift_case(case: ShuffleCase) -> SmirFunction {
    let bytes = case.bytes();
    let function = lift_bytes(&bytes);
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction, case: ShuffleCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX shuffle lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX floating-point shuffle"),
        result.entry_offset,
    )
}

#[test]
fn all_216_form_format_width_immediate_and_optimization_cells_admit_and_lower() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 2 * 3 * 6);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_lift_and_sequence(&function, case);
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {case:?}: missing reserved vector index"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector-scratch displacement"
            );
            let expected = case.emitted_shuffle_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 216);
}

#[test]
fn llvm_23_canonical_memory_encodings_match_the_case_generator() {
    for (case, expected) in [
        (
            ShuffleCase {
                format: Format::F32,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                immediate: 0x1B,
            },
            &[0xC5, 0xF0, 0xC6, 0x43, 0x20, 0x1B][..],
        ),
        (
            ShuffleCase {
                format: Format::F32,
                width: VecWidth::V256,
                form: EncodingForm::C4W0,
                immediate: 0xE4,
            },
            &[0xC4, 0x41, 0x7C, 0xC6, 0x7B, 0x20, 0xE4][..],
        ),
        (
            ShuffleCase {
                format: Format::F64,
                width: VecWidth::V128,
                form: EncodingForm::C5,
                immediate: 0x03,
            },
            &[0xC5, 0xF1, 0xC6, 0x43, 0x20, 0x03][..],
        ),
        (
            ShuffleCase {
                format: Format::F64,
                width: VecWidth::V256,
                form: EncodingForm::C4W1,
                immediate: 0x0A,
            },
            &[0xC4, 0x41, 0xB5, 0xC6, 0x4B, 0x20, 0x0A][..],
        ),
    ] {
        assert_eq!(case.bytes(), expected, "{case:?}");
    }
}

#[test]
fn rip_relative_segment_sib_disp32_and_addr32_memory_shapes_admit_at_every_opt_level() {
    let encodings: &[&[u8]] = &[
        // vshufpd xmm1, xmm2, [rip + 0x44332211], 0x03
        &[0xC5, 0xE9, 0xC6, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x03],
        // vshufps ymm0, ymm1, fs:[rcx*4 + 0x44332211], 0x1B
        &[
            0x64, 0xC5, 0xF4, 0xC6, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x1B,
        ],
        // vshufpd ymm14, ymm10, fs:addr32 [esi*2 + 0x44332211], 0xA5
        &[
            0x64, 0x67, 0xC4, 0x61, 0xAD, 0xC6, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44, 0xA5,
        ],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let sequence = classified_sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: not classified"));
            assert_eq!(sequence.immediate, bytes[bytes.len() - 1]);
            assert!(is_native_clobber_safe_excluding(
                &function,
                &HashMap::new(),
                true
            ));

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_mem_helpers(true);
            lowerer.set_preserve_vector_mem_helpers(true);
            lowerer.set_avx_ymm16_vector_state(true);
            let result = lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            assert!(result.relocations.is_empty());
            lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * LEVELS.len());
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

#[test]
fn classifier_and_lowerer_fail_closed_for_source_graph_and_boundary_mutations() {
    let case = ShuffleCase {
        format: Format::F32,
        width: VecWidth::V256,
        form: EncodingForm::C4W1,
        immediate: 0xA5,
    };
    let base = lift_case(case);
    let loaded = loaded_virtual(&base);
    let final_index = base.blocks[0].ops.len() - 1;
    let mut malformed = Vec::new();

    let mut missing_metadata = base.clone();
    missing_metadata
        .x86_instruction_bytes
        .remove(&(BlockId(0), PC));
    malformed.push(("missing source bytes", missing_metadata));

    for (name, byte_index, xor) in [
        ("source destination", 4, 0x08),
        ("source first operand", 2, 0x08),
        ("source element format", 2, 0x01),
        ("source vector width", 2, 0x04),
        ("source immediate", 6, 0x01),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte_index] ^= xor;
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        malformed.push((name, function));
    }

    let mut missing_load_hint = base.clone();
    missing_load_hint.blocks[0].ops[0].x86_hint = None;
    malformed.push(("missing unaligned load provenance", missing_load_hint));

    let mut aligned_load = base.clone();
    aligned_load.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    malformed.push(("aligned load provenance", aligned_load));

    let mut wrong_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(("load width", wrong_width));

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }
    malformed.push(("virtual address component", virtual_address));

    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFF),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: loaded,
            width: VecWidth::V256,
        },
    ));
    malformed.push(("loaded value escapes sequence", external_use));

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[3].guest_pc += 1;
    malformed.push(("split guest PC", wrong_pc));

    let mut internal_hint = base.clone();
    internal_hint.blocks[0].ops[3].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(("invented internal hint", internal_hint));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7FFE), PC, OpKind::Nop));
    malformed.push(("unconsumed same-PC tail", same_pc_tail));

    let mut wrong_final_destination = base.clone();
    if let OpKind::VShuffle { dst, .. } =
        &mut wrong_final_destination.blocks[0].ops[final_index].kind
    {
        *dst = vector(8, VecWidth::V256);
    }
    malformed.push(("final destination", wrong_final_destination));

    let mut wrong_final_source1 = base.clone();
    if let OpKind::VShuffle { src1, .. } = &mut wrong_final_source1.blocks[0].ops[final_index].kind
    {
        *src1 = vector(8, VecWidth::V256);
    }
    malformed.push(("final first source", wrong_final_source1));

    let mut wrong_final_source2 = base.clone();
    if let OpKind::VShuffle { src2, .. } = &mut wrong_final_source2.blocks[0].ops[final_index].kind
    {
        *src2 = None;
    }
    malformed.push(("final second source", wrong_final_source2));

    let mut wrong_final_element = base.clone();
    if let OpKind::VShuffle { elem, .. } = &mut wrong_final_element.blocks[0].ops[final_index].kind
    {
        *elem = VecElementType::F64;
    }
    malformed.push(("final element", wrong_final_element));

    let mut wrong_final_lanes = base.clone();
    if let OpKind::VShuffle { lanes, .. } = &mut wrong_final_lanes.blocks[0].ops[final_index].kind {
        *lanes -= 1;
    }
    malformed.push(("final lane count", wrong_final_lanes));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

fn loaded_virtual(function: &SmirFunction) -> VReg {
    match function.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    }
}

#[test]
fn every_generated_selector_and_insert_invariant_is_fail_closed() {
    let case = ShuffleCase {
        format: Format::F32,
        width: VecWidth::V256,
        form: EncodingForm::C5,
        immediate: 0x4E,
    };
    let base = lift_case(case);
    let lanes = case.width.lanes(case.format.elem()) as usize;
    let loaded = loaded_virtual(&base);
    let (zero, indices) = match (&base.blocks[0].ops[1].kind, &base.blocks[0].ops[2].kind) {
        (
            OpKind::Mov { dst: zero, .. },
            OpKind::VBroadcast {
                dst: indices,
                scalar,
                ..
            },
        ) if scalar == zero => (*zero, *indices),
        _ => unreachable!("validated zero-vector index construction"),
    };

    let mut wrong_zero = base.clone();
    if let OpKind::Mov {
        src: crate::smir::ir::types::SrcOperand::Imm(value),
        ..
    } = &mut wrong_zero.blocks[0].ops[1].kind
    {
        *value = 1;
    }
    assert_rejected("nonzero index initializer", &wrong_zero);

    let mut wrong_zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[1].kind {
        *width = crate::smir::ir::types::OpWidth::W32;
    }
    assert_rejected("index initializer width", &wrong_zero_width);

    let mut wrong_broadcast_scalar = base.clone();
    if let OpKind::VBroadcast { scalar, .. } = &mut wrong_broadcast_scalar.blocks[0].ops[2].kind {
        *scalar = loaded;
    }
    assert_rejected("index broadcast scalar", &wrong_broadcast_scalar);

    let mut wrong_broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut wrong_broadcast_element.blocks[0].ops[2].kind {
        *elem = VecElementType::F64;
    }
    assert_rejected("index broadcast element", &wrong_broadcast_element);

    let mut wrong_broadcast = base.clone();
    if let OpKind::VBroadcast {
        lanes: broadcast_lanes,
        ..
    } = &mut wrong_broadcast.blocks[0].ops[2].kind
    {
        *broadcast_lanes -= 1;
    }
    assert_rejected("index broadcast lane count", &wrong_broadcast);

    for lane in 0..lanes {
        let mov_index = 3 + lane * 2;
        let insert_index = mov_index + 1;

        let mut wrong_selector = base.clone();
        if let OpKind::Mov {
            src: crate::smir::ir::types::SrcOperand::Imm(selector),
            ..
        } = &mut wrong_selector.blocks[0].ops[mov_index].kind
        {
            *selector ^= 1;
        }
        assert_rejected("lane selector immediate", &wrong_selector);

        let mut wrong_selector_width = base.clone();
        if let OpKind::Mov { width, .. } = &mut wrong_selector_width.blocks[0].ops[mov_index].kind {
            *width = crate::smir::ir::types::OpWidth::W32;
        }
        assert_rejected("lane selector width", &wrong_selector_width);

        let mut duplicate_selector = base.clone();
        if let OpKind::Mov { dst, .. } = &mut duplicate_selector.blocks[0].ops[mov_index].kind {
            *dst = zero;
        }
        assert_rejected("nonunique lane selector", &duplicate_selector);

        let mut wrong_insert_destination = base.clone();
        if let OpKind::VInsertLane { dst, .. } =
            &mut wrong_insert_destination.blocks[0].ops[insert_index].kind
        {
            *dst = loaded;
        }
        assert_rejected("insert destination vector", &wrong_insert_destination);

        let mut wrong_insert_vector = base.clone();
        if let OpKind::VInsertLane { vec, .. } =
            &mut wrong_insert_vector.blocks[0].ops[insert_index].kind
        {
            *vec = loaded;
        }
        assert_rejected("insert input vector", &wrong_insert_vector);

        let mut wrong_lane = base.clone();
        if let OpKind::VInsertLane {
            lane: inserted_lane,
            ..
        } = &mut wrong_lane.blocks[0].ops[insert_index].kind
        {
            *inserted_lane = inserted_lane.wrapping_add(1);
        }
        assert_rejected("insert destination lane", &wrong_lane);

        let mut wrong_insert_element = base.clone();
        if let OpKind::VInsertLane { elem, .. } =
            &mut wrong_insert_element.blocks[0].ops[insert_index].kind
        {
            *elem = VecElementType::F64;
        }
        assert_rejected("insert element type", &wrong_insert_element);

        let mut wrong_scalar = base.clone();
        if let OpKind::VInsertLane { scalar, .. } =
            &mut wrong_scalar.blocks[0].ops[insert_index].kind
        {
            *scalar = loaded_virtual(&base);
        }
        assert_rejected("insert selector register", &wrong_scalar);
    }

    let final_index = base.blocks[0].ops.len() - 1;
    let mut wrong_final_indices = base.clone();
    if let OpKind::VShuffle {
        indices: shuffled_indices,
        ..
    } = &mut wrong_final_indices.blocks[0].ops[final_index].kind
    {
        *shuffled_indices = loaded;
    }
    assert_rejected("final shuffle indices", &wrong_final_indices);

    let mut escaped_indices = base.clone();
    escaped_indices.blocks[0].ops.push(SmirOp::new(
        OpId(0x7FFD),
        PC + 1,
        OpKind::VMov {
            dst: vector(4, VecWidth::V256),
            src: indices,
            width: VecWidth::V256,
        },
    ));
    assert_rejected("index vector escapes sequence", &escaped_indices);
}

#[cfg(target_arch = "x86_64")]
fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn raw_lane(vector: &[u64; 8], format: Format, index: usize) -> u64 {
    match format {
        Format::F32 => (vector[index / 2] >> ((index % 2) * 32)) & 0xFFFF_FFFF,
        Format::F64 => vector[index],
    }
}

#[cfg(target_arch = "x86_64")]
fn set_raw_lane(vector: &mut [u64; 8], format: Format, index: usize, value: u64) {
    match format {
        Format::F32 => {
            let shift = (index % 2) * 32;
            let mask = 0xFFFF_FFFFu64 << shift;
            vector[index / 2] = (vector[index / 2] & !mask) | ((value << shift) & mask);
        }
        Format::F64 => vector[index] = value,
    }
}

/// Intel SDM Vol. 2B VSHUFPD/VSHUFPS operation equations expressed over raw
/// element bits. VEX clears every destination bit above VL.
#[cfg(target_arch = "x86_64")]
fn architectural_destination(case: ShuffleCase, source1: [u64; 8], source2: [u64; 8]) -> [u64; 8] {
    let mut destination = [0; 8];
    let lanes_per_128 = match case.format {
        Format::F32 => 4,
        Format::F64 => 2,
    };
    let chunks = (case.width.bytes() / 16) as usize;
    for chunk in 0..chunks {
        let base = chunk * lanes_per_128;
        match case.format {
            Format::F32 => {
                for output in 0..4 {
                    let selector = usize::from((case.immediate >> (output * 2)) & 3);
                    let source = if output < 2 { &source1 } else { &source2 };
                    set_raw_lane(
                        &mut destination,
                        case.format,
                        base + output,
                        raw_lane(source, case.format, base + selector),
                    );
                }
            }
            Format::F64 => {
                let source1_selector = usize::from((case.immediate >> (chunk * 2)) & 1);
                let source2_selector = usize::from((case.immediate >> (chunk * 2 + 1)) & 1);
                set_raw_lane(
                    &mut destination,
                    case.format,
                    base,
                    raw_lane(&source1, case.format, base + source1_selector),
                );
                set_raw_lane(
                    &mut destination,
                    case.format,
                    base + 1,
                    raw_lane(&source2, case.format, base + source2_selector),
                );
            }
        }
    }
    destination
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn operands(case: ShuffleCase, ordinal: usize) -> ([u64; 8], [u64; 8]) {
    let source1 = std::array::from_fn(|word| {
        0x7FF8_2468_ACE0_1357u64.rotate_left((word * 7 + ordinal) as u32)
            ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
    });
    let source2 = std::array::from_fn(|word| {
        0x8000_0000_0000_0001u64.rotate_right((word * 11 + ordinal) as u32)
            ^ (word as u64).wrapping_mul(0x1111_2222_3333_4444)
            ^ u64::from(case.immediate)
    });
    (source1, source2)
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: ShuffleCase, ordinal: usize) -> GuestRegs {
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
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    let (source1, _) = operands(case, ordinal);
    registers.zmm[usize::from(case.source1())] = source1;
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] =
            std::array::from_fn(|word| 0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7) as u32));
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreted_architecture(
    function: &SmirFunction,
    initial: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: ShuffleCase,
) -> ([u64; 32], [[u64; 8]; 32], [u64; 8], u64, u32) {
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
    memory.load(address as usize, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    let mut expected_vectors = initial.zmm;
    expected_vectors[usize::from(case.destination())] =
        architectural_destination(case, initial.zmm[usize::from(case.source1())], source2);
    assert_eq!(x86.gpr, initial.gpr, "{case:?}: interpreter GPR state");
    assert_eq!(
        vectors, expected_vectors,
        "{case:?}: interpreter versus Intel raw-bit equations"
    );
    assert_eq!(x86.k, initial.k, "{case:?}: interpreter opmask state");
    assert_eq!(x86.rflags, initial.rflags, "{case:?}: interpreter RFLAGS");
    assert_eq!(x86.mxcsr, initial.mxcsr, "{case:?}: interpreter MXCSR");
    (x86.gpr, vectors, x86.k, x86.rflags, x86.mxcsr)
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_vshufps_vshufpd_memory_matches_o0_o2_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX memory-shuffle differential: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let (_, source2) = operands(case, ordinal);

            let mut context = VectorMemoryContext {
                value: source2,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let (gpr, zmm, k, rflags, mxcsr) =
                interpreted_architecture(&function, &initial, source2, address, case);
            let mut expected = initial;
            expected.gpr = gpr;
            expected.zmm = zmm;
            expected.k = k;
            expected.rflags = rflags;
            expected.mxcsr = mxcsr;
            let words = (case.width.bytes() / 8) as usize;
            expected.vector_scratch =
                std::array::from_fn(|word| if word < words { source2[word] } else { 0 });

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
            assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            successes += 1;

            let mut context = VectorMemoryContext {
                value: source2,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
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
                case.width.bytes(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }

    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
