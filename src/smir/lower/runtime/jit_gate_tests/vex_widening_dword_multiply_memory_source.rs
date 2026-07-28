//! Exact helper-backed VEX VPMULUDQ/VPMULDQ memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
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

const PC: u64 = 0xBC80;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    Vex2,
    Vex3W0,
    Vex3W1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WideningMultiplyMemoryCase {
    signed: bool,
    width: VecWidth,
    form: VexForm,
    alias: bool,
}

impl WideningMultiplyMemoryCase {
    const fn operands(self) -> (u8, u8, u8) {
        match (self.form, self.alias) {
            (VexForm::Vex2, false) => (14, 9, 3),
            (VexForm::Vex2, true) => (15, 15, 3),
            (VexForm::Vex3W0, false) => (0, 1, 3),
            (VexForm::Vex3W0, true) => (0, 0, 3),
            (VexForm::Vex3W1, false) => (14, 9, 11),
            (VexForm::Vex3W1, true) => (15, 15, 11),
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

    const fn w(self) -> bool {
        matches!(self.form, VexForm::Vex3W1)
    }

    const fn map(self) -> X86VecMap {
        if self.signed {
            X86VecMap::Map0F38
        } else {
            X86VecMap::Map0F
        }
    }

    const fn map_bits(self) -> u8 {
        if self.signed { 2 } else { 1 }
    }

    const fn opcode(self) -> u8 {
        if self.signed { 0x28 } else { 0xF4 }
    }

    const fn qwords(self) -> usize {
        match self.width {
            VecWidth::V128 => 2,
            VecWidth::V256 => 4,
            _ => unreachable!(),
        }
    }

    const fn consumed(self) -> usize {
        4 * self.qwords() + 4
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination() && *index != self.source1())
            .expect("two VEX operands leave at least fourteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let (destination, source1, base) = self.operands();
        let l = u8::from(self.width == VecWidth::V256);
        match self.form {
            VexForm::Vex2 => {
                assert!(!self.signed);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 })
                        | (((!source1) & 0x0F) << 3)
                        | (l << 2)
                        | 1,
                    self.opcode(),
                    0x40 | ((destination & 7) << 3) | base,
                    DISP as u8,
                ]
            }
            VexForm::Vex3W0 | VexForm::Vex3W1 => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | self.map_bits(),
                (u8::from(self.w()) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                self.opcode(),
                0x40 | ((destination & 7) << 3) | (base & 7),
                DISP as u8,
            ],
        }
    }

    fn emitted_bytes(self) -> Vec<u8> {
        let destination = self.destination();
        let source1 = self.source1();
        let scratch = self.scratch();
        let l = u8::from(self.width == VecWidth::V256);
        if self.signed {
            vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if scratch < 8 { 0x20 } else { 0 })
                    | 2,
                (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                0x28,
                0xC0 | ((destination & 7) << 3) | (scratch & 7),
            ]
        } else {
            vec![
                0xC5,
                (if destination < 8 { 0x80 } else { 0 })
                    | (((!source1) & 0x0F) << 3)
                    | (l << 2)
                    | 1,
                0xF4,
                0xC0 | ((destination & 7) << 3) | scratch,
            ]
        }
    }
}

fn all_cases() -> Vec<WideningMultiplyMemoryCase> {
    let mut cases = Vec::new();
    for signed in [false, true] {
        for width in [VecWidth::V128, VecWidth::V256] {
            for form in [VexForm::Vex2, VexForm::Vex3W0, VexForm::Vex3W1] {
                if signed && form == VexForm::Vex2 {
                    continue;
                }
                for alias in [false, true] {
                    cases.push(WideningMultiplyMemoryCase {
                        signed,
                        width,
                        form,
                        alias,
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
        _ => unreachable!("VEX VPMULUDQ/VPMULDQ has only 128-/256-bit forms"),
    })
}

fn expected_address(case: WideningMultiplyMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_chain(ops: &[SmirOp], case: WideningMultiplyMemoryCase) {
    assert_eq!(ops.len(), case.consumed(), "{case:?}: {ops:?}");
    let load = &ops[0];
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
    let sign = if case.signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let mut products = Vec::new();
    for lane in 0..case.qwords() {
        let offset = 1 + lane * 3;
        let a = match &ops[offset].kind {
            OpKind::VExtractLane {
                dst: a @ VReg::Virtual(_),
                vec,
                lane: source_lane,
                elem: VecElementType::I32,
                sign: actual_sign,
            } => {
                assert_eq!(
                    *vec,
                    vector(case.source1(), case.width),
                    "{case:?} lane {lane}"
                );
                assert_eq!(usize::from(*source_lane), lane * 2, "{case:?}");
                assert_eq!(*actual_sign, sign, "{case:?}");
                *a
            }
            other => panic!("{case:?} lane {lane}: first extract {other:?}"),
        };
        let b = match &ops[offset + 1].kind {
            OpKind::VExtractLane {
                dst: b @ VReg::Virtual(_),
                vec,
                lane: source_lane,
                elem: VecElementType::I32,
                sign: actual_sign,
            } => {
                assert_eq!(*vec, loaded, "{case:?}");
                assert_eq!(usize::from(*source_lane), lane * 2, "{case:?}");
                assert_eq!(*actual_sign, sign, "{case:?}");
                *b
            }
            other => panic!("{case:?} lane {lane}: second extract {other:?}"),
        };
        let product = match (&ops[offset + 2].kind, case.signed) {
            (
                OpKind::MulS {
                    dst_lo: product @ VReg::Virtual(_),
                    dst_hi: None,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                true,
            )
            | (
                OpKind::MulU {
                    dst_lo: product @ VReg::Virtual(_),
                    dst_hi: None,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                false,
            ) => {
                assert_eq!(*src1, a, "{case:?}");
                assert_eq!(*src2, b, "{case:?}");
                *product
            }
            (other, _) => panic!("{case:?} lane {lane}: product {other:?}"),
        };
        for op in &ops[offset..offset + 3] {
            assert_eq!(op.guest_pc, PC, "{case:?}");
            assert_eq!(op.x86_hint, None, "{case:?}");
        }
        products.push(product);
    }

    let zero_offset = 1 + case.qwords() * 3;
    let zero = match ops[zero_offset].kind {
        OpKind::Mov {
            dst: zero @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => zero,
        ref other => panic!("{case:?}: zero {other:?}"),
    };
    let output = match ops[zero_offset + 1].kind {
        OpKind::VBroadcast {
            dst: output @ VReg::Virtual(_),
            scalar,
            elem: VecElementType::I64,
            lanes,
        } => {
            assert_eq!(scalar, zero, "{case:?}");
            assert_eq!(usize::from(lanes), case.qwords(), "{case:?}");
            output
        }
        ref other => panic!("{case:?}: zero broadcast {other:?}"),
    };
    let first_insert = zero_offset + 2;
    for (lane, product) in products.into_iter().enumerate() {
        assert!(matches!(
            ops[first_insert + lane].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: insert_lane,
                elem: VecElementType::I64,
            } if dst == output
                && vec == output
                && scalar == product
                && usize::from(insert_lane) == lane
        ));
    }
    assert!(matches!(
        ops[first_insert + case.qwords()].kind,
        OpKind::VMov {
            dst,
            src,
            width,
        } if dst == vector(case.destination(), case.width)
            && src == output
            && width == case.width
    ));
    for op in &ops[zero_offset..] {
        assert_eq!(op.guest_pc, PC, "{case:?}");
        assert_eq!(op.x86_hint, None, "{case:?}");
    }
}

fn lift_case(case: WideningMultiplyMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_exact_chain(&result.ops, case);

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&bytes).expect("VEX instruction fits metadata"),
    );
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

fn lower(function: &SmirFunction, case: WideningMultiplyMemoryCase) -> (Vec<u8>, usize) {
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
    assert_eq!(
        requirements.needs_avx2,
        case.width == VecWidth::V256,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_fma);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("helper-backed VEX VPMULUDQ/VPMULDQ lowering failed: {error:?}")
    });
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX VPMULUDQ/VPMULDQ"),
        result.entry_offset,
    )
}

#[test]
fn every_signedness_width_form_alias_and_optimizer_chain_is_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 2 * 2 * 3 * 2 - 2 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_chain(&function.blocks[0].ops, case);

            let actual = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(actual.consumed, case.consumed(), "{level:?} {case:?}");
            assert_eq!(actual.memory_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(actual.destination, case.destination(), "{level:?} {case:?}");
            assert_eq!(actual.source1, case.source1(), "{level:?} {case:?}");
            assert_eq!(actual.width, case.width, "{level:?} {case:?}");
            assert_eq!(actual.map, case.map(), "{level:?} {case:?}");
            assert_eq!(actual.prefix, X86SsePrefix::OpSize, "{level:?} {case:?}");
            assert_eq!(actual.opcode, case.opcode(), "{level:?} {case:?}");
            assert!(!actual.w, "{level:?} {case:?}: WIG replay must use W=0");
            assert_eq!(
                actual.needs_avx2,
                case.width == VecWidth::V256,
                "{level:?} {case:?}"
            );
            assert!(!actual.needs_fma, "{level:?} {case:?}");
            assert!(
                sequence(&function, false).is_none(),
                "{level:?} {case:?}: memory-disabled gate admitted sequence"
            );

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
            let expected = case.emitted_bytes();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 20 * LEVELS.len());
}

#[test]
fn register_rewrite_matches_independent_llvm_23_encodings() {
    for (case, expected) in [
        (
            WideningMultiplyMemoryCase {
                signed: false,
                width: VecWidth::V128,
                form: VexForm::Vex3W0,
                alias: false,
            },
            &[0xC5, 0xF1, 0xF4, 0xC2][..],
        ),
        (
            WideningMultiplyMemoryCase {
                signed: true,
                width: VecWidth::V128,
                form: VexForm::Vex3W0,
                alias: false,
            },
            &[0xC4, 0xE2, 0x71, 0x28, 0xC2][..],
        ),
        (
            WideningMultiplyMemoryCase {
                signed: false,
                width: VecWidth::V128,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC5, 0x31, 0xF4, 0xF0][..],
        ),
        (
            WideningMultiplyMemoryCase {
                signed: true,
                width: VecWidth::V256,
                form: VexForm::Vex3W1,
                alias: false,
            },
            &[0xC4, 0x62, 0x35, 0x28, 0xF0][..],
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
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed chain"
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
fn classifier_and_lowerer_fail_closed_for_every_chain_and_provenance_invariant() {
    let case = WideningMultiplyMemoryCase {
        signed: true,
        width: VecWidth::V256,
        form: VexForm::Vex3W1,
        alias: false,
    };
    let base = lift_case(case);
    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let zero_offset = 1 + case.qwords() * 3;
    let first_insert = zero_offset + 2;

    assert_mutation_rejected(&base, "loaded value has an external use", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(case.consumed() as u16),
            PC,
            OpKind::VMov {
                dst: vector(4, VecWidth::V256),
                src: loaded,
                width: VecWidth::V256,
            },
        ));
    });
    assert_mutation_rejected(&base, "loaded value has another definition", |function| {
        function.blocks[0].ops.push(SmirOp::new(
            OpId(case.consumed() as u16),
            PC,
            OpKind::VLoad {
                dst: loaded,
                addr: expected_address(case),
                width: VecWidth::V256,
            },
        ));
    });
    assert_mutation_rejected(&base, "load carries a hint", |function| {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(
            crate::smir::ir::ops::X86VecAlign::Unaligned,
        ));
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
    assert_mutation_rejected(&base, "different guest PCs", |function| {
        function.blocks[0].ops[1].guest_pc += 1;
    });
    assert_mutation_rejected(&base, "first extract uses odd lane", |function| {
        if let OpKind::VExtractLane { lane, .. } = &mut function.blocks[0].ops[1].kind {
            *lane = 1;
        }
    });
    assert_mutation_rejected(&base, "first extract uses wrong source", |function| {
        if let OpKind::VExtractLane { vec, .. } = &mut function.blocks[0].ops[1].kind {
            *vec = vector(2, VecWidth::V256);
        }
    });
    assert_mutation_rejected(&base, "second extract bypasses load", |function| {
        if let OpKind::VExtractLane { vec, .. } = &mut function.blocks[0].ops[2].kind {
            *vec = vector(2, VecWidth::V256);
        }
    });
    assert_mutation_rejected(&base, "lane signedness mismatch", |function| {
        if let OpKind::VExtractLane { sign, .. } = &mut function.blocks[0].ops[2].kind {
            *sign = SignExtend::Zero;
        }
    });
    assert_mutation_rejected(&base, "multiply signedness mismatch", |function| {
        let kind = function.blocks[0].ops[3].kind.clone();
        let OpKind::MulS {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width,
            flags,
        } = kind
        else {
            unreachable!()
        };
        function.blocks[0].ops[3].kind = OpKind::MulU {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width,
            flags,
        };
    });
    assert_mutation_rejected(&base, "multiply writes high half", |function| {
        if let OpKind::MulS { dst_hi, .. } = &mut function.blocks[0].ops[3].kind {
            *dst_hi = Some(VReg::Virtual(VirtualId(0xFFFE)));
        }
    });
    assert_mutation_rejected(&base, "zero constructor is nonzero", |function| {
        if let OpKind::Mov { src, .. } = &mut function.blocks[0].ops[zero_offset].kind {
            *src = SrcOperand::Imm(1);
        }
    });
    assert_mutation_rejected(&base, "broadcast lane geometry mismatch", |function| {
        if let OpKind::VBroadcast { lanes, .. } = &mut function.blocks[0].ops[zero_offset + 1].kind
        {
            *lanes -= 1;
        }
    });
    assert_mutation_rejected(&base, "insert uses wrong lane", |function| {
        if let OpKind::VInsertLane { lane, .. } = &mut function.blocks[0].ops[first_insert].kind {
            *lane = 1;
        }
    });
    assert_mutation_rejected(&base, "final move has wrong width", |function| {
        if let OpKind::VMov { width, .. } = &mut function.blocks[0].ops[case.consumed() - 1].kind {
            *width = VecWidth::V128;
        }
    });
    assert_mutation_rejected(&base, "high EVEX-only destination", |function| {
        if let OpKind::VMov { dst, .. } = &mut function.blocks[0].ops[case.consumed() - 1].kind {
            *dst = vector(16, VecWidth::V256);
        }
    });
    assert_mutation_rejected(&base, "missing instruction-byte provenance", |function| {
        function.x86_instruction_bytes.clear();
    });

    let mut bytes = case.bytes();
    bytes[4] = (bytes[4] & !0x38) | 0x28;
    assert_mutation_rejected(&base, "encoded destination mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] = (bytes[2] & !0x78) | (((!2u8) & 0x0F) << 3);
    assert_mutation_rejected(&base, "encoded first-source mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[2] &= !0x04;
    assert_mutation_rejected(&base, "encoded width mismatch", |function| {
        replace_instruction_bytes(function, &bytes);
    });
    let mut bytes = case.bytes();
    bytes[1] = (bytes[1] & !0x1F) | 1;
    bytes[3] = 0xF4;
    assert_mutation_rejected(&base, "encoded signedness mismatch", |function| {
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

const DWORD_BOUNDARIES: [u32; 12] = [
    0,
    1,
    2,
    0x3FFF_FFFF,
    0x4000_0000,
    0x7FFF_FFFF,
    0x8000_0000,
    0x8000_0001,
    0xBFFF_FFFF,
    0xFFFF_FFFD,
    0xFFFF_FFFE,
    0xFFFF_FFFF,
];

fn operand_vectors(case: WideningMultiplyMemoryCase) -> ([u64; 8], [u64; 8]) {
    let mut source1 = [0xC3; 64];
    let mut source2 = [0x5A; 64];
    for lane in 0..case.qwords() {
        let first =
            DWORD_BOUNDARIES[(lane * 3 + usize::from(case.signed)) % DWORD_BOUNDARIES.len()];
        let second =
            DWORD_BOUNDARIES[(lane * 5 + 7 + usize::from(case.signed)) % DWORD_BOUNDARIES.len()];
        let odd1 = 0xA5A5_5A5Au32.rotate_left((lane * 7) as u32);
        let odd2 = 0x3CC3_C33Cu32.rotate_right((lane * 5) as u32);
        source1[lane * 8..lane * 8 + 4].copy_from_slice(&first.to_le_bytes());
        source1[lane * 8 + 4..lane * 8 + 8].copy_from_slice(&odd1.to_le_bytes());
        source2[lane * 8..lane * 8 + 4].copy_from_slice(&second.to_le_bytes());
        source2[lane * 8 + 4..lane * 8 + 8].copy_from_slice(&odd2.to_le_bytes());
    }
    (bytes_to_words(source1), bytes_to_words(source2))
}

fn model_qword(first: u32, second: u32, signed: bool) -> u64 {
    if signed {
        (i64::from(first as i32) * i64::from(second as i32)) as u64
    } else {
        u64::from(first) * u64::from(second)
    }
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
fn full_guest_regs(case: WideningMultiplyMemoryCase, ordinal: usize) -> GuestRegs {
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
    let (source1, _) = operand_vectors(case);
    registers.zmm[usize::from(case.source1())] = source1;
    if case.destination() != case.source1() {
        registers.zmm[usize::from(case.destination())] = std::array::from_fn(|word| {
            0xA55A_F00F_6996_3CC3u64.rotate_left((word * 7 + ordinal) as u32)
        });
    }
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: WideningMultiplyMemoryCase,
    source2: [u64; 8],
) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1())];
    let mut result = [0; 8];
    for lane in 0..case.qwords() {
        result[lane] = model_qword(source1[lane] as u32, source2[lane] as u32, case.signed);
    }
    registers.zmm[usize::from(case.destination())] = result;
    let words = (case.width.bytes() / 8) as usize;
    registers.vector_scratch =
        std::array::from_fn(|word| if word < words { source2[word] } else { 0 });
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source2: [u64; 8],
    address: u64,
    case: WideningMultiplyMemoryCase,
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
fn native_widening_multiply_matches_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX VPMULUDQ/VPMULDQ memory differential: host lacks AVX");
        return;
    }

    let avx2 = std::is_x86_feature_detected!("avx2");
    let cases = all_cases()
        .into_iter()
        .filter(|case| avx2 || case.width == VecWidth::V128)
        .collect::<Vec<_>>();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let (_, source2) = operand_vectors(case);

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
            let mut expected = expected_success(registers, case, source2);

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
                &function, &initial, &expected, source2, address, case, level,
            );
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

    assert!(expected_executions > 0);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX \
         VPMULUDQ/VPMULDQ memory cases"
    );
}
