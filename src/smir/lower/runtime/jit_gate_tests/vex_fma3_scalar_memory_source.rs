//! Exact helper-backed scalar VEX FMA3 memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86FmaOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FpRoundMode, FunctionId, MemWidth, OpId, OpWidth,
    SignExtend, SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86FmaKind, X86FmaOrder,
    X86Reg,
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

const PC: u64 = 0xF3A0;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const NATIVE_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const SCALAR_OPCODES: [u8; 12] = [
    0x99, 0x9B, 0x9D, 0x9F, 0xA9, 0xAB, 0xAD, 0xAF, 0xB9, 0xBB, 0xBD, 0xBF,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarFormat {
    F32,
    F64,
}

impl ScalarFormat {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    const fn w(self) -> bool {
        matches!(self, Self::F64)
    }

    const fn elem(self) -> VecElementType {
        match self {
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::F32 => MemWidth::B4,
            Self::F64 => MemWidth::B8,
        }
    }

    const fn memory_size(self) -> u32 {
        match self {
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperandForm {
    Low,
    High,
    DestinationSourceAlias,
}

impl OperandForm {
    const ALL: [Self; 3] = [Self::Low, Self::High, Self::DestinationSourceAlias];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFmaCase {
    opcode: u8,
    format: ScalarFormat,
    l: bool,
    form: OperandForm,
}

impl ScalarFmaCase {
    const fn operands(self) -> (u8, u8, u8) {
        match self.form {
            // Destination/source2 occupy XMM0/1, forcing scratch register 2.
            OperandForm::Low => (0, 1, 3),
            // High destination/source2 and base exercise every VEX extension.
            OperandForm::High => (15, 14, 11),
            // The destructive destination and VEX.vvvv source may alias.
            OperandForm::DestinationSourceAlias => (9, 9, 11),
        }
    }

    const fn destination(self) -> u8 {
        self.operands().0
    }

    const fn source2(self) -> u8 {
        self.operands().1
    }

    const fn base(self) -> u8 {
        self.operands().2
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination() && *index != self.source2())
            .expect("two scalar VEX operands leave at least fourteen scratch registers")
    }

    const fn kind(self) -> X86FmaKind {
        match self.opcode & 0x0F {
            0x09 => X86FmaKind::Add,
            0x0B => X86FmaKind::Sub,
            0x0D => X86FmaKind::NegativeMultiplyAdd,
            0x0F => X86FmaKind::NegativeMultiplySub,
            _ => unreachable!(),
        }
    }

    const fn order(self) -> X86FmaOrder {
        match self.opcode >> 4 {
            0x09 => X86FmaOrder::Order132,
            0x0A => X86FmaOrder::Order213,
            0x0B => X86FmaOrder::Order231,
            _ => unreachable!(),
        }
    }

    fn vex_p0(self) -> u8 {
        let destination = self.destination();
        let base = self.base();
        (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 0x02
    }

    fn vex_p1(self) -> u8 {
        (u8::from(self.format.w()) << 7)
            | (((!self.source2()) & 0x0F) << 3)
            | (u8::from(self.l) << 2)
            | 0x01
    }

    fn bytes(self) -> Vec<u8> {
        vec![
            0xC4,
            self.vex_p0(),
            self.vex_p1(),
            self.opcode,
            0x40 | ((self.destination() & 7) << 3) | (self.base() & 7),
            DISP as u8,
        ]
    }

    fn emitted_fma_bytes(self) -> [u8; 5] {
        let destination = self.destination();
        let source2 = self.source2();
        let scratch = self.scratch();
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if scratch < 8 { 0x20 } else { 0 })
                | 0x02,
            (u8::from(self.format.w()) << 7) | (((!source2) & 0x0F) << 3) | 0x01,
            self.opcode,
            0xC0 | ((destination & 7) << 3) | (scratch & 7),
        ]
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn expected_address(case: ScalarFmaCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: ScalarFmaCase) {
    let elem = case.format.elem();
    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    let expected_len = 2 * xmm_lanes + 5;
    assert_eq!(ops.len(), expected_len, "{case:?}: {ops:#?}");
    assert!(
        ops.iter().all(|op| op.guest_pc == PC),
        "{case:?}: split guest provenance"
    );

    let loaded_scalar = match &ops[0].kind {
        OpKind::Load {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            assert_eq!(*width, case.format.memory_width(), "{case:?}");
            *loaded
        }
        other => panic!("{case:?}: expected scalar load, got {other:?}"),
    };
    assert_eq!(ops[0].x86_hint, None, "{case:?}");

    let source_vector = match &ops[1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } => {
            assert_eq!(*scalar, loaded_scalar, "{case:?}");
            assert_eq!(*broadcast_elem, elem, "{case:?}");
            *vector
        }
        other => panic!("{case:?}: expected memory broadcast, got {other:?}"),
    };
    assert_eq!(ops[1].x86_hint, None, "{case:?}");

    let raw = match &ops[2].kind {
        OpKind::X86Fma(X86FmaOp {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2,
            src3,
            mask,
            elem: fma_elem,
            kind,
            order,
            round,
            lanes,
        }) => {
            assert_eq!(*src1, xmm(case.destination()), "{case:?}");
            assert_eq!(*src2, xmm(case.source2()), "{case:?}");
            assert_eq!(*src3, source_vector, "{case:?}");
            assert_eq!(*mask, None, "{case:?}");
            assert_eq!(*fma_elem, elem, "{case:?}");
            assert_eq!(*kind, case.kind(), "{case:?}");
            assert_eq!(*order, case.order(), "{case:?}");
            assert_eq!(*round, FpRoundMode::Dynamic, "{case:?}");
            assert_eq!(*lanes, 1, "{case:?}");
            *raw
        }
        other => panic!("{case:?}: expected scalar X86Fma, got {other:?}"),
    };
    assert_eq!(
        ops[2].x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: case.opcode,
            width: if case.l {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            w: case.format.w(),
        }),
        "{case:?}"
    );
    assert!(ops[2].kind.has_side_effects(), "{case:?}: MXCSR");

    let scalar_result = match &ops[3].kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*vec, raw, "{case:?}");
            assert_eq!(*extract_elem, elem, "{case:?}");
            *scalar
        }
        other => panic!("{case:?}: expected result extraction, got {other:?}"),
    };

    let mut upper_scalars = Vec::new();
    for lane in 1..xmm_lanes {
        let scalar = match &ops[3 + lane].kind {
            OpKind::VExtractLane {
                dst: scalar @ VReg::Virtual(_),
                vec,
                lane: extract_lane,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*vec, xmm(case.destination()), "{case:?}");
                assert_eq!(usize::from(*extract_lane), lane, "{case:?}");
                assert_eq!(*extract_elem, elem, "{case:?}");
                *scalar
            }
            other => panic!("{case:?}: expected upper extraction {lane}, got {other:?}"),
        };
        upper_scalars.push(scalar);
    }

    let zero_offset = 3 + xmm_lanes;
    let zero = match &ops[zero_offset].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *zero,
        other => panic!("{case:?}: expected vector-clear zero, got {other:?}"),
    };
    assert!(matches!(
        &ops[zero_offset + 1].kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: clear_elem,
            lanes: 1,
        } if *dst == xmm(case.destination()) && *scalar == zero && *clear_elem == elem
    ));
    assert!(matches!(
        &ops[zero_offset + 2].kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane: 0,
            elem: insert_elem,
        } if *dst == xmm(case.destination())
            && *vec == xmm(case.destination())
            && *scalar == scalar_result
            && *insert_elem == elem
    ));
    for (lane, scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        assert!(matches!(
            &ops[zero_offset + 2 + lane].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: insert_lane,
                elem: insert_elem,
            } if *dst == xmm(case.destination())
                && *vec == xmm(case.destination())
                && *inserted == scalar
                && usize::from(*insert_lane) == lane
                && *insert_elem == elem
        ));
    }
    for (index, op) in ops.iter().enumerate() {
        if index != 2 {
            assert_eq!(op.x86_hint, None, "{case:?} op {index}");
        }
    }
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
        X86InstructionBytes::new(bytes).expect("VEX scalar FMA3 instruction metadata"),
    );
    function
}

fn lift_case(case: ScalarFmaCase) -> SmirFunction {
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

fn sequence(function: &SmirFunction) -> crate::smir::lower::runtime::X86JitVexBinaryMemorySequence {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_vex_binary_memory_sequence(
        &function.blocks[0],
        0,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
    .unwrap_or_else(|| {
        panic!(
            "exact scalar VEX FMA3 memory sequence: {:#?}",
            function.blocks[0].ops
        )
    })
}

fn lower(function: &SmirFunction, case: ScalarFmaCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma"),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed scalar FMA3 lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed scalar VEX FMA3"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<ScalarFmaCase> {
    let mut cases = Vec::new();
    for opcode in SCALAR_OPCODES {
        for format in ScalarFormat::ALL {
            for l in [false, true] {
                for form in OperandForm::ALL {
                    cases.push(ScalarFmaCase {
                        opcode,
                        format,
                        l,
                        form,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn scalar_fma3_memory_byte_classifier_is_exhaustive_and_exact_for_vex_lig() {
    let mut accepted = 0usize;
    for opcode in 0..=u8::MAX {
        for w in [false, true] {
            for l in [false, true] {
                for destination in 0..16 {
                    for source2 in 0..16 {
                        let bytes = [
                            0xC4,
                            (if destination < 8 { 0x80 } else { 0 }) | 0x62,
                            (u8::from(w) << 7)
                                | (((!source2) & 0x0F) << 3)
                                | (u8::from(l) << 2)
                                | 1,
                            opcode,
                            0x40 | ((destination & 7) << 3) | 3,
                            0x20,
                        ];
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        let fields = instruction.vex_memory_scalar_fma3_fields();
                        if SCALAR_OPCODES.contains(&opcode) {
                            assert_eq!(
                                fields,
                                Some((destination, source2, opcode, w)),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                        } else {
                            assert_eq!(fields, None, "{bytes:02X?}");
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 12 * 2 * 2 * 16 * 16);

    let valid = ScalarFmaCase {
        opcode: 0x99,
        format: ScalarFormat::F32,
        l: false,
        form: OperandForm::Low,
    }
    .bytes();
    let mut malformed = Vec::new();
    malformed.push(valid[..valid.len() - 1].to_vec());
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[4] |= 0xC0;
    register.truncate(5);
    malformed.push(register);
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
    malformed.push(wrong_map);
    let mut wrong_prefix = valid.clone();
    wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
    malformed.push(wrong_prefix);
    let mut disallowed_legacy = valid.clone();
    disallowed_legacy.insert(0, 0xF0);
    malformed.push(disallowed_legacy);
    for bytes in malformed {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_memory_scalar_fma3_fields(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn all_144_scalar_fma3_opcode_format_lig_and_alias_shapes_lower_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 12 * 2 * 2 * 3);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);
            let actual = sequence(&function);
            assert_eq!(actual.consumed, if case.format.w() { 9 } else { 13 });
            assert_eq!(actual.memory_size, case.format.memory_size());
            assert_eq!(actual.destination, case.destination());
            assert_eq!(actual.source1, case.source2());
            assert_eq!(actual.width, VecWidth::V128);
            assert_eq!(actual.map, X86VecMap::Map0F38);
            assert_eq!(actual.prefix, X86SsePrefix::OpSize);
            assert_eq!(actual.opcode, case.opcode);
            assert_eq!(actual.w, case.format.w());
            assert!(!actual.needs_avx2);
            assert!(actual.needs_fma);

            let (code, _) = lower(&function, case);
            assert!(
                code.windows(5)
                    .any(|window| { window == [0xB9, case.format.memory_size() as u8, 0, 0, 0] }),
                "{level:?} {case:?}: missing scalar helper size"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector-scratch displacement"
            );
            let expected = case.emitted_fma_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            // The guest L bit is ignored and is canonicalized to L=0.
            assert_eq!(expected[2] & 0x04, 0, "{level:?} {case:?}");
            lowered += 1;
        }
    }
    assert_eq!(lowered, 144 * LEVELS.len());
}

#[test]
fn scalar_fma3_prefixed_sib_rip_relative_and_llvm_encoding_oracles_are_exact() {
    let address_cases: &[&[u8]] = &[
        // FS + address-size override, VFMADD132SS xmm0,xmm1,fs:[ebx+esi*2+0x20].
        &[0x64, 0x67, 0xC4, 0xE2, 0x71, 0x99, 0x44, 0x73, 0x20],
        // VEX.L=1 VFNMSUB213SS xmm15,xmm14,[r11+0x20].
        &[0xC4, 0x42, 0x0D, 0xAF, 0x7B, 0x20],
        // VFMADD231SD xmm0,xmm1,[rip+0x44332211].
        &[0xC4, 0xE2, 0xF1, 0xB9, 0x05, 0x11, 0x22, 0x33, 0x44],
    ];
    for bytes in address_cases {
        let function = lift_bytes(bytes);
        let (definitions, uses) = virtual_counts(&function);
        let actual = x86_jit_vex_binary_memory_sequence(
            &function.blocks[0],
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        );
        assert!(actual.is_some(), "{bytes:02X?}");
    }

    // LLVM 23 independent register-form encodings. The helper-backed lowerer
    // substitutes its scratch register for the original memory source.
    let oracle_cases = [
        (
            ScalarFmaCase {
                opcode: 0x99,
                format: ScalarFormat::F32,
                l: false,
                form: OperandForm::Low,
            },
            [0xC4, 0xE2, 0x71, 0x99, 0xC2],
        ),
        (
            ScalarFmaCase {
                opcode: 0xBB,
                format: ScalarFormat::F64,
                l: true,
                form: OperandForm::High,
            },
            [0xC4, 0x62, 0x89, 0xBB, 0xF8],
        ),
        (
            ScalarFmaCase {
                opcode: 0xAD,
                format: ScalarFormat::F32,
                l: false,
                form: OperandForm::DestinationSourceAlias,
            },
            [0xC4, 0x62, 0x31, 0xAD, 0xC8],
        ),
    ];
    for (case, llvm_bytes) in oracle_cases {
        assert_eq!(case.emitted_fma_bytes(), llvm_bytes, "{case:?}");
        let function = lift_case(case);
        let (code, _) = lower(&function, case);
        assert!(
            code.windows(llvm_bytes.len())
                .any(|window| window == llvm_bytes),
            "{case:?}: missing LLVM encoding {llvm_bytes:02X?}"
        );
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_vex_binary_memory_sequence(
            &function.blocks[0],
            0,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: classifier admitted malformed scalar FMA3 chain"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed scalar FMA3 chain"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed scalar FMA3 chain"
    );
}

#[test]
fn scalar_fma3_classifier_and_lowerer_fail_closed_for_graph_and_provenance_invariants() {
    let case = ScalarFmaCase {
        opcode: 0x99,
        format: ScalarFormat::F32,
        l: false,
        form: OperandForm::Low,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let raw = match base.blocks[0].ops[2].kind {
        OpKind::X86Fma(X86FmaOp { dst, .. }) => dst,
        _ => unreachable!(),
    };

    let mut missing_metadata = base.clone();
    missing_metadata.x86_instruction_bytes.clear();

    let mut mismatched_metadata = Vec::new();
    for (name, byte, xor) in [
        ("metadata destination", 4usize, 0x08u8),
        ("metadata VEX.vvvv", 2, 0x08),
        ("metadata W", 2, 0x80),
        ("metadata opcode", 3, 0x02),
    ] {
        let mut function = base.clone();
        let mut bytes = case.bytes();
        bytes[byte] ^= xor;
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        mismatched_metadata.push((name, function));
    }
    let mut trailing_metadata = base.clone();
    let mut trailing = case.bytes();
    trailing.push(0);
    trailing_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&trailing).unwrap(),
    );

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = base.blocks[0].ops[2].x86_hint;

    let mut wrong_load_width = base.clone();
    if let OpKind::Load { width, .. } = &mut wrong_load_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }

    let mut signed_load = base.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }

    let mut virtual_address = base.clone();
    if let OpKind::Load { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut loaded_used_twice = base.clone();
    loaded_used_twice.blocks[0].ops.push(SmirOp::new(
        OpId(0xF0),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFE)),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W32,
        },
    ));

    let mut wrong_broadcast = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_broadcast.blocks[0].ops[1].kind {
        *lanes = 2;
    }

    let mut fma_wrong_pc = base.clone();
    fma_wrong_pc.blocks[0].ops[2].guest_pc += 1;

    let mut fma_hint_missing = base.clone();
    fma_hint_missing.blocks[0].ops[2].x86_hint = None;

    let mut fma_wrong_map = base.clone();
    if let Some(X86OpHint::VexOp { map, .. }) = &mut fma_wrong_map.blocks[0].ops[2].x86_hint {
        *map = X86VecMap::Map0F;
    }

    let mut fma_wrong_destination = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_wrong_destination.blocks[0].ops[2].kind {
        op.src1 = xmm(2);
    }

    let mut fma_wrong_source2 = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_wrong_source2.blocks[0].ops[2].kind {
        op.src2 = xmm(2);
    }

    let mut fma_bypasses_load = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_bypasses_load.blocks[0].ops[2].kind {
        op.src3 = xmm(2);
    }

    let mut fma_masked = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_masked.blocks[0].ops[2].kind {
        op.mask = Some(x86(X86Reg::K(1)));
    }

    let mut fma_wrong_elem = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_wrong_elem.blocks[0].ops[2].kind {
        op.elem = VecElementType::F64;
    }

    let mut fma_wrong_lanes = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_wrong_lanes.blocks[0].ops[2].kind {
        op.lanes = 2;
    }

    let mut fma_wrong_kind = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_wrong_kind.blocks[0].ops[2].kind {
        op.kind = X86FmaKind::Sub;
    }

    let mut fma_wrong_order = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_wrong_order.blocks[0].ops[2].kind {
        op.order = X86FmaOrder::Order231;
    }

    let mut fma_explicit_round = base.clone();
    if let OpKind::X86Fma(op) = &mut fma_explicit_round.blocks[0].ops[2].kind {
        op.round = FpRoundMode::RoundUp;
    }

    let mut raw_used_twice = base.clone();
    raw_used_twice.blocks[0].ops.push(SmirOp::new(
        OpId(0xF1),
        PC + 1,
        OpKind::VMov {
            dst: xmm(2),
            src: raw,
            width: VecWidth::V128,
        },
    ));

    let mut result_wrong_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut result_wrong_lane.blocks[0].ops[3].kind {
        *lane = 1;
    }

    let mut upper_wrong_source = base.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut upper_wrong_source.blocks[0].ops[4].kind {
        *vec = xmm(case.source2());
    }

    let mut nonzero_clear = base.clone();
    if let OpKind::Mov { src, .. } = &mut nonzero_clear.blocks[0].ops[7].kind {
        *src = SrcOperand::Imm(1);
    }

    let mut wrong_clear_destination = base.clone();
    if let OpKind::VBroadcast { dst, .. } = &mut wrong_clear_destination.blocks[0].ops[8].kind {
        *dst = xmm(2);
    }

    let mut wrong_insert_scalar = base.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut wrong_insert_scalar.blocks[0].ops[9].kind {
        *scalar = loaded;
    }

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0].ops.push(SmirOp::new(
        OpId(0xF2),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xFFFD)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));

    let mut malformed = vec![
        ("missing metadata", missing_metadata),
        ("trailing metadata", trailing_metadata),
        ("unexpected load hint", load_hint),
        ("wrong scalar load width", wrong_load_width),
        ("signed scalar load", signed_load),
        ("virtual address component", virtual_address),
        ("loaded temporary used twice", loaded_used_twice),
        ("wrong broadcast", wrong_broadcast),
        ("FMA guest PC mismatch", fma_wrong_pc),
        ("missing FMA hint", fma_hint_missing),
        ("wrong FMA map", fma_wrong_map),
        ("wrong destructive destination", fma_wrong_destination),
        ("wrong VEX.vvvv source", fma_wrong_source2),
        ("FMA bypasses load", fma_bypasses_load),
        ("masked FMA", fma_masked),
        ("wrong FMA element", fma_wrong_elem),
        ("wrong FMA lanes", fma_wrong_lanes),
        ("wrong FMA kind", fma_wrong_kind),
        ("wrong FMA order", fma_wrong_order),
        ("explicit FMA rounding", fma_explicit_round),
        ("raw temporary used twice", raw_used_twice),
        ("wrong result lane", result_wrong_lane),
        ("wrong upper source", upper_wrong_source),
        ("nonzero clear", nonzero_clear),
        ("wrong clear destination", wrong_clear_destination),
        ("wrong insert scalar", wrong_insert_scalar),
        ("same-PC tail", same_pc_tail),
    ];
    malformed.extend(mismatched_metadata);
    for (name, function) in malformed {
        assert_rejected(name, &function);
    }
}

#[cfg(target_arch = "x86_64")]
fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct ScalarMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut ScalarMemoryContext) };
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

    let mut bytes = if zero_upper != 0 {
        [0; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    let value = words_to_bytes(context.value);
    bytes[..size as usize].copy_from_slice(&value[..size as usize]);
    state.vector_scratch = bytes_to_words(bytes);
    1
}

#[cfg(target_arch = "x86_64")]
fn role_scalar(format: ScalarFormat, data_case: usize, role: usize) -> u64 {
    const F32: [[u32; 3]; 4] = [
        [0x3FC0_0000, 0x4000_0000, 0x4040_0000],
        [0x3F80_0001, 0x3F7F_FFFF, 0x3380_0000],
        [0x7FC0_0011, 0x7F80_0022, 0x7F80_0000],
        [0x0080_0000, 0x7F7F_FFFF, 0x4000_0000],
    ];
    const F64: [[u64; 3]; 4] = [
        [
            0x3FF8_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x4008_0000_0000_0000,
        ],
        [
            0x3FF0_0000_0000_0001,
            0x3FEF_FFFF_FFFF_FFFF,
            0x3CA0_0000_0000_0000,
        ],
        [
            0x7FF8_0000_0000_0011,
            0x7FF0_0000_0000_0022,
            0x7FF0_0000_0000_0000,
        ],
        [
            0x0010_0000_0000_0000,
            0x7FEF_FFFF_FFFF_FFFF,
            0x4000_0000_0000_0000,
        ],
    ];
    match format {
        ScalarFormat::F32 => u64::from(F32[data_case % F32.len()][role]),
        ScalarFormat::F64 => F64[data_case % F64.len()][role],
    }
}

#[cfg(target_arch = "x86_64")]
fn source_words(case: ScalarFmaCase, data_case: usize) -> [u64; 8] {
    let mut bytes = [0xA5; 64];
    let scalar = role_scalar(case.format, data_case, 2);
    match case.format {
        ScalarFormat::F32 => bytes[..4].copy_from_slice(&(scalar as u32).to_le_bytes()),
        ScalarFormat::F64 => bytes[..8].copy_from_slice(&scalar.to_le_bytes()),
    }
    bytes_to_words(bytes)
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: ScalarFmaCase, ordinal: usize, data_case: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        // All exception masks remain set. RC and prior status vary, while
        // DAZ/FTZ remain clear for native-vs-translated-host portability.
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
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

    let mut set_low_scalar = |index: u8, role: usize| {
        let scalar = role_scalar(case.format, data_case, role);
        match case.format {
            ScalarFormat::F32 => {
                let word = &mut registers.zmm[usize::from(index)][0];
                *word = (*word & !u64::from(u32::MAX)) | scalar;
            }
            ScalarFormat::F64 => registers.zmm[usize::from(index)][0] = scalar,
        }
    };
    set_low_scalar(case.destination(), 0);
    if case.source2() != case.destination() {
        set_low_scalar(case.source2(), 1);
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x80;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: [u64; 8],
    address: u64,
    case: ScalarFmaCase,
) -> GuestRegs {
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
    let bytes = words_to_bytes(source);
    memory.load(
        address as usize,
        &bytes[..case.format.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&value[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    let mut scratch = [0; 64];
    scratch[..case.format.memory_size() as usize]
        .copy_from_slice(&bytes[..case.format.memory_size() as usize]);
    expected.vector_scratch = bytes_to_words(scratch);
    expected
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_scalar_fma3_memory_matches_o0_o2_interpretation_and_faults_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("fma") {
        eprintln!("skipping native scalar VEX FMA3 memory differential: host lacks AVX/FMA");
        return;
    }

    let cases = all_cases();
    assert_eq!(cases.len(), 12 * 2 * 2 * 3);
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            // Rosetta 2 can report spurious FMA status on boundary
            // cancellation. Keep every opcode/order/format/L/alias and MXCSR
            // RC combination there, but use the exact finite data set. Native
            // x86-64 hosts cycle NaNs, infinities, subnormals, and overflow
            // boundaries as well.
            #[cfg(target_os = "macos")]
            let data_case = if running_under_rosetta() { 0 } else { ordinal };
            #[cfg(not(target_os = "macos"))]
            let data_case = ordinal;
            let source = source_words(case, data_case);

            let mut context = ScalarMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal, data_case);
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.vec_load_fn = scalar_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, address, case);

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
            successes += 1;

            let mut context = ScalarMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal ^ 0x55, data_case);
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
            registers.vec_load_fn = scalar_load_helper as usize as u64;
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
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
