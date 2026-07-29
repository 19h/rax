//! Exact helper-backed base AVX-VNNI memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{Address, DispSize, OpId, VirtualId};
use crate::smir::ir::{SmirBlock, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_vex_binary_memory_sequence, x86_jit_vex_integer_dot_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;

const DISP: i64 = 0x20;
const MODEL_MEMORY_VECTOR: u8 = 31;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OperandShape {
    destination: u8,
    source1: u8,
    base: u8,
}

const OPERAND_SHAPES: [OperandShape; 6] = [
    OperandShape {
        destination: 0,
        source1: 0,
        base: 3,
    },
    OperandShape {
        destination: 1,
        source1: 2,
        base: 3,
    },
    OperandShape {
        destination: 2,
        source1: 1,
        base: 11,
    },
    OperandShape {
        destination: 9,
        source1: 10,
        base: 3,
    },
    OperandShape {
        destination: 9,
        source1: 9,
        base: 11,
    },
    OperandShape {
        destination: 15,
        source1: 15,
        base: 11,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryCase {
    kind: DotKind,
    saturate: bool,
    width: VecWidth,
    operands: OperandShape,
}

impl MemoryCase {
    fn opcode(self) -> u8 {
        self.kind.opcode(self.saturate)
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.operands.destination && *index != self.operands.source1)
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        vec![
            0xC4,
            (if self.operands.destination < 8 {
                0x80
            } else {
                0
            }) | 0x40
                | (if self.operands.base < 8 { 0x20 } else { 0 })
                | 2,
            (((!self.operands.source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            self.opcode(),
            0x40 | ((self.operands.destination & 7) << 3) | (self.operands.base & 7),
            DISP as u8,
        ]
    }

    fn emitted_bytes(self) -> [u8; 5] {
        let scratch = self.scratch();
        [
            0xC4,
            (if self.operands.destination < 8 {
                0x80
            } else {
                0
            }) | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 2,
            (((!self.operands.source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            self.opcode(),
            0xC0 | ((self.operands.destination & 7) << 3) | (scratch & 7),
        ]
    }

    fn model_case(self) -> DotCase {
        DotCase {
            kind: self.kind,
            saturate: self.saturate,
            width: self.width,
            destination: self.operands.destination,
            source1: self.operands.source1,
            source2: MODEL_MEMORY_VECTOR,
            clear_ignored_x: false,
        }
    }
}

fn all_cases() -> Vec<MemoryCase> {
    let mut cases = Vec::new();
    for kind in DotKind::ALL {
        for saturate in [false, true] {
            for width in [VecWidth::V128, VecWidth::V256] {
                for operands in OPERAND_SHAPES {
                    cases.push(MemoryCase {
                        kind,
                        saturate,
                        width,
                        operands,
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

fn expected_address(case: MemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.operands.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: MemoryCase) {
    let [load, consumer] = ops else {
        panic!("expected VLoad + VDotProduct for {case:?}, got {ops:?}")
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
    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask: None,
        src_elem,
        acc_elem: VecElementType::I32,
        width,
        src1_unsigned,
        saturate,
        zeroing: false,
    } = &consumer.kind
    else {
        panic!("{case:?}: expected VDotProduct, got {consumer:?}")
    };
    assert_eq!(*dst, vector(case.operands.destination, case.width));
    assert_eq!(*acc, *dst, "{case:?}");
    assert_eq!(*src1, vector(case.operands.source1, case.width));
    assert_eq!(*src2, loaded, "{case:?}");
    assert_eq!(*src_elem, case.kind.elem(), "{case:?}");
    assert_eq!(*width, case.width, "{case:?}");
    assert_eq!(*src1_unsigned, case.kind.src1_unsigned(), "{case:?}");
    assert_eq!(*saturate, case.saturate, "{case:?}");
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
        X86InstructionBytes::new(bytes).expect("VEX instruction fits provenance"),
    );
    function
}

fn lift_case(case: MemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
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
) -> Option<crate::smir::lower::runtime::X86JitVexIntegerDotMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_vex_integer_dot_memory_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn generic_sequence(
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

fn lower(function: &SmirFunction, case: MemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx_vnni, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_avx_vnni_int8, "{case:?}");
    assert!(!requirements.needs_avx_vnni_int16, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx") && x86_host_has_avx_vnni(),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed base AVX-VNNI lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed base AVX-VNNI"),
        result.entry_offset,
    )
}

#[test]
fn every_kind_width_operand_alias_and_optimizer_shape_is_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 2 * 2 * 6);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);
            let actual = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            let binary = actual.binary;
            assert_eq!(binary.consumed, 2, "{level:?} {case:?}");
            assert_eq!(binary.memory_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(
                binary.destination, case.operands.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(binary.source1, case.operands.source1, "{level:?} {case:?}");
            assert_eq!(binary.width, case.width, "{level:?} {case:?}");
            assert_eq!(binary.map, X86VecMap::Map0F38, "{level:?} {case:?}");
            assert_eq!(binary.prefix, X86SsePrefix::OpSize, "{level:?} {case:?}");
            assert_eq!(binary.opcode, case.opcode(), "{level:?} {case:?}");
            assert!(!binary.w, "{level:?} {case:?}");
            assert!(!binary.needs_avx2, "{level:?} {case:?}");
            assert!(!binary.needs_fma, "{level:?} {case:?}");
            assert_eq!(
                generic_sequence(&function, true),
                Some(binary),
                "{level:?} {case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");
            assert!(
                generic_sequence(&function, false).is_none(),
                "{level:?} {case:?}"
            );

            let (code, _) = lower(&function, case);
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 2 * 2 * 2 * 6 * LEVELS.len());
}

#[test]
fn all_1_024_decoder_census_cells_are_admitted_and_lowered_after_o2() {
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for kind in DotKind::ALL {
        for saturate in [false, true] {
            let opcode = kind.opcode(saturate);
            for width in [VecWidth::V128, VecWidth::V256] {
                let l = u8::from(width == VecWidth::V256);
                for source1 in 0u8..16 {
                    let p1 = (((!source1) & 0x0F) << 3) | (l << 2) | 1;
                    for destination in 0u8..8 {
                        let bytes = [0xC4, 0xE2, p1, opcode, 0x02 | (destination << 3)];
                        let function = optimize(lift_bytes(&bytes), OptLevel::O2);
                        let actual = sequence(&function, true).unwrap_or_else(|| {
                            panic!("{bytes:02X?}: defined base AVX-VNNI form was rejected")
                        });
                        assert_eq!(actual.binary.destination, destination, "{bytes:02X?}");
                        assert_eq!(actual.binary.source1, source1, "{bytes:02X?}");
                        assert_eq!(actual.binary.width, width, "{bytes:02X?}");
                        admitted += 1;

                        let case = MemoryCase {
                            kind,
                            saturate,
                            width,
                            operands: OperandShape {
                                destination,
                                source1,
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
    }
    assert_eq!(admitted, 1_024);
    assert_eq!(lowered, 1_024);
}

#[test]
fn sib_rip_relative_segment_absolute_and_addr32_forms_are_admitted_and_lowered() {
    // Independently assembled by LLVM 23 with `{vex}` and `+avxvnni`.
    let encodings = [
        (&[0xC4, 0xE2, 0x69, 0x50, 0x0C, 0x8B][..], 1, 2),
        (
            &[0xC4, 0x42, 0x29, 0x50, 0x8C, 0xD3, 0x78, 0x56, 0x34, 0x12][..],
            9,
            10,
        ),
        (&[0xC4, 0xE2, 0x69, 0x50, 0x0D, 0x20, 0, 0, 0][..], 1, 2),
        (&[0x64, 0xC4, 0xE2, 0x69, 0x50, 0x4B, 0x20][..], 1, 2),
        (&[0x65, 0xC4, 0xC2, 0x69, 0x50, 0x4C, 0xD3, 0x20][..], 1, 2),
        (
            &[0xC4, 0xE2, 0x69, 0x50, 0x0C, 0x25, 0x78, 0x56, 0x34, 0x12][..],
            1,
            2,
        ),
        (&[0x67, 0xC4, 0xE2, 0x69, 0x50, 0x4C, 0x8B, 0x20][..], 1, 2),
    ];

    let mut lowered = 0usize;
    for (bytes, destination, source1) in encodings {
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
            let OpKind::VDotProduct {
                dst: consumer_dst,
                acc,
                src1: consumer_src1,
                src2,
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I32,
                width: VecWidth::V128,
                src1_unsigned: true,
                saturate: false,
                zeroing: false,
            } = &consumer.kind
            else {
                panic!("{level:?} {bytes:02X?}: wrong consumer {consumer:?}")
            };
            assert_eq!(*consumer_dst, vector(destination, VecWidth::V128));
            assert_eq!(*acc, *consumer_dst);
            assert_eq!(*consumer_src1, vector(source1, VecWidth::V128));
            assert_eq!(*src2, *dst);
            let admitted = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: sequence rejected"));
            assert_eq!(admitted.binary.destination, destination);
            assert_eq!(admitted.binary.source1, source1);

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
fn register_rewrite_matches_all_independent_llvm_23_encodings() {
    let low = OperandShape {
        destination: 1,
        source1: 2,
        base: 11,
    };
    let high = OperandShape {
        destination: 7,
        source1: 6,
        base: 13,
    };
    let cases = [
        (DotKind::Byte, false, VecWidth::V128, low),
        (DotKind::Byte, true, VecWidth::V256, high),
        (DotKind::Word, false, VecWidth::V128, low),
        (DotKind::Word, true, VecWidth::V256, high),
    ];
    let expected = [
        [0xC4, 0xE2, 0x69, 0x50, 0xC8],
        [0xC4, 0xE2, 0x4D, 0x51, 0xF8],
        [0xC4, 0xE2, 0x69, 0x52, 0xC8],
        [0xC4, 0xE2, 0x4D, 0x53, 0xF8],
    ];
    for ((kind, saturate, width, operands), expected) in cases.into_iter().zip(expected) {
        let case = MemoryCase {
            kind,
            saturate,
            width,
            operands,
        };
        assert_eq!(case.scratch(), 0, "{case:?}");
        assert_eq!(case.emitted_bytes(), expected, "{case:?}");
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: specialized classifier admitted malformed chain"
    );
    assert!(
        generic_sequence(function, true).is_none(),
        "{name}: generic classifier admitted malformed chain"
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
        X86InstructionBytes::new(bytes).expect("mutated encoding fits provenance"),
    );
}

#[test]
fn classifier_and_gate_fail_closed_for_every_chain_and_provenance_invariant() {
    let case = MemoryCase {
        kind: DotKind::Word,
        saturate: true,
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
    assert_mutation_rejected(&base, "extra same-PC operation", |function| {
        function.blocks[0]
            .ops
            .push(SmirOp::new(OpId(2), PC, OpKind::Nop));
    });
    assert_mutation_rejected(&base, "preceding same-PC operation", |function| {
        function.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(u16::MAX), PC, OpKind::Nop));
    });
    assert_mutation_rejected(&base, "load has encoding hint", |function| {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
            crate::smir::ir::ops::X86VecAlign::Unaligned,
        ));
    });
    assert_mutation_rejected(&base, "virtual address component", |function| {
        if let OpKind::VLoad { addr, .. } = &mut function.blocks[0].ops[0].kind {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
    });
    assert_mutation_rejected(&base, "consumer guest PC mismatch", |function| {
        function.blocks[0].ops[1].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "consumer has encoding hint", |function| {
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x53,
            width: VecWidth::V256,
            w: false,
        });
    });
    assert_mutation_rejected(&base, "accumulator differs from destination", |function| {
        if let OpKind::VDotProduct { acc, .. } = &mut function.blocks[0].ops[1].kind {
            *acc = vector(4, case.width);
        }
    });
    assert_mutation_rejected(&base, "consumer bypasses loaded source", |function| {
        if let OpKind::VDotProduct { src2, .. } = &mut function.blocks[0].ops[1].kind {
            *src2 = vector(4, case.width);
        }
    });
    assert_mutation_rejected(&base, "consumer width mismatch", |function| {
        if let OpKind::VDotProduct { width, .. } = &mut function.blocks[0].ops[1].kind {
            *width = VecWidth::V128;
        }
    });
    assert_mutation_rejected(&base, "wrong source element", |function| {
        if let OpKind::VDotProduct { src_elem, .. } = &mut function.blocks[0].ops[1].kind {
            *src_elem = VecElementType::I8;
        }
    });
    assert_mutation_rejected(&base, "wrong accumulator element", |function| {
        if let OpKind::VDotProduct { acc_elem, .. } = &mut function.blocks[0].ops[1].kind {
            *acc_elem = VecElementType::I64;
        }
    });
    assert_mutation_rejected(&base, "wrong first-source signedness", |function| {
        if let OpKind::VDotProduct { src1_unsigned, .. } = &mut function.blocks[0].ops[1].kind {
            *src1_unsigned = !*src1_unsigned;
        }
    });
    assert_mutation_rejected(&base, "wrong saturation", |function| {
        if let OpKind::VDotProduct { saturate, .. } = &mut function.blocks[0].ops[1].kind {
            *saturate = !*saturate;
        }
    });
    assert_mutation_rejected(&base, "unexpected mask", |function| {
        if let OpKind::VDotProduct { mask, .. } = &mut function.blocks[0].ops[1].kind {
            *mask = Some(x86(X86Reg::K(1)));
        }
    });
    assert_mutation_rejected(&base, "unexpected zeroing", |function| {
        if let OpKind::VDotProduct { zeroing, .. } = &mut function.blocks[0].ops[1].kind {
            *zeroing = true;
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only source", |function| {
        if let OpKind::VDotProduct { src1, .. } = &mut function.blocks[0].ops[1].kind {
            *src1 = vector(16, case.width);
        }
    });
    assert_mutation_rejected(&base, "missing instruction provenance", |function| {
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
    assert_mutation_rejected(&base, "reserved encoded W", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[3] ^= 1;
    assert_mutation_rejected(&base, "encoded saturation mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[3] = 0x50;
    assert_mutation_rejected(&base, "encoded element mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[1] = (bytes[1] & !0x1F) | 1;
    assert_mutation_rejected(&base, "encoded map mismatch", |function| {
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
    bytes.truncate(5);
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

fn initial_memory_state(case: MemoryCase, ordinal: usize) -> DotState {
    let mut state = initial_state(case.model_case(), ordinal);
    state.gprs[usize::from(case.operands.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    state
}

fn assert_interpreter_matches_model(
    function: &SmirFunction,
    case: MemoryCase,
    initial: &DotState,
    expected: &DotState,
    level: OptLevel,
) {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;

    let address = initial.gprs[usize::from(case.operands.base)].wrapping_add(DISP as u64) as usize;
    let mut memory = FlatMemory::new(0x10000);
    let source = words_to_bytes(initial.vectors[usize::from(MODEL_MEMORY_VECTOR)]);
    memory.load(address, &source[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, expected.gprs, "{level:?} {case:?}: GPRs");
    for (index, value) in expected.vectors.iter().enumerate() {
        assert_eq!(
            &x86.xmm[index][..8],
            value,
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(x86.k, expected.masks, "{level:?} {case:?}: masks");
    assert_eq!(x86.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

#[test]
fn memory_interpretation_matches_sdm_equations_for_every_variant_alias_and_boundary() {
    let cases = all_cases();
    let mut interpreted = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_memory_state(case, ordinal);
        let expected = architectural_expected(case.model_case(), &initial);
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            assert_interpreter_matches_model(&function, case, &initial, &expected, level);
            interpreted += 1;
        }
    }
    assert_eq!(interpreted, 2 * 2 * 2 * 6 * DIFFERENTIAL_LEVELS.len());
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
fn guest_regs(initial: &DotState) -> GuestRegs {
    GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        zmm: initial.vectors,
        k: initial.masks,
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: initial.mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    }
}

#[cfg(target_arch = "x86_64")]
fn helper_payload(case: MemoryCase, source: [u64; 8]) -> [u64; 8] {
    let words = (case.width.bytes() / 8) as usize;
    std::array::from_fn(|word| if word < words { source[word] } else { 0 })
}

#[cfg(target_arch = "x86_64")]
fn model_expected_guest(
    mut registers: GuestRegs,
    case: MemoryCase,
    initial: &DotState,
) -> GuestRegs {
    let expected = architectural_expected(case.model_case(), initial);
    registers.gpr = expected.gprs;
    registers.zmm = expected.vectors;
    registers.k = expected.masks;
    registers.rflags = expected.rflags;
    registers.mxcsr = expected.mxcsr;
    registers.vector_scratch =
        helper_payload(case, initial.vectors[usize::from(MODEL_MEMORY_VECTOR)]);
    registers
}

#[cfg(target_arch = "x86_64")]
fn vxorps_bytes(case: MemoryCase) -> [u8; 5] {
    let destination = case.operands.destination;
    let source1 = case.operands.source1;
    let scratch = case.scratch();
    [
        0xC4,
        (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if scratch < 8 { 0x20 } else { 0 }) | 1,
        (((!source1) & 0x0F) << 3) | (u8::from(case.width == VecWidth::V256) << 2),
        0x57,
        0xC0 | ((destination & 7) << 3) | (scratch & 7),
    ]
}

#[cfg(target_arch = "x86_64")]
fn patch_dot_product_to_vxorps(code: &mut [u8], case: MemoryCase) {
    let source = case.emitted_bytes();
    let offsets = code
        .windows(source.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == source).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "{case:?}");
    let offset = offsets[0];
    code[offset..offset + source.len()].copy_from_slice(&vxorps_bytes(case));
}

#[cfg(target_arch = "x86_64")]
fn vxorps_expected(mut registers: GuestRegs, case: MemoryCase, memory: [u64; 8]) -> GuestRegs {
    let source1 = words_to_bytes(registers.zmm[usize::from(case.operands.source1)]);
    let payload = words_to_bytes(helper_payload(case, memory));
    let mut destination = [0u8; 64];
    for byte in 0..case.width.bytes() as usize {
        destination[byte] = source1[byte] ^ payload[byte];
    }
    registers.zmm[usize::from(case.operands.destination)] = std::array::from_fn(|word| {
        u64::from_le_bytes(destination[word * 8..word * 8 + 8].try_into().unwrap())
    });
    registers.vector_scratch = helper_payload(case, memory);
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: MemoryCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, case.width.bytes(), "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn patched_native_boundary_executes_96_successes_and_faults_with_exact_state() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping patched base AVX-VNNI helper boundary: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (mut code, entry) = lower(&function, case);
            patch_dot_product_to_vxorps(&mut code, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let initial = initial_memory_state(case, ordinal);
            let memory = initial.vectors[usize::from(MODEL_MEMORY_VECTOR)];

            let mut context = VectorMemoryContext {
                value: memory,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(&initial);
            let address = registers.gpr[usize::from(case.operands.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = vxorps_expected(registers, case, memory);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_observation(&context, address, case, "success");
            successes += 1;

            let fault_initial = initial_memory_state(case, ordinal ^ 0x55);
            let mut context = VectorMemoryContext {
                value: memory,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = guest_regs(&fault_initial);
            let address = registers.gpr[usize::from(case.operands.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_helper_observation(&context, address, case, "fault");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}

#[cfg(target_arch = "x86_64")]
const MEMORY_CHILD_ENV: &str = "RAX_VEX_INTEGER_DOT_MEMORY_CHILD";

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_sources_match_sdm_model_on_an_avx_vnni_host() {
    if std::env::var_os(MEMORY_CHILD_ENV).is_some() {
        use crate::smir::lower::runtime::ExecMem;

        for (ordinal, case) in all_cases().into_iter().enumerate() {
            for level in DIFFERENTIAL_LEVELS {
                let function = optimize(lift_case(case), level);
                let (code, entry) = lower(&function, case);
                let exec = ExecMem::new(&code)
                    .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                let initial = initial_memory_state(case, ordinal);
                let memory = initial.vectors[usize::from(MODEL_MEMORY_VECTOR)];
                let mut context = VectorMemoryContext {
                    value: memory,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = guest_regs(&initial);
                let address =
                    registers.gpr[usize::from(case.operands.base)].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as usize as u64;
                let mut expected = model_expected_guest(registers, case, &initial);
                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}");
                assert_helper_observation(&context, address, case, "native");
            }
        }
        return;
    }
    if !std::is_x86_feature_detected!("avx") || !x86_host_has_avx_vnni() {
        eprintln!("skipping native base AVX-VNNI memory differential: host feature unavailable");
        return;
    }

    let test_name = "smir::lower::runtime::jit_gate_tests::vex_integer_dot_replay::\
                     memory_source::native_memory_sources_match_sdm_model_on_an_avx_vnni_host";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(MEMORY_CHILD_ENV, "1")
        .output()
        .expect("spawn isolated base AVX-VNNI memory differential");
    assert!(
        output.status.success(),
        "isolated base AVX-VNNI memory differential failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
