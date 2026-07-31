//! Exact helper-backed EVEX VPERMI2*/VPERMT2* memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, SourceArch, VReg, VecElementType,
    VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexTwoTablePermuteMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexTwoTablePermuteMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_two_table_permute_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceForm {
    Vector,
    Broadcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (3, false),
            Self::Zero => (5, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PermuteMemoryCase {
    elem: VecElementType,
    width: VecWidth,
    destination: u8,
    source1: u8,
    form: SourceForm,
    control: MaskControl,
    overwrite_table: bool,
}

impl PermuteMemoryCase {
    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    fn opcode_w(self) -> (u8, bool) {
        match (self.elem, self.overwrite_table) {
            (VecElementType::I8, false) => (0x75, false),
            (VecElementType::I16, false) => (0x75, true),
            (VecElementType::I32, false) => (0x76, false),
            (VecElementType::I64, false) => (0x76, true),
            (VecElementType::F32, false) => (0x77, false),
            (VecElementType::F64, false) => (0x77, true),
            (VecElementType::I8, true) => (0x7D, false),
            (VecElementType::I16, true) => (0x7D, true),
            (VecElementType::I32, true) => (0x7E, false),
            (VecElementType::I64, true) => (0x7E, true),
            (VecElementType::F32, true) => (0x7F, false),
            (VecElementType::F64, true) => (0x7F, true),
            _ => unreachable!("two-table-permute element"),
        }
    }

    fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!("EVEX vector width"),
        }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 32 && self.source1 < 32);
        assert!(
            self.form == SourceForm::Vector
                || !matches!(self.elem, VecElementType::I8 | VecElementType::I16)
        );
        let (opcode, w) = self.opcode_w();
        vec![
            0x62,
            0x62 | (u8::from(self.destination & 8 == 0) << 7)
                | (u8::from(self.destination & 16 == 0) << 4),
            (u8::from(w) << 7) | (((!self.source1) & 0x0F) << 3) | 0x05,
            (u8::from(self.zeroing()) << 7)
                | (self.ll() << 5)
                | (u8::from(self.form == SourceForm::Broadcast) << 4)
                | (u8::from(self.source1 < 16) << 3)
                | self.mask(),
            opcode,
            ((self.destination & 7) << 3) | 3,
        ]
    }
}

const ELEMENTS: [VecElementType; 6] = [
    VecElementType::I8,
    VecElementType::I16,
    VecElementType::I32,
    VecElementType::I64,
    VecElementType::F32,
    VecElementType::F64,
];

fn forms(elem: VecElementType) -> &'static [SourceForm] {
    if matches!(elem, VecElementType::I8 | VecElementType::I16) {
        &[SourceForm::Vector]
    } else {
        &[SourceForm::Vector, SourceForm::Broadcast]
    }
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
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
        X86InstructionBytes::new(bytes).expect("EVEX two-table-permute provenance"),
    );
    function
}

fn lift_case(case: PermuteMemoryCase) -> SmirFunction {
    lift_bytes(&case.bytes())
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. } | OpKind::Load { .. }))
        .expect("EVEX two-table-permute memory operation")
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
) -> Option<X86JitEvexTwoTablePermuteMemorySequence> {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_two_table_permute_memory_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn replay_bytes(sequence: X86JitEvexTwoTablePermuteMemorySequence) -> X86InstructionBytes {
    match sequence.encoding.replay {
        X86EvexTwoTablePermuteMemoryReplay::Vector {
            register_instruction,
            ..
        } => register_instruction,
        X86EvexTwoTablePermuteMemoryReplay::Broadcast {
            stack_instruction, ..
        } => stack_instruction,
    }
}

fn lower(function: &SmirFunction, case: PermuteMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert_eq!(
        requirements.needs_avx512vbmi,
        case.elem == VecElementType::I8,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512vbmi2, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
            && (case.elem != VecElementType::I8 || std::is_x86_feature_detected!("avx512vbmi")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: two-table-permute lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed EVEX two-table permute"),
        result.entry_offset,
    )
}

#[test]
fn all_540_family_scanner_cells_lift_optimize_admit_and_lower_exactly() {
    let mut dword_qword_float_cells = 0usize;
    let mut byte_word_cells = 0usize;
    for elem in ELEMENTS {
        for overwrite_table in [false, true] {
            for &form in forms(elem) {
                for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                    for source1 in [0u8, 1, 15] {
                        for control in MaskControl::ALL {
                            let case = PermuteMemoryCase {
                                elem,
                                width,
                                destination: 0,
                                source1,
                                form,
                                control,
                                overwrite_table,
                            };
                            let bytes = case.bytes();
                            let function = optimize(lift_bytes(&bytes), OptLevel::O2);
                            let matched = sequence(&function, true)
                                .unwrap_or_else(|| panic!("{bytes:02X?}: missing exact sequence"));
                            assert_eq!(matched.encoding.elem, elem, "{bytes:02X?}");
                            assert_eq!(matched.encoding.width, width, "{bytes:02X?}");
                            assert_eq!(matched.encoding.source1, source1, "{bytes:02X?}");
                            assert_eq!(
                                matched.encoding.overwrite_table, overwrite_table,
                                "{bytes:02X?}"
                            );
                            assert_eq!(
                                matched.memory_size,
                                if form == SourceForm::Broadcast {
                                    elem.bytes()
                                } else {
                                    width.bytes()
                                },
                                "{bytes:02X?}"
                            );
                            let replay = replay_bytes(matched);
                            let (code, _) = lower(&function, case);
                            assert!(
                                code.windows(replay.as_slice().len())
                                    .any(|window| window == replay.as_slice()),
                                "{bytes:02X?}"
                            );
                            if matches!(elem, VecElementType::I8 | VecElementType::I16) {
                                byte_word_cells += 1;
                            } else {
                                dword_qword_float_cells += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(dword_qword_float_cells, 432);
    assert_eq!(byte_word_cells, 108);
    assert_eq!(dword_qword_float_cells + byte_word_cells, 540);
}

fn representative_cases() -> [PermuteMemoryCase; 12] {
    [
        PermuteMemoryCase {
            elem: VecElementType::I8,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            form: SourceForm::Vector,
            control: MaskControl::None,
            overwrite_table: false,
        },
        PermuteMemoryCase {
            elem: VecElementType::I16,
            width: VecWidth::V256,
            destination: 9,
            source1: 9,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            overwrite_table: true,
        },
        PermuteMemoryCase {
            elem: VecElementType::I32,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            overwrite_table: false,
        },
        PermuteMemoryCase {
            elem: VecElementType::I64,
            width: VecWidth::V128,
            destination: 31,
            source1: 31,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
            overwrite_table: true,
        },
        PermuteMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V256,
            destination: 25,
            source1: 26,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
            overwrite_table: false,
        },
        PermuteMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V512,
            destination: 0,
            source1: 0,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            overwrite_table: true,
        },
        PermuteMemoryCase {
            elem: VecElementType::I8,
            width: VecWidth::V512,
            destination: 25,
            source1: 26,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            overwrite_table: true,
        },
        PermuteMemoryCase {
            elem: VecElementType::I16,
            width: VecWidth::V128,
            destination: 31,
            source1: 30,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            overwrite_table: false,
        },
        PermuteMemoryCase {
            elem: VecElementType::I32,
            width: VecWidth::V256,
            destination: 9,
            source1: 14,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
            overwrite_table: true,
        },
        PermuteMemoryCase {
            elem: VecElementType::I64,
            width: VecWidth::V512,
            destination: 17,
            source1: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            overwrite_table: false,
        },
        PermuteMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            destination: 1,
            source1: 2,
            form: SourceForm::Vector,
            control: MaskControl::Zero,
            overwrite_table: true,
        },
        PermuteMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V256,
            destination: 25,
            source1: 26,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
            overwrite_table: false,
        },
    ]
}

#[test]
fn all_mnemonics_aliases_masks_tuple_shapes_and_optimization_profiles_admit() {
    let mut admitted = 0usize;
    for case in representative_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let matched = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: missing exact sequence"));
            assert_eq!(matched.encoding.elem, case.elem);
            assert_eq!(matched.encoding.width, case.width);
            assert_eq!(matched.encoding.destination, case.destination);
            assert_eq!(matched.encoding.source1, case.source1);
            assert_eq!(
                matched.encoding.writemask,
                (case.mask() != 0).then_some(case.mask())
            );
            assert_eq!(matched.encoding.zeroing, case.zeroing());
            assert_eq!(matched.encoding.overwrite_table, case.overwrite_table);
            assert_eq!(
                matches!(
                    matched.encoding.replay,
                    X86EvexTwoTablePermuteMemoryReplay::Broadcast { .. }
                ),
                case.form == SourceForm::Broadcast
            );
            let replay = replay_bytes(matched);
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(replay.as_slice().len())
                    .any(|window| window == replay.as_slice())
            );
            admitted += 1;
        }
    }
    assert_eq!(admitted, representative_cases().len() * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact matcher admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted malformed sequence"
    );
}

#[test]
fn matcher_fails_closed_for_provenance_graph_fault_mask_and_boundary_mutations() {
    let case = PermuteMemoryCase {
        elem: VecElementType::I32,
        width: VecWidth::V512,
        destination: 25,
        source1: 26,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        overwrite_table: true,
    };
    let base = optimize(lift_case(case), OptLevel::O0);
    let index = sequence_index(&base);

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_provenance = base.clone();
    let mut wrong_bytes = case.bytes();
    wrong_bytes[4] = 0x76;
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&wrong_bytes).unwrap(),
    );

    let mut unaligned_hint = base.clone();
    unaligned_hint.blocks[0].ops[index].x86_hint =
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

    let mut virtual_address = base.clone();
    match &mut virtual_address.blocks[0].ops[index].kind {
        OpKind::VLoad { addr, .. } => {
            *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
        }
        _ => unreachable!("vector tuple starts with VLoad"),
    }

    let mut wrong_table = base.clone();
    let permute = wrong_table.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VPermute { .. }))
        .unwrap();
    match &mut permute.kind {
        OpKind::VPermute { src1, .. } => {
            *src1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(24)));
        }
        _ => unreachable!(),
    }

    let mut wrong_direction = base.clone();
    let permute = wrong_direction.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::VPermute { .. }))
        .unwrap();
    match &mut permute.kind {
        OpKind::VPermute {
            overwrite_table, ..
        } => *overwrite_table = false,
        _ => unreachable!(),
    }

    let mut child_hint = base.clone();
    child_hint.blocks[0].ops[index + 1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));

    let mut child_pc = base.clone();
    child_pc.blocks[0].ops[index + 1].guest_pc += 1;

    let mut wrong_mask = base.clone();
    let shift = wrong_mask.blocks[0]
        .ops
        .iter_mut()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::Shr {
                    amount: crate::smir::ir::types::SrcOperand::Imm(1),
                    ..
                }
            )
        })
        .expect("mask lane-one shift");
    match &mut shift.kind {
        OpKind::Shr {
            amount: crate::smir::ir::types::SrcOperand::Imm(amount),
            ..
        } => *amount = 2,
        _ => unreachable!(),
    }

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7F00), PC, OpKind::Nop));

    let loaded = match base.blocks[0].ops[index].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut external_use = base.clone();
    external_use.blocks[0].ops.push(SmirOp::new(
        OpId(0x7F01),
        PC + 1,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
            src: loaded,
            width: VecWidth::V512,
        },
    ));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("opcode provenance differs", wrong_provenance),
        ("noncanonical unaligned hint", unaligned_hint),
        ("address contains virtual register", virtual_address),
        ("table source differs", wrong_table),
        ("overwrite direction differs", wrong_direction),
        ("semantic child has hint", child_hint),
        ("semantic child PC differs", child_pc),
        ("mask predicate differs", wrong_mask),
        ("same-PC operation follows sequence", same_pc_tail),
        ("loaded temporary escapes sequence", external_use),
    ] {
        assert_rejected(name, &function);
    }
    assert!(sequence(&base, false).is_none());
}

#[test]
fn segment_addr32_rip_compressed_tuple_and_apx_b4_x4_addresses_remain_exact() {
    let case = PermuteMemoryCase {
        elem: VecElementType::I64,
        width: VecWidth::V512,
        destination: 9,
        source1: 14,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
        overwrite_table: true,
    };
    let vector = case.bytes();
    let mut rip = vector.clone();
    rip[5] = (rip[5] & 0x38) | 0x05;
    rip.splice(6..6, 0x20i32.to_le_bytes());
    let mut addr32 = vector.clone();
    addr32.insert(0, 0x67);
    let mut fs = vector.clone();
    fs.insert(0, 0x64);
    let mut gs_addr32 = vector.clone();
    gs_addr32[5] = (gs_addr32[5] & 0x38) | 0x44;
    gs_addr32.splice(6..6, [0x8B, 0x02]);
    gs_addr32.insert(0, 0x67);
    gs_addr32.insert(0, 0x65);

    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let address_cases = [
        (
            "RIP+disp32",
            rip.clone(),
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + rip.len() as u64),
            },
        ),
        (
            "addr32 base",
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rbx)))),
        ),
        (
            "FS Full Mem",
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
        (
            "GS addr32 SIB compressed Full Mem",
            gs_addr32,
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rbx)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 4,
                disp: 128,
            })),
        ),
    ];
    for (name, bytes, expected_address) in address_cases {
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::VLoad { addr, .. } => addr == &expected_address,
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true).unwrap_or_else(|| panic!("{name} {level:?}"));
        }
    }

    let mut apx = case.bytes();
    apx[5] = (apx[5] & 0x38) | 0x04;
    apx.push(0x48); // [r16+r17*2] after APX extensions
    apx[1] |= 0x08; // EVEX.B4
    apx[2] &= !0x04; // EVEX.X4 / !U
    let expected = Address::BaseIndexScale {
        base: Some(x86(X86Reg::R16)),
        index: x86(X86Reg::R17),
        scale: 2,
        disp: 0,
        disp_size: DispSize::Auto,
    };
    for level in LEVELS {
        let function = optimize(lift_bytes(&apx), level);
        assert!(matches!(
            function.blocks[0].ops.first().map(|op| &op.kind),
            Some(OpKind::X86RequireApx)
        ));
        assert!(
            function.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(&op.kind, OpKind::VLoad { addr, .. } if addr == &expected))
        );
        sequence(&function, true).unwrap_or_else(|| panic!("APX {level:?}"));
    }
}

#[test]
fn avx_only_vector_bridge_is_rejected() {
    let case = representative_cases()[2];
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("AVX-only bridge must reject EVEX two-table permute");
    assert!(
        format!("{error:?}").contains("AVX-only vector bridge"),
        "{error:?}"
    );
}
