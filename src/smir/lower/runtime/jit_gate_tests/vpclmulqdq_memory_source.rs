//! Helper-backed VEX/EVEX VPCLMULQDQ memory-source coverage.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vpclmulqdq_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xC4D0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug)]
struct VpclmulqdqMemoryCase {
    name: &'static str,
    bytes: &'static [u8],
    width: VecWidth,
    destination: u8,
    source1: u8,
    scratch: u8,
    emitted: &'static [u8],
    supports_avx_ymm16: bool,
    needs_pclmulqdq: bool,
    needs_vpclmulqdq: bool,
    needs_avx512vl: bool,
}

const CASES: [VpclmulqdqMemoryCase; 7] = [
    VpclmulqdqMemoryCase {
        name: "VEX.128 low registers",
        bytes: &[0xC4, 0xE3, 0x71, 0x44, 0x43, 0x20, 0xA5],
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        scratch: 2,
        emitted: &[0xC4, 0xE3, 0x71, 0x44, 0xC2, 0xA5],
        supports_avx_ymm16: true,
        needs_pclmulqdq: true,
        needs_vpclmulqdq: false,
        needs_avx512vl: false,
    },
    VpclmulqdqMemoryCase {
        name: "VEX.256 high registers",
        bytes: &[0xC4, 0x43, 0x25, 0x44, 0x4B, 0x20, 0x11],
        width: VecWidth::V256,
        destination: 9,
        source1: 11,
        scratch: 0,
        emitted: &[0xC4, 0x63, 0x25, 0x44, 0xC8, 0x11],
        supports_avx_ymm16: true,
        needs_pclmulqdq: false,
        needs_vpclmulqdq: true,
        needs_avx512vl: false,
    },
    VpclmulqdqMemoryCase {
        name: "EVEX.128 low registers",
        bytes: &[0x62, 0xF3, 0x75, 0x08, 0x44, 0x43, 0x02, 0xEF],
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        scratch: 2,
        emitted: &[0x62, 0xF3, 0x75, 0x08, 0x44, 0xC2, 0xEF],
        supports_avx_ymm16: false,
        needs_pclmulqdq: false,
        needs_vpclmulqdq: true,
        needs_avx512vl: true,
    },
    VpclmulqdqMemoryCase {
        name: "EVEX.256 high registers",
        bytes: &[0x62, 0x43, 0x35, 0x20, 0x44, 0x43, 0x02, 0x10],
        width: VecWidth::V256,
        destination: 24,
        source1: 25,
        scratch: 0,
        emitted: &[0x62, 0x63, 0x35, 0x20, 0x44, 0xC0, 0x10],
        supports_avx_ymm16: false,
        needs_pclmulqdq: false,
        needs_vpclmulqdq: true,
        needs_avx512vl: true,
    },
    VpclmulqdqMemoryCase {
        name: "EVEX.512 high registers",
        bytes: &[0x62, 0x43, 0x0D, 0x40, 0x44, 0x7B, 0x01, 0x01],
        width: VecWidth::V512,
        destination: 31,
        source1: 30,
        scratch: 0,
        emitted: &[0x62, 0x63, 0x0D, 0x40, 0x44, 0xF8, 0x01],
        supports_avx_ymm16: false,
        needs_pclmulqdq: false,
        needs_vpclmulqdq: true,
        needs_avx512vl: false,
    },
    VpclmulqdqMemoryCase {
        name: "EVEX APX r16 memory base",
        bytes: &[0x62, 0xFB, 0x75, 0x08, 0x44, 0x00, 0x00],
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        scratch: 2,
        emitted: &[0x62, 0xF3, 0x75, 0x08, 0x44, 0xC2, 0x00],
        supports_avx_ymm16: false,
        needs_pclmulqdq: false,
        needs_vpclmulqdq: true,
        needs_avx512vl: true,
    },
    VpclmulqdqMemoryCase {
        name: "VEX WIG addr32 FS SIB memory",
        bytes: &[0x64, 0x67, 0xC4, 0xE3, 0xF1, 0x44, 0x44, 0x73, 0x20, 0xAA],
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        scratch: 2,
        emitted: &[0xC4, 0xE3, 0xF1, 0x44, 0xC2, 0xAA],
        supports_avx_ymm16: true,
        needs_pclmulqdq: true,
        needs_vpclmulqdq: false,
        needs_avx512vl: false,
    },
];

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("VPCLMULQDQ vector width"),
    }))
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
        X86InstructionBytes::new(bytes).expect("x86 instruction provenance"),
    );
    function
}

fn sequence_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| {
            matches!(op.kind, OpKind::VLoad { .. })
                && op.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
        })
        .expect("VPCLMULQDQ memory load")
}

fn raw_load_index(function: &SmirFunction) -> usize {
    function.blocks[0]
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .expect("VPCLMULQDQ memory load")
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

fn lower(function: &SmirFunction, case: VpclmulqdqMemoryCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert_eq!(
        x86_native_vector_uses_avx_ymm16_only_excluding(function, &excluded),
        case.supports_avx_ymm16,
        "{}",
        case.name
    );

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{}", case.name);
    assert_eq!(
        requirements.all_spans_support_avx_ymm16, case.supports_avx_ymm16,
        "{}",
        case.name
    );
    assert!(requirements.needs_avx, "{}", case.name);
    assert_eq!(
        requirements.needs_pclmulqdq, case.needs_pclmulqdq,
        "{}",
        case.name
    );
    assert_eq!(
        requirements.needs_vpclmulqdq, case.needs_vpclmulqdq,
        "{}",
        case.name
    );
    assert_eq!(
        requirements.needs_avx512bw, !case.supports_avx_ymm16,
        "{}",
        case.name
    );
    assert_eq!(
        requirements.needs_avx512vl, case.needs_avx512vl,
        "{}",
        case.name
    );
    assert!(!requirements.needs_aes, "{}", case.name);
    assert!(!requirements.needs_vaes, "{}", case.name);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(case.supports_avx_ymm16);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
    assert!(result.relocations.is_empty(), "{}", case.name);
    (
        lowerer
            .finalize()
            .expect("finalize VPCLMULQDQ memory source"),
        result.entry_offset,
    )
}

#[test]
fn exact_vpclmulqdq_memory_sequences_cross_gate_and_lower_at_every_opt_level() {
    for case in CASES {
        for level in LEVELS {
            let mut function = lift_bytes(case.bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            let index = sequence_index(&function);
            let (definitions, uses) = virtual_counts(&function);
            let sequence = x86_jit_vpclmulqdq_memory_sequence(
                &function.blocks[0],
                index,
                true,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .unwrap_or_else(|| panic!("{level:?} {}: classifier rejected", case.name));
            assert_eq!(
                sequence.consumed,
                4 + 5 * (case.width.bytes() / 16) as usize,
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                sequence.memory_size,
                case.width.bytes(),
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                sequence.encoding.destination, case.destination,
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                sequence.encoding.source1, case.source1,
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                sequence.encoding.scratch, case.scratch,
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.emitted,
                "{level:?} {}",
                case.name
            );
            assert!(
                is_native_clobber_safe_excluding(&function, &HashMap::new(), true),
                "{level:?} {}",
                case.name
            );
            assert!(
                !is_native_clobber_safe_excluding(&function, &HashMap::new(), false),
                "{level:?} {}",
                case.name
            );
            assert!(
                !is_x86_aarch64_native_clobber_safe_excluding(&function, &HashMap::new()),
                "{level:?} {}",
                case.name
            );

            let (code, _) = lower(&function, case);
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{level:?} {}: missing reserved helper destination",
                case.name
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {}: missing vector scratch",
                case.name
            );
            assert!(
                code.windows(case.emitted.len())
                    .any(|window| window == case.emitted),
                "{level:?} {}: missing {:02X?}",
                case.name,
                case.emitted
            );
        }
    }
}

#[test]
fn memory_sequence_classifier_covers_all_1_280_immediate_and_width_combinations() {
    let templates: [(&[u8], VecWidth); 5] = [
        (&[0xC4, 0xE3, 0x71, 0x44, 0x00, 0x00], VecWidth::V128),
        (&[0xC4, 0xE3, 0x75, 0x44, 0x00, 0x00], VecWidth::V256),
        (&[0x62, 0xF3, 0x75, 0x08, 0x44, 0x00, 0x00], VecWidth::V128),
        (&[0x62, 0xF3, 0x75, 0x28, 0x44, 0x00, 0x00], VecWidth::V256),
        (&[0x62, 0xF3, 0x75, 0x48, 0x44, 0x00, 0x00], VecWidth::V512),
    ];
    let mut classified = 0usize;
    for (template, width) in templates {
        for immediate in u8::MIN..=u8::MAX {
            let mut bytes = template.to_vec();
            *bytes.last_mut().unwrap() = immediate;
            let function = lift_bytes(&bytes);
            let index = sequence_index(&function);
            let (definitions, uses) = virtual_counts(&function);
            let sequence = x86_jit_vpclmulqdq_memory_sequence(
                &function.blocks[0],
                index,
                true,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .unwrap_or_else(|| panic!("{width:?} {immediate:#04x}"));
            assert_eq!(sequence.encoding.width, width);
            assert_eq!(sequence.encoding.immediate, immediate);
            classified += 1;
        }
    }
    assert_eq!(classified, 5 * 256);
}

#[test]
fn evex_memory_sequence_rejects_the_avx_only_vector_bridge() {
    let function = lift_bytes(CASES[2].bytes);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let error = lowerer
        .lower_function(&function)
        .expect_err("EVEX VPCLMULQDQ must reject the AVX-only state bridge");
    assert!(
        matches!(
            error,
            crate::smir::lower::LowerError::InvalidOperand { ref op, .. }
                if op == "VPCLMULQDQ memory source"
        ),
        "{error:?}"
    );
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let index = raw_load_index(function);
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_vpclmulqdq_memory_sequence(
            &function.blocks[0],
            index,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none(),
        "{name}: classifier admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: native gate admitted malformed sequence"
    );
}

#[test]
fn vpclmulqdq_memory_sequence_fails_closed_for_every_structural_invariant() {
    let base = lift_bytes(CASES[0].bytes);
    let index = sequence_index(&base);
    let temporary = match base.blocks[0].ops[index].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_immediate_provenance = base.clone();
    let mut bytes = CASES[0].bytes.to_vec();
    *bytes.last_mut().unwrap() ^= 1;
    wrong_immediate_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let mut trailing_provenance = base.clone();
    let mut bytes = CASES[0].bytes.to_vec();
    bytes.push(0);
    trailing_provenance
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[index].x86_hint = None;

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[index].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut child_hint = base.clone();
    child_hint.blocks[0].ops[index + 1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode: 0x44,
        width: VecWidth::V128,
        w: false,
    });

    let mut child_pc = base.clone();
    child_pc.blocks[0].ops[index + 1].guest_pc += 1;

    let mut reordered_extracts = base.clone();
    reordered_extracts.blocks[0].ops.swap(index + 1, index + 2);

    let mut alias_extracts = base.clone();
    let first_extract = match alias_extracts.blocks[0].ops[index + 1].kind {
        OpKind::VExtractLane { dst, .. } => dst,
        _ => unreachable!(),
    };
    if let OpKind::VExtractLane { dst, .. } = &mut alias_extracts.blocks[0].ops[index + 2].kind {
        *dst = first_extract;
    }

    let mut wrong_clmul = base.clone();
    if let OpKind::ClMul { acc, .. } = &mut wrong_clmul.blocks[0].ops[index + 3].kind {
        *acc = true;
    }

    let mut missing_high_product = base.clone();
    if let OpKind::ClMul { dst_hi, .. } = &mut missing_high_product.blocks[0].ops[index + 3].kind {
        *dst_hi = None;
    }

    let mut wrong_product_width = base.clone();
    if let OpKind::ClMul { elem_bits, .. } = &mut wrong_product_width.blocks[0].ops[index + 3].kind
    {
        *elem_bits = 32;
    }

    let mut wrong_extract_lane = base.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_extract_lane.blocks[0].ops[index + 1].kind
    {
        *lane ^= 1;
    }

    let mut wrong_extract_element = base.clone();
    if let OpKind::VExtractLane { elem, .. } =
        &mut wrong_extract_element.blocks[0].ops[index + 1].kind
    {
        *elem = VecElementType::F64;
    }

    let mut wrong_extract_extension = base.clone();
    if let OpKind::VExtractLane { sign, .. } =
        &mut wrong_extract_extension.blocks[0].ops[index + 1].kind
    {
        *sign = SignExtend::Sign;
    }

    let mut wrong_zero = base.clone();
    if let OpKind::Mov { src, .. } = &mut wrong_zero.blocks[0].ops[index + 4].kind {
        *src = SrcOperand::Imm(1);
    }

    let mut wrong_zero_width = base.clone();
    if let OpKind::Mov { width, .. } = &mut wrong_zero_width.blocks[0].ops[index + 4].kind {
        *width = OpWidth::W32;
    }

    let mut wrong_broadcast = base.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_broadcast.blocks[0].ops[index + 5].kind {
        *lanes = 4;
    }

    let mut wrong_broadcast_element = base.clone();
    if let OpKind::VBroadcast { elem, .. } =
        &mut wrong_broadcast_element.blocks[0].ops[index + 5].kind
    {
        *elem = VecElementType::I32;
    }

    let mut wrong_insert = base.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut wrong_insert.blocks[0].ops[index + 6].kind {
        *lane = 1;
    }

    let mut wrong_insert_element = base.clone();
    if let OpKind::VInsertLane { elem, .. } =
        &mut wrong_insert_element.blocks[0].ops[index + 6].kind
    {
        *elem = VecElementType::I32;
    }

    let mut wrong_destination = base.clone();
    if let OpKind::VMov { dst, .. } = &mut wrong_destination.blocks[0].ops[index + 8].kind {
        *dst = vector(3, VecWidth::V128);
    }

    let mut wrong_result_width = base.clone();
    if let OpKind::VMov { width, .. } = &mut wrong_result_width.blocks[0].ops[index + 8].kind {
        *width = VecWidth::V256;
    }

    let mut truncated = base.clone();
    truncated.blocks[0].ops.pop();

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7000), PC, OpKind::Nop));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("provenance immediate differs", wrong_immediate_provenance),
        ("provenance has trailing byte", trailing_provenance),
        ("load hint differs", load_hint),
        ("address contains virtual register", virtual_address),
        ("semantic child has hint", child_hint),
        ("semantic child PC differs", child_pc),
        ("extracts reordered", reordered_extracts),
        ("extract temporaries alias", alias_extracts),
        ("carry-less multiply accumulates", wrong_clmul),
        (
            "carry-less multiply lacks high product",
            missing_high_product,
        ),
        ("carry-less multiply width differs", wrong_product_width),
        ("source selector lane differs", wrong_extract_lane),
        ("source extract element differs", wrong_extract_element),
        ("source extract extension differs", wrong_extract_extension),
        ("zero seed is nonzero", wrong_zero),
        ("zero seed width differs", wrong_zero_width),
        ("broadcast lane count differs", wrong_broadcast),
        ("broadcast element differs", wrong_broadcast_element),
        ("insert lane differs", wrong_insert),
        ("insert element differs", wrong_insert_element),
        ("architectural destination differs", wrong_destination),
        ("result width differs", wrong_result_width),
        ("sequence is truncated", truncated),
        ("same-PC operation follows sequence", same_pc_tail),
    ] {
        assert_rejected(name, &function);
    }

    for (case, expected_virtuals) in [(CASES[0], 7), (CASES[4], 19)] {
        let function = lift_bytes(case.bytes);
        let virtuals = function.blocks[0]
            .ops
            .iter()
            .flat_map(|op| op.kind.dests())
            .filter(|reg| matches!(reg, VReg::Virtual(_)))
            .collect::<HashSet<_>>();
        assert_eq!(virtuals.len(), expected_virtuals, "{}", case.name);
        for (ordinal, reg) in virtuals.into_iter().enumerate() {
            let mut extra_use = function.clone();
            extra_use.blocks[0].ops.push(SmirOp::new(
                OpId(0x7100 + ordinal as u16),
                PC + 1,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                    src: SrcOperand::Reg(reg),
                    width: OpWidth::W64,
                },
            ));
            assert_rejected("temporary has an external use", &extra_use);

            let mut extra_definition = function.clone();
            extra_definition.blocks[0].ops.push(SmirOp::new(
                OpId(0x7200 + ordinal as u16),
                PC + 1,
                OpKind::Mov {
                    dst: reg,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            assert_rejected("temporary has an external definition", &extra_definition);
        }
    }

    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_vpclmulqdq_memory_sequence(
            &base.blocks[0],
            index,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );
    assert!(matches!(temporary, VReg::Virtual(_)));
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
    state: *mut crate::smir::lower::runtime::GuestRegs,
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
        || !matches!(size, 16 | 32 | 64)
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
#[derive(Clone, Debug)]
struct NativeCase {
    bytes: Vec<u8>,
    width: VecWidth,
    destination: u8,
    source1: u8,
    base: u8,
    displacement: u64,
    immediate: u8,
    needs_pclmulqdq: bool,
    needs_vpclmulqdq: bool,
    needs_avx512vl: bool,
    supports_avx_ymm16: bool,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::new();
    for immediate in [0x00, 0x01, 0x10, 0x11, 0xA5, 0x5A, 0xFF] {
        cases.push(NativeCase {
            bytes: vec![0xC4, 0xE3, 0x71, 0x44, 0x43, 0x20, immediate],
            width: VecWidth::V128,
            destination: 0,
            source1: 1,
            base: 3,
            displacement: 0x20,
            immediate,
            needs_pclmulqdq: true,
            needs_vpclmulqdq: false,
            needs_avx512vl: false,
            supports_avx_ymm16: true,
        });
        cases.push(NativeCase {
            bytes: vec![0xC4, 0x43, 0x25, 0x44, 0x4B, 0x20, immediate],
            width: VecWidth::V256,
            destination: 9,
            source1: 11,
            base: 11,
            displacement: 0x20,
            immediate,
            needs_pclmulqdq: false,
            needs_vpclmulqdq: true,
            needs_avx512vl: false,
            supports_avx_ymm16: true,
        });
    }
    cases.push(NativeCase {
        bytes: vec![0xC4, 0xE3, 0x71, 0x44, 0x4B, 0x20, 0x11],
        width: VecWidth::V128,
        destination: 1,
        source1: 1,
        base: 3,
        displacement: 0x20,
        immediate: 0x11,
        needs_pclmulqdq: true,
        needs_vpclmulqdq: false,
        needs_avx512vl: false,
        supports_avx_ymm16: true,
    });
    cases.push(NativeCase {
        bytes: vec![0xC4, 0xE3, 0x79, 0x44, 0x43, 0x20, 0x10],
        width: VecWidth::V128,
        destination: 0,
        source1: 0,
        base: 3,
        displacement: 0x20,
        immediate: 0x10,
        needs_pclmulqdq: true,
        needs_vpclmulqdq: false,
        needs_avx512vl: false,
        supports_avx_ymm16: true,
    });
    cases.extend([
        NativeCase {
            bytes: vec![0x62, 0xF3, 0x75, 0x08, 0x44, 0x43, 0x02, 0xEF],
            width: VecWidth::V128,
            destination: 0,
            source1: 1,
            base: 3,
            displacement: 0x20,
            immediate: 0xEF,
            needs_pclmulqdq: false,
            needs_vpclmulqdq: true,
            needs_avx512vl: true,
            supports_avx_ymm16: false,
        },
        NativeCase {
            bytes: vec![0x62, 0x43, 0x35, 0x20, 0x44, 0x43, 0x02, 0x10],
            width: VecWidth::V256,
            destination: 24,
            source1: 25,
            base: 11,
            displacement: 0x40,
            immediate: 0x10,
            needs_pclmulqdq: false,
            needs_vpclmulqdq: true,
            needs_avx512vl: true,
            supports_avx_ymm16: false,
        },
        NativeCase {
            bytes: vec![0x62, 0x43, 0x0D, 0x40, 0x44, 0x7B, 0x01, 0x01],
            width: VecWidth::V512,
            destination: 31,
            source1: 30,
            base: 11,
            displacement: 0x40,
            immediate: 0x01,
            needs_pclmulqdq: false,
            needs_vpclmulqdq: true,
            needs_avx512vl: false,
            supports_avx_ymm16: false,
        },
    ]);
    cases
}

#[cfg(target_arch = "x86_64")]
fn source_value(ordinal: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x0011_2233_4455_6677u64.rotate_left((ordinal * 7 + word * 11) as u32)
            ^ (word as u64).wrapping_mul(0x0F1E_2D3C_4B5A_6978)
    })
}

#[cfg(target_arch = "x86_64")]
fn source_bytes(source: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(source) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: &NativeCase, ordinal: usize) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64, X86_VECTOR_STATE_YMM16};

    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: if case.supports_avx_ymm16 {
            X86_VECTOR_STATE_YMM16
        } else {
            X86_VECTOR_STATE_K64
        },
        mxcsr: 0x1F80,
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
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x80;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreter_success(
    function: &SmirFunction,
    initial: &crate::smir::lower::runtime::GuestRegs,
    source: [u64; 8],
    address: u64,
    width: VecWidth,
) -> crate::smir::lower::runtime::GuestRegs {
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
    let bytes = source_bytes(source);
    memory.load(address as usize, &bytes[..width.bytes() as usize]);
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
    let words = (width.bytes() / 8) as usize;
    expected.vector_scratch =
        std::array::from_fn(|word| if word < words { source[word] } else { 0 });
    expected
}

#[cfg(target_arch = "x86_64")]
fn lower_native(function: &SmirFunction, supports_avx_ymm16: bool) -> (Vec<u8>, usize) {
    assert!(is_native_clobber_safe_excluding(
        function,
        &HashMap::new(),
        true
    ));
    assert_eq!(
        x86_native_vector_uses_avx_ymm16_only_excluding(function, &HashMap::new()),
        supports_avx_ymm16
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(supports_avx_ymm16);
    let result = lowerer
        .lower_function(function)
        .expect("lower native VPCLMULQDQ memory source");
    (
        lowerer
            .finalize()
            .expect("finalize native VPCLMULQDQ memory source"),
        result.entry_offset,
    )
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_vex_evex_memory_sources_match_interpretation_and_fault_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VPCLMULQDQ memory differential: host lacks AVX");
        return;
    }
    let pclmulqdq = std::is_x86_feature_detected!("pclmulqdq");
    let vpclmulqdq = std::is_x86_feature_detected!("vpclmulqdq");
    let avx512f = std::is_x86_feature_detected!("avx512f");
    let avx512bw = std::is_x86_feature_detected!("avx512bw");
    let avx512vl = std::is_x86_feature_detected!("avx512vl");
    let cases = native_cases()
        .into_iter()
        .filter(|case| {
            (!case.needs_pclmulqdq || pclmulqdq)
                && (!case.needs_vpclmulqdq || vpclmulqdq)
                && (case.supports_avx_ymm16
                    || (avx512f && avx512bw && (!case.needs_avx512vl || avx512vl)))
        })
        .collect::<Vec<_>>();
    if cases.is_empty() {
        eprintln!(
            "skipping native VPCLMULQDQ memory differential: host lacks PCLMULQDQ/VPCLMULQDQ"
        );
        return;
    }

    let levels = [OptLevel::O0, OptLevel::O2];
    let expected_executions = cases.len() * levels.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in levels {
            let mut function = lift_bytes(&case.bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            let requirements = x86_native_replay_feature_requirements(&function, &HashMap::new());
            assert_eq!(requirements.needs_pclmulqdq, case.needs_pclmulqdq);
            assert_eq!(requirements.needs_vpclmulqdq, case.needs_vpclmulqdq);
            assert_eq!(requirements.needs_avx512vl, case.needs_avx512vl);
            let index = sequence_index(&function);
            let (definitions, uses) = virtual_counts(&function);
            let sequence = x86_jit_vpclmulqdq_memory_sequence(
                &function.blocks[0],
                index,
                true,
                &function.x86_instruction_bytes,
                &definitions,
                &uses,
            )
            .expect("native VPCLMULQDQ sequence");
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_ne!(sequence.encoding.scratch, case.destination);
            assert_ne!(sequence.encoding.scratch, case.source1);
            let (code, entry) = lower_native(&function, case.supports_avx_ymm16);
            let exec = ExecMem::new(&code).expect("map VPCLMULQDQ memory replay");
            let source = source_value(ordinal ^ usize::from(case.immediate));

            let mut context = VectorMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(&case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(case.displacement);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected =
                interpreter_success(&function, &registers, source, address, case.width);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected,
                "{level:?} {:?} imm={:#04x}: success",
                case.width, case.immediate
            );
            assert_eq!(context.calls, 1);
            assert_eq!(context.last_addr, address);
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
            );
            assert_eq!(context.last_size, case.width.bytes());
            assert_eq!(context.last_zero_upper, 1);
            successes += 1;

            let mut context = VectorMemoryContext {
                value: source,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(&case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(case.displacement);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(
                registers, expected,
                "{level:?} {:?} imm={:#04x}: fault",
                case.width, case.immediate
            );
            assert_eq!(context.calls, 1);
            assert_eq!(context.last_addr, address);
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
            );
            assert_eq!(context.last_size, case.width.bytes());
            assert_eq!(context.last_zero_upper, 1);
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
