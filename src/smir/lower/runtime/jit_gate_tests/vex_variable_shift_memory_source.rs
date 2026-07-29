//! Exact helper-backed AVX2 per-element variable-shift memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, ShiftOp, VReg, VecElementType, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_binary_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xC700;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftKind {
    opcode: u8,
    elem: VecElementType,
    shift: ShiftOp,
}

const KINDS: [ShiftKind; 5] = [
    ShiftKind {
        opcode: 0x45,
        elem: VecElementType::I32,
        shift: ShiftOp::Lsr,
    },
    ShiftKind {
        opcode: 0x45,
        elem: VecElementType::I64,
        shift: ShiftOp::Lsr,
    },
    ShiftKind {
        opcode: 0x46,
        elem: VecElementType::I32,
        shift: ShiftOp::Asr,
    },
    ShiftKind {
        opcode: 0x47,
        elem: VecElementType::I32,
        shift: ShiftOp::Lsl,
    },
    ShiftKind {
        opcode: 0x47,
        elem: VecElementType::I64,
        shift: ShiftOp::Lsl,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OperandShape {
    destination: u8,
    source: u8,
    base: u8,
}

const OPERAND_SHAPES: [OperandShape; 4] = [
    OperandShape {
        destination: 0,
        source: 0,
        base: 3,
    },
    OperandShape {
        destination: 2,
        source: 9,
        base: 3,
    },
    OperandShape {
        destination: 9,
        source: 2,
        base: 11,
    },
    OperandShape {
        destination: 15,
        source: 15,
        base: 11,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VariableShiftMemoryCase {
    kind: ShiftKind,
    width: VecWidth,
    operands: OperandShape,
}

impl VariableShiftMemoryCase {
    const fn w(self) -> bool {
        matches!(self.kind.elem, VecElementType::I64)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.operands.destination && *index != self.operands.source)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let l = u8::from(self.width == VecWidth::V256);
        vec![
            0xC4,
            (if self.operands.destination < 8 {
                0x80
            } else {
                0
            }) | 0x40
                | (if self.operands.base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w()) << 7) | (((!self.operands.source) & 0x0F) << 3) | (l << 2) | 1,
            self.kind.opcode,
            0x40 | ((self.operands.destination & 7) << 3) | (self.operands.base & 7),
            DISP as u8,
        ]
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let scratch = self.scratch();
        let l = u8::from(self.width == VecWidth::V256);
        vec![
            0xC4,
            (if self.operands.destination < 8 {
                0x80
            } else {
                0
            }) | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(self.w()) << 7) | (((!self.operands.source) & 0x0F) << 3) | (l << 2) | 1,
            self.kind.opcode,
            0xC0 | ((self.operands.destination & 7) << 3) | (scratch & 7),
        ]
    }
}

fn all_cases() -> Vec<VariableShiftMemoryCase> {
    let mut cases = Vec::new();
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256] {
            for operands in OPERAND_SHAPES {
                cases.push(VariableShiftMemoryCase {
                    kind,
                    width,
                    operands,
                });
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
        _ => unreachable!("VEX variable shifts have only 128-/256-bit operands"),
    })
}

fn expected_address(case: VariableShiftMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.operands.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: VariableShiftMemoryCase) {
    let [load, consumer] = ops else {
        panic!("expected VLoad + X86PackedShiftVariable for {case:?}, got {ops:?}")
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    let loaded = match &load.kind {
        OpKind::VLoad {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };

    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(consumer.x86_hint, None, "{case:?}");
    let OpKind::X86PackedShiftVariable {
        dst,
        src,
        count,
        mask,
        width,
        elem,
        shift,
        zeroing,
    } = &consumer.kind
    else {
        panic!("{case:?}: expected X86PackedShiftVariable, got {consumer:?}")
    };
    assert_eq!(
        *dst,
        vector(case.operands.destination, case.width),
        "{case:?}"
    );
    assert_eq!(*src, vector(case.operands.source, case.width), "{case:?}");
    assert_eq!(*count, loaded, "{case:?}");
    assert_eq!(*mask, None, "{case:?}");
    assert_eq!(*width, case.width, "{case:?}");
    assert_eq!(*elem, case.kind.elem, "{case:?}");
    assert_eq!(*shift, case.kind.shift, "{case:?}");
    assert!(!zeroing, "{case:?}");
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

fn lift_case(case: VariableShiftMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let function = lift_bytes(&bytes);
    assert_exact_chain(&function.blocks[0].ops, case);
    function
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
) -> Option<crate::smir::lower::runtime::X86JitVexBinaryMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_vex_binary_memory_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: VariableShiftMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx2);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_fma);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed variable-shift lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX variable shift"),
        result.entry_offset,
    )
}

#[test]
fn every_kind_width_operand_alias_and_optimizer_shape_is_lifted_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 40);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);

            let actual = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(actual.consumed, 2, "{level:?} {case:?}");
            assert_eq!(actual.memory_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(
                actual.destination, case.operands.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(actual.source1, case.operands.source, "{level:?} {case:?}");
            assert_eq!(actual.width, case.width, "{level:?} {case:?}");
            assert_eq!(actual.map, X86VecMap::Map0F38, "{level:?} {case:?}");
            assert_eq!(actual.prefix, X86SsePrefix::OpSize, "{level:?} {case:?}");
            assert_eq!(actual.opcode, case.kind.opcode, "{level:?} {case:?}");
            assert_eq!(actual.w, case.w(), "{level:?} {case:?}");
            assert!(actual.needs_avx2, "{level:?} {case:?}");
            assert!(!actual.needs_fma, "{level:?} {case:?}");
            assert!(
                sequence(&function, false).is_none(),
                "{level:?} {case:?}: memory-disabled classifier admitted sequence"
            );

            let (code, _) = lower(&function, case);
            assert!(
                code.windows(5).any(|window| {
                    window
                        == [
                            0xBA,
                            crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                            0,
                            0,
                            0,
                        ]
                }),
                "{level:?} {case:?}: missing reserved vector transfer index"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector-scratch displacement"
            );
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 40 * LEVELS.len());
}

#[test]
fn all_1_280_decoder_census_cells_are_admitted_and_lowered_after_o2() {
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256] {
            let l = u8::from(width == VecWidth::V256);
            for source in 0u8..16 {
                let p1 = (u8::from(kind.elem == VecElementType::I64) << 7)
                    | (((!source) & 0x0F) << 3)
                    | (l << 2)
                    | 1;
                for destination in 0u8..8 {
                    let bytes = [0xC4, 0xE2, p1, kind.opcode, 0x02 | (destination << 3)];
                    let function = optimize(lift_bytes(&bytes), OptLevel::O2);
                    let sequence = sequence(&function, true).unwrap_or_else(|| {
                        panic!("{bytes:02X?}: defined AVX2 variable shift was rejected")
                    });
                    assert_eq!(sequence.destination, destination, "{bytes:02X?}");
                    assert_eq!(sequence.source1, source, "{bytes:02X?}");
                    assert_eq!(sequence.width, width, "{bytes:02X?}");
                    assert_eq!(sequence.memory_size, width.bytes(), "{bytes:02X?}");
                    assert!(sequence.needs_avx2, "{bytes:02X?}");
                    admitted += 1;

                    let case = VariableShiftMemoryCase {
                        kind,
                        width,
                        operands: OperandShape {
                            destination,
                            source,
                            base: 2,
                        },
                    };
                    let (code, _) = lower(&function, case);
                    let expected = case.emitted_bytes();
                    assert!(
                        code.windows(expected.len())
                            .any(|window| window == expected),
                        "{bytes:02X?}: missing {expected:02X?}"
                    );
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(admitted, 1_280);
    assert_eq!(lowered, 1_280);
}

#[test]
fn sib_rip_relative_segment_absolute_and_addr32_forms_are_admitted_and_lowered() {
    // Independently assembled by LLVM 23 with +avx2.
    let encodings = [
        (&[0xC4, 0xE2, 0x69, 0x47, 0x04, 0x8B][..], 0, 2),
        (
            &[0xC4, 0x02, 0x69, 0x47, 0x8C, 0xD3, 0x78, 0x56, 0x34, 0x12][..],
            9,
            2,
        ),
        (&[0xC4, 0xE2, 0x69, 0x47, 0x05, 0x20, 0, 0, 0][..], 0, 2),
        (&[0x64, 0xC4, 0xE2, 0x69, 0x47, 0x43, 0x20][..], 0, 2),
        (&[0x67, 0xC4, 0xE2, 0x69, 0x47, 0x43, 0x20][..], 0, 2),
        (
            &[0xC4, 0xE2, 0x69, 0x47, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12][..],
            0,
            2,
        ),
        (&[0x65, 0xC4, 0x02, 0x69, 0x47, 0x4C, 0xD3, 0x20][..], 9, 2),
    ];

    let mut lowered = 0usize;
    for (bytes, destination, source) in encodings {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let [load, consumer] = function.blocks[0].ops.as_slice() else {
                panic!("{level:?} {bytes:02X?}: expected exact two-op chain")
            };
            let OpKind::VLoad { dst, addr, width } = &load.kind else {
                panic!("{level:?} {bytes:02X?}: expected VLoad, got {load:?}")
            };
            assert!(matches!(dst, VReg::Virtual(_)), "{level:?} {bytes:02X?}");
            assert!(
                addr.is_x86_state_backed_shape(),
                "{level:?} {bytes:02X?}: {addr:?}"
            );
            assert_eq!(*width, VecWidth::V128, "{level:?} {bytes:02X?}");
            assert_eq!(load.x86_hint, None, "{level:?} {bytes:02X?}");
            let OpKind::X86PackedShiftVariable {
                dst: consumer_dst,
                src: consumer_src,
                count,
                mask: None,
                width: VecWidth::V128,
                elem: VecElementType::I32,
                shift: ShiftOp::Lsl,
                zeroing: false,
            } = &consumer.kind
            else {
                panic!("{level:?} {bytes:02X?}: wrong consumer {consumer:?}")
            };
            assert_eq!(*consumer_dst, vector(destination, VecWidth::V128));
            assert_eq!(*consumer_src, vector(source, VecWidth::V128));
            assert_eq!(count, dst);
            assert_eq!(consumer.x86_hint, None, "{level:?} {bytes:02X?}");

            let admitted = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: sequence rejected"));
            assert_eq!(admitted.destination, destination);
            assert_eq!(admitted.source1, source);
            assert_eq!(admitted.memory_size, VecWidth::V128.bytes());
            assert!(admitted.needs_avx2);
            assert!(is_native_clobber_safe_excluding(
                &function,
                &HashMap::new(),
                true
            ));

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_mem_helpers(true);
            lowerer.set_preserve_vector_mem_helpers(true);
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer.set_guest_pcrel_lea_immediates(true);
            lowerer.set_jit_fault_deopt_guards(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * DIFFERENTIAL_LEVELS.len());
}

#[test]
fn register_rewrite_matches_independent_llvm_23_encodings() {
    let find = |opcode, elem| {
        KINDS
            .into_iter()
            .find(|kind| kind.opcode == opcode && kind.elem == elem)
            .unwrap()
    };
    let operands = OPERAND_SHAPES[2];
    for (case, expected) in [
        (
            VariableShiftMemoryCase {
                kind: find(0x47, VecElementType::I32),
                width: VecWidth::V128,
                operands,
            },
            &[0xC4, 0x62, 0x69, 0x47, 0xC8][..],
        ),
        (
            VariableShiftMemoryCase {
                kind: find(0x47, VecElementType::I64),
                width: VecWidth::V256,
                operands,
            },
            &[0xC4, 0x62, 0xED, 0x47, 0xC8][..],
        ),
        (
            VariableShiftMemoryCase {
                kind: find(0x46, VecElementType::I32),
                width: VecWidth::V256,
                operands,
            },
            &[0xC4, 0x62, 0x6D, 0x46, 0xC8][..],
        ),
        (
            VariableShiftMemoryCase {
                kind: find(0x45, VecElementType::I32),
                width: VecWidth::V128,
                operands,
            },
            &[0xC4, 0x62, 0x69, 0x45, 0xC8][..],
        ),
        (
            VariableShiftMemoryCase {
                kind: find(0x45, VecElementType::I64),
                width: VecWidth::V256,
                operands,
            },
            &[0xC4, 0x62, 0xED, 0x45, 0xC8][..],
        ),
    ] {
        assert_eq!(case.emitted_bytes(), expected, "{case:?}");
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact sequence classifier admitted malformed chain"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed chain"
    );
}

fn assert_mutation_rejected(
    base: &SmirFunction,
    name: &str,
    mutate: impl FnOnce(&mut SmirFunction),
) {
    let mut function = base.clone();
    mutate(&mut function);
    assert_rejected(name, &function);
}

fn replace_instruction_bytes(function: &mut SmirFunction, bytes: &[u8]) {
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("mutated test encoding fits metadata"),
    );
}

#[test]
fn classifier_and_gate_fail_closed_for_every_chain_and_provenance_invariant() {
    let case = VariableShiftMemoryCase {
        kind: KINDS[2],
        width: VecWidth::V256,
        operands: OPERAND_SHAPES[2],
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        ref other => panic!("expected load, got {other:?}"),
    };

    assert_mutation_rejected(&base, "loaded vector used twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(2),
            PC + 1,
            OpKind::VMov {
                dst: vector(4, case.width),
                src: loaded,
                width: case.width,
            },
        ));
    });
    assert_mutation_rejected(&base, "loaded vector defined twice", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(2),
            PC + 1,
            OpKind::VLoad {
                dst: loaded,
                addr: expected_address(case),
                width: case.width,
            },
        ));
    });
    assert_mutation_rejected(&base, "extra same-PC semantic operation", |function| {
        function.blocks[0]
            .ops
            .push(SmirOp::new(OpId(2), PC, OpKind::Nop));
    });
    assert_mutation_rejected(&base, "preceding same-PC semantic operation", |function| {
        function.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(u16::MAX), PC, OpKind::Nop));
    });
    assert_mutation_rejected(&base, "load has an encoding hint", |function| {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
            crate::smir::ir::ops::X86VecAlign::Unaligned,
        ));
    });
    assert_mutation_rejected(&base, "load writes architectural state", |function| {
        if let OpKind::VLoad { dst, .. } = &mut function.blocks[0].ops[0].kind {
            *dst = vector(3, case.width);
        }
    });
    assert_mutation_rejected(&base, "load width mismatch", |function| {
        if let OpKind::VLoad { width, .. } = &mut function.blocks[0].ops[0].kind {
            *width = VecWidth::V128;
        }
    });
    assert_mutation_rejected(&base, "virtual address component", |function| {
        if let OpKind::VLoad { addr, .. } = &mut function.blocks[0].ops[0].kind {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
    });
    assert_mutation_rejected(&base, "consumer has a different guest PC", |function| {
        function.blocks[0].ops[1].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "consumer has an encoding hint", |function| {
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: case.kind.opcode,
            width: case.width,
            w: case.w(),
        });
    });
    assert_mutation_rejected(&base, "consumer bypasses loaded count", |function| {
        if let OpKind::X86PackedShiftVariable { count, .. } = &mut function.blocks[0].ops[1].kind {
            *count = vector(3, case.width);
        }
    });
    assert_mutation_rejected(&base, "consumer has an opmask", |function| {
        if let OpKind::X86PackedShiftVariable { mask, .. } = &mut function.blocks[0].ops[1].kind {
            *mask = Some(x86(X86Reg::K(1)));
        }
    });
    assert_mutation_rejected(&base, "consumer requests zero masking", |function| {
        if let OpKind::X86PackedShiftVariable { zeroing, .. } = &mut function.blocks[0].ops[1].kind
        {
            *zeroing = true;
        }
    });
    assert_mutation_rejected(&base, "consumer width mismatch", |function| {
        if let OpKind::X86PackedShiftVariable { width, .. } = &mut function.blocks[0].ops[1].kind {
            *width = VecWidth::V128;
        }
    });
    assert_mutation_rejected(&base, "wrong element width", |function| {
        if let OpKind::X86PackedShiftVariable { elem, .. } = &mut function.blocks[0].ops[1].kind {
            *elem = VecElementType::I64;
        }
    });
    assert_mutation_rejected(&base, "wrong shift operation", |function| {
        if let OpKind::X86PackedShiftVariable { shift, .. } = &mut function.blocks[0].ops[1].kind {
            *shift = ShiftOp::Ror;
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only source", |function| {
        if let OpKind::X86PackedShiftVariable { src, .. } = &mut function.blocks[0].ops[1].kind {
            *src = vector(16, case.width);
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only destination", |function| {
        if let OpKind::X86PackedShiftVariable { dst, .. } = &mut function.blocks[0].ops[1].kind {
            *dst = vector(16, case.width);
        }
    });
    assert_mutation_rejected(
        &base,
        "destination register namespace mismatch",
        |function| {
            if let OpKind::X86PackedShiftVariable { dst, .. } = &mut function.blocks[0].ops[1].kind
            {
                *dst = x86(X86Reg::Xmm(case.operands.destination));
            }
        },
    );
    assert_mutation_rejected(&base, "missing instruction-byte provenance", |function| {
        function.x86_instruction_bytes.clear();
    });

    let mut bytes = case.bytes();
    bytes[4] = (bytes[4] & !0x38) | 0x30;
    assert_mutation_rejected(&base, "encoded destination mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] = (bytes[2] & !0x78) | (((!3u8) & 0x0F) << 3);
    assert_mutation_rejected(&base, "encoded source mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] ^= 0x04;
    assert_mutation_rejected(&base, "encoded width mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] |= 0x80;
    assert_mutation_rejected(&base, "encoded W mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[3] = 0x47;
    assert_mutation_rejected(&base, "encoded shift mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[1] = (bytes[1] & !0x1F) | 1;
    assert_mutation_rejected(&base, "encoded map mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] = (bytes[2] & !3) | 2;
    assert_mutation_rejected(&base, "encoded mandatory-prefix mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes.pop();
    assert_mutation_rejected(&base, "truncated displacement", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes.push(0);
    assert_mutation_rejected(&base, "trailing byte", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[4] |= 0xC0;
    assert_mutation_rejected(&base, "register-form provenance", |function| {
        replace_instruction_bytes(function, &bytes);
    });
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

fn read_lane(bytes: &[u8], elem: VecElementType, lane: usize) -> u64 {
    let size = elem.bytes() as usize;
    let mut raw = [0; 8];
    raw[..size].copy_from_slice(&bytes[lane * size..lane * size + size]);
    u64::from_le_bytes(raw)
}

fn write_lane(bytes: &mut [u8], elem: VecElementType, lane: usize, value: u64) {
    let size = elem.bytes() as usize;
    bytes[lane * size..lane * size + size].copy_from_slice(&value.to_le_bytes()[..size]);
}

fn lane_mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn shift_lane(value: u64, bits: u32, amount: u64, shift: ShiftOp) -> u64 {
    let mask = lane_mask(bits);
    let value = value & mask;
    if amount >= u64::from(bits) {
        return match shift {
            ShiftOp::Asr if value & (1u64 << (bits - 1)) != 0 => mask,
            ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr => 0,
            _ => unreachable!("AVX2 variable shifts use only LSL/LSR/ASR"),
        };
    }
    match shift {
        ShiftOp::Lsl => (value << amount) & mask,
        ShiftOp::Lsr => value >> amount,
        ShiftOp::Asr => {
            let signed = if bits == 64 {
                value as i64
            } else {
                ((value << (64 - bits)) as i64) >> (64 - bits)
            };
            ((signed >> amount) as u64) & mask
        }
        _ => unreachable!("AVX2 variable shifts use only LSL/LSR/ASR"),
    }
}

fn operand_vectors(case: VariableShiftMemoryCase, ordinal: usize) -> ([u64; 8], [u64; 8]) {
    let mut source = [0xC3; 64];
    let mut counts = [0x5A; 64];
    let bits = u32::from(case.kind.elem.bytes()) * 8;
    let mask = lane_mask(bits);
    let lanes = case.width.bytes() as usize / case.kind.elem.bytes() as usize;
    let boundary_counts = [
        0,
        1,
        u64::from(bits - 1),
        u64::from(bits),
        u64::from(bits + 1),
        u64::MAX,
    ];
    for lane in 0..lanes {
        let lane_u64 = lane as u64;
        let mut value = 0x0102_0408_1020_4081u64.rotate_left((lane_u64 * 7) as u32)
            ^ lane_u64.wrapping_mul(0x1111_2222_3333_4444)
            ^ ordinal as u64;
        value &= mask;
        if lane & 1 != 0 {
            value |= 1u64 << (bits - 1);
        } else {
            value &= !(1u64 << (bits - 1));
        }
        write_lane(&mut source, case.kind.elem, lane, value);
        write_lane(
            &mut counts,
            case.kind.elem,
            lane,
            boundary_counts[(lane + ordinal) % boundary_counts.len()],
        );
    }
    (bytes_to_words(source), bytes_to_words(counts))
}

fn model_result(case: VariableShiftMemoryCase, source: [u64; 8], counts: [u64; 8]) -> [u64; 8] {
    let source = words_to_bytes(source);
    let counts = words_to_bytes(counts);
    let mut result = [0; 64];
    let bits = u32::from(case.kind.elem.bytes()) * 8;
    let lanes = case.width.bytes() as usize / case.kind.elem.bytes() as usize;
    for lane in 0..lanes {
        write_lane(
            &mut result,
            case.kind.elem,
            lane,
            shift_lane(
                read_lane(&source, case.kind.elem, lane),
                bits,
                read_lane(&counts, case.kind.elem, lane),
                case.kind.shift,
            ),
        );
    }
    bytes_to_words(result)
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
fn full_guest_regs(case: VariableShiftMemoryCase, ordinal: usize) -> GuestRegs {
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
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
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
    let (source, _) = operand_vectors(case, ordinal);
    registers.zmm[usize::from(case.operands.source)] = source;
    if case.operands.destination != case.operands.source {
        registers.zmm[usize::from(case.operands.destination)] = std::array::from_fn(|word| {
            0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7 + ordinal) as u32)
        });
    }
    registers.gpr[usize::from(case.operands.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: VariableShiftMemoryCase,
    counts: [u64; 8],
) -> GuestRegs {
    let source = registers.zmm[usize::from(case.operands.source)];
    registers.zmm[usize::from(case.operands.destination)] = model_result(case, source, counts);
    let words = (case.width.bytes() / 8) as usize;
    registers.vector_scratch =
        std::array::from_fn(|word| if word < words { counts[word] } else { 0 });
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    counts: [u64; 8],
    address: u64,
    case: VariableShiftMemoryCase,
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
    let bytes = words_to_bytes(counts);
    memory.load(address as usize, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

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
    assert_eq!(x86.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_variable_shifts_match_sdm_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping native AVX2 variable-shift memory differential: host lacks AVX2");
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
            let (_, counts) = operand_vectors(case, ordinal);

            let mut context = VectorMemoryContext {
                value: counts,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.operands.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, counts);

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
            assert_interpreter_matches(
                &function, &initial, &expected, counts, address, case, level,
            );
            successes += 1;

            let mut context = VectorMemoryContext {
                value: counts,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.operands.base)].wrapping_add(DISP as u64);
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

    assert!(expected_executions > 0);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native AVX2 variable-shift memory cases"
    );
}
