//! Exact helper-backed EVEX VPMULTISHIFTQB memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SignExtend, SourceArch,
    SrcOperand, VReg, VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexMultiShiftMemoryReplay, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86JitEvexMultiShiftMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_multishift_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classifier;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x7E83;
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
            Self::Zero => (1, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MultiShiftMemoryCase {
    width: VecWidth,
    destination: u8,
    control_register: u8,
    form: SourceForm,
    mask_control: MaskControl,
}

impl MultiShiftMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    const fn mask(self) -> u8 {
        self.mask_control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.mask_control.fields().1
    }

    const fn broadcast(self) -> bool {
        matches!(self.form, SourceForm::Broadcast)
    }

    fn bytes(self) -> Vec<u8> {
        memory_encoding_with_controls(self, false, self.mask(), self.zeroing())
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|candidate| *candidate != self.destination && *candidate != self.control_register)
            .expect("two operands leave a low vector scratch")
    }

    fn expected_replay(self) -> Vec<u8> {
        match self.form {
            SourceForm::Vector => register_encoding(self, self.scratch()),
            SourceForm::Broadcast => stack_encoding(self),
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("VPMULTISHIFTQB vector width"),
    }))
}

fn memory_encoding_with_controls(
    case: MultiShiftMemoryCase,
    sib: bool,
    mask: u8,
    zeroing: bool,
) -> Vec<u8> {
    assert!(case.destination < 32 && case.control_register < 32);
    assert!(mask < 8 && (!zeroing || mask != 0));
    let p0 = 0x62
        | (u8::from(case.destination & 8 == 0) << 7)
        | (u8::from(case.destination & 16 == 0) << 4);
    let p1 = 0x85 | (((!case.control_register) & 0x0F) << 3);
    let p2 = (u8::from(zeroing) << 7)
        | (case.ll() << 5)
        | (u8::from(case.broadcast()) << 4)
        | (u8::from(case.control_register < 16) << 3)
        | mask;
    let mut bytes = vec![
        0x62,
        p0,
        p1,
        p2,
        0x83,
        ((case.destination & 7) << 3) | if sib { 4 } else { 2 },
    ];
    if sib {
        // [RAX + RCX*2]; APX B4/X4 are injected independently by tests.
        bytes.push(0x48);
    }
    bytes
}

fn register_encoding(case: MultiShiftMemoryCase, scratch: u8) -> Vec<u8> {
    assert!(scratch < 16);
    let p0 = 0x42
        | (u8::from(case.destination & 8 == 0) << 7)
        | (u8::from(case.destination & 16 == 0) << 4)
        | (u8::from(scratch & 8 == 0) << 5);
    let p1 = 0x85 | (((!case.control_register) & 0x0F) << 3);
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | (u8::from(case.control_register < 16) << 3)
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x83,
        0xC0 | ((case.destination & 7) << 3) | scratch,
    ]
}

fn stack_encoding(case: MultiShiftMemoryCase) -> Vec<u8> {
    let p0 = 0x62
        | (u8::from(case.destination & 8 == 0) << 7)
        | (u8::from(case.destination & 16 == 0) << 4);
    let p1 = 0x85 | (((!case.control_register) & 0x0F) << 3);
    let p2 = (u8::from(case.zeroing()) << 7)
        | (case.ll() << 5)
        | 0x10
        | (u8::from(case.control_register < 16) << 3)
        | case.mask();
    vec![
        0x62,
        p0,
        p1,
        p2,
        0x83,
        ((case.destination & 7) << 3) | 4,
        0x24,
    ]
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
        X86InstructionBytes::new(bytes).expect("VPMULTISHIFTQB provenance"),
    );
    function
}

fn lift_case(case: MultiShiftMemoryCase) -> SmirFunction {
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
        .position(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
        .expect("VPMULTISHIFTQB memory operation")
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
) -> Option<X86JitEvexMultiShiftMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_multishift_memory_sequence(
        &function.blocks[0],
        sequence_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: MultiShiftMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(requirements.needs_avx512vbmi, "{case:?}");
    assert_eq!(
        requirements.needs_avx512vl,
        case.width != VecWidth::V512,
        "{case:?}"
    );
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512vbmi2, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512vbmi")
            && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: VPMULTISHIFTQB lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VPMULTISHIFTQB"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<MultiShiftMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for (destination, control_register) in [(0, 0), (9, 10), (17, 18)] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for mask_control in MaskControl::ALL {
                    cases.push(MultiShiftMemoryCase {
                        width,
                        destination,
                        control_register,
                        form,
                        mask_control,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn all_54_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
            assert_eq!(exact.encoding.width, case.width, "{case:?}");
            assert_eq!(exact.encoding.destination, case.destination, "{case:?}");
            assert_eq!(exact.encoding.control, case.control_register, "{case:?}");
            assert_eq!(
                exact.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{case:?}"
            );
            assert_eq!(exact.encoding.zeroing, case.zeroing(), "{case:?}");
            assert_eq!(
                exact.encoding.needs_avx512vl,
                case.width != VecWidth::V512,
                "{case:?}"
            );
            assert_eq!(
                exact.memory_size,
                if case.broadcast() {
                    8
                } else {
                    case.width.bytes()
                },
                "{case:?}"
            );
            assert_eq!(
                matches!(
                    exact.encoding.replay,
                    X86EvexMultiShiftMemoryReplay::Broadcast { .. }
                ),
                case.broadcast(),
                "{case:?}"
            );
            assert_eq!(
                exact.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );

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
    assert_eq!(lowerings, 54 * LEVELS.len());
}

#[test]
fn type_e4nf_graphs_keep_one_unconditional_tuple_read_at_all_levels() {
    for case in all_cases() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let vector_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VLoad { .. }))
                .count();
            let scalar_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::Load { .. }))
                .count();
            let broadcasts = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::VBroadcast { .. }))
                .count();
            let predicated_loads = function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count();
            assert_eq!(
                (vector_loads, scalar_loads, broadcasts, predicated_loads),
                match case.form {
                    SourceForm::Vector => (1, 0, 0, 0),
                    SourceForm::Broadcast => (0, 1, 1, 0),
                },
                "{level:?} {case:?}: {:#?}",
                function.blocks[0].ops
            );
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: exact matcher admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed sequence"
    );
}

#[test]
fn sequence_fails_closed_for_provenance_graph_virtual_and_boundary_mutations() {
    for case in [
        MultiShiftMemoryCase {
            width: VecWidth::V256,
            destination: 9,
            control_register: 10,
            form: SourceForm::Vector,
            mask_control: MaskControl::Merge,
        },
        MultiShiftMemoryCase {
            width: VecWidth::V512,
            destination: 17,
            control_register: 18,
            form: SourceForm::Broadcast,
            mask_control: MaskControl::Zero,
        },
    ] {
        let function = optimize(lift_case(case), OptLevel::O2);
        assert!(sequence(&function, true).is_some(), "{case:?}");
        assert!(sequence(&function, false).is_none(), "{case:?}");

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[4] ^= 1;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong opcode provenance", &wrong_provenance);

        let mut virtual_address = function.clone();
        let index = sequence_index(&virtual_address);
        match &mut virtual_address.blocks[0].ops[index].kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                *addr = Address::Direct(VReg::Virtual(VirtualId(0x7FFF)));
            }
            _ => unreachable!("VPMULTISHIFTQB tuple load"),
        }
        assert_rejected("virtual address", &virtual_address);

        let mut hinted_load = function.clone();
        let index = sequence_index(&hinted_load);
        hinted_load.blocks[0].ops[index].x86_hint =
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
        assert_rejected("hinted tuple load", &hinted_load);

        let mut wrong_load_shape = function.clone();
        let index = sequence_index(&wrong_load_shape);
        match &mut wrong_load_shape.blocks[0].ops[index].kind {
            OpKind::VLoad { width, .. } => {
                *width = if case.width == VecWidth::V128 {
                    VecWidth::V256
                } else {
                    VecWidth::V128
                };
            }
            OpKind::Load { width, sign, .. } => {
                *width = MemWidth::B4;
                *sign = SignExtend::Sign;
            }
            _ => unreachable!(),
        }
        assert_rejected("wrong tuple load shape", &wrong_load_shape);

        let mut wrong_destination = function.clone();
        let terminal = wrong_destination.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
            .unwrap();
        match &mut terminal.kind {
            OpKind::X86MultiShiftQB { dst, .. } => *dst = vector(31, case.width),
            _ => unreachable!(),
        }
        assert_rejected("wrong destination register", &wrong_destination);

        let mut wrong_terminal = function.clone();
        let terminal = wrong_terminal.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
            .unwrap();
        match &mut terminal.kind {
            OpKind::X86MultiShiftQB { control, .. } => {
                *control = vector(31, case.width);
            }
            _ => unreachable!(),
        }
        assert_rejected("wrong control register", &wrong_terminal);

        let mut wrong_source = function.clone();
        let terminal = wrong_source.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
            .unwrap();
        match &mut terminal.kind {
            OpKind::X86MultiShiftQB { source, .. } => {
                *source = vector(30, case.width);
            }
            _ => unreachable!(),
        }
        assert_rejected("wrong memory source", &wrong_source);

        let mut wrong_mask = function.clone();
        let terminal = wrong_mask.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
            .unwrap();
        match &mut terminal.kind {
            OpKind::X86MultiShiftQB { mask, .. } => *mask = None,
            _ => unreachable!(),
        }
        assert_rejected("wrong writemask", &wrong_mask);

        let mut wrong_zeroing = function.clone();
        let terminal = wrong_zeroing.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
            .unwrap();
        match &mut terminal.kind {
            OpKind::X86MultiShiftQB { zeroing, .. } => *zeroing = !*zeroing,
            _ => unreachable!(),
        }
        assert_rejected("wrong zeroing control", &wrong_zeroing);

        let mut wrong_terminal_pc = function.clone();
        wrong_terminal_pc.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
            .unwrap()
            .guest_pc += 1;
        assert_rejected("wrong terminal guest PC", &wrong_terminal_pc);

        if case.form == SourceForm::Broadcast {
            let mut wrong_broadcast = function.clone();
            let broadcast = wrong_broadcast.blocks[0]
                .ops
                .iter_mut()
                .find(|op| matches!(op.kind, OpKind::VBroadcast { .. }))
                .unwrap();
            match &mut broadcast.kind {
                OpKind::VBroadcast { lanes, .. } => *lanes ^= 1,
                _ => unreachable!(),
            }
            assert_rejected("wrong broadcast lanes", &wrong_broadcast);

            let mut broadcast_extra_use = function.clone();
            let broadcast_value =
                match broadcast_extra_use.blocks[0]
                    .ops
                    .iter()
                    .find_map(|op| match op.kind {
                        OpKind::VBroadcast { dst, .. } => Some(dst),
                        _ => None,
                    }) {
                    Some(value) => value,
                    None => unreachable!(),
                };
            broadcast_extra_use.blocks[0].ops.push(SmirOp::new(
                OpId(0xFFFD),
                PC + 1,
                OpKind::Mov {
                    dst: VReg::Virtual(VirtualId(0xFFFD)),
                    src: SrcOperand::Reg(broadcast_value),
                    width: crate::smir::ir::types::OpWidth::W64,
                },
            ));
            assert_rejected("broadcast temporary has an extra use", &broadcast_extra_use);
        }

        let mut extra_use = function.clone();
        let loaded = match extra_use.blocks[0].ops[sequence_index(&extra_use)].kind {
            OpKind::Load { dst, .. } | OpKind::VLoad { dst, .. } => dst,
            _ => unreachable!(),
        };
        extra_use.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFE),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFE)),
                src: SrcOperand::Reg(loaded),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("tuple temporary has an extra use", &extra_use);

        let mut same_pc_tail = function;
        same_pc_tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFF),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFF)),
                src: SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("same-PC tail", &same_pc_tail);
    }
}

#[test]
fn segment_addr32_rip_and_apx_b4_x4_addresses_admit_and_lower() {
    let x86 = |register| VReg::Arch(ArchReg::X86(register));
    let vector_case = MultiShiftMemoryCase {
        width: VecWidth::V128,
        destination: 1,
        control_register: 2,
        form: SourceForm::Vector,
        mask_control: MaskControl::None,
    };
    let broadcast_case = MultiShiftMemoryCase {
        width: VecWidth::V256,
        destination: 9,
        control_register: 10,
        form: SourceForm::Broadcast,
        mask_control: MaskControl::Merge,
    };

    let mut rip = vector_case.bytes();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    let mut addr32 = vector_case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = broadcast_case.bytes();
    fs.insert(0, 0x64);
    for (name, case, bytes, expected_address) in [
        (
            "RIP-relative vector tuple",
            vector_case,
            rip,
            Address::PcRel {
                offset: 0x20,
                disp_size: DispSize::Disp32,
                base: Some(PC + 10),
            },
        ),
        (
            "addr32 vector tuple",
            vector_case,
            addr32,
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rdx)))),
        ),
        (
            "FS broadcast tuple",
            broadcast_case,
            fs,
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rdx)),
                index: None,
                scale: 1,
                disp: 0,
            },
        ),
    ] {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            assert!(
                function.blocks[0].ops.iter().any(|op| match &op.kind {
                    OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                        addr == &expected_address
                    }
                    _ => false,
                }),
                "{name} {level:?}: {:#?}",
                function.blocks[0].ops
            );
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    for case in [vector_case, broadcast_case] {
        for (base_high, index_high, expected_base, expected_index) in [
            (true, false, X86Reg::R16, X86Reg::Rcx),
            (false, true, X86Reg::Rax, X86Reg::R17),
            (true, true, X86Reg::R16, X86Reg::R17),
        ] {
            let mut bytes = memory_encoding_with_controls(case, true, case.mask(), case.zeroing());
            if base_high {
                bytes[1] |= 0x08;
            }
            if index_high {
                bytes[2] &= !0x04;
            }
            let expected_address = Address::BaseIndexScale {
                base: Some(x86(expected_base)),
                index: x86(expected_index),
                scale: 2,
                disp: 0,
                disp_size: DispSize::Auto,
            };
            let base = lift_bytes(&bytes);
            for level in LEVELS {
                let function = optimize(base.clone(), level);
                assert!(
                    matches!(
                        function.blocks[0].ops.first().map(|op| &op.kind),
                        Some(OpKind::X86RequireApx)
                    ),
                    "{level:?} {bytes:02X?}: APX address lost its dynamic guard"
                );
                assert!(
                    function.blocks[0].ops.iter().any(|op| match &op.kind {
                        OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                            addr == &expected_address
                        }
                        _ => false,
                    }),
                    "{level:?} {bytes:02X?}: {:#?}",
                    function.blocks[0].ops
                );
                sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {bytes:02X?}"));
                lower(&function, case);
            }
        }
    }
}

#[test]
fn avx_only_state_bridge_is_rejected() {
    let case = MultiShiftMemoryCase {
        width: VecWidth::V512,
        destination: 17,
        control_register: 18,
        form: SourceForm::Vector,
        mask_control: MaskControl::Merge,
    };
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(
        lowerer.lower_function(&function).is_err(),
        "AVX-only bridge admitted AVX-512 VBMI state"
    );
}
