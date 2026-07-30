//! Helper-backed VEX GFNI memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, SrcOperand, VReg, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexGfniMemoryKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_gfni_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x6FC0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug)]
struct GfniMemoryCase {
    name: &'static str,
    kind: X86VexGfniMemoryKind,
    bytes: &'static [u8],
    width: VecWidth,
    destination: u8,
    source1: u8,
    base: u8,
    displacement: u64,
    scratch: u8,
    emitted: &'static [u8],
}

const CASES: [GfniMemoryCase; 6] = [
    GfniMemoryCase {
        name: "VGF2P8MULB VEX.128 low registers",
        kind: X86VexGfniMemoryKind::Multiply,
        bytes: &[0xC4, 0xE2, 0x71, 0xCF, 0x43, 0x20],
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        base: 3,
        displacement: 0x20,
        scratch: 2,
        emitted: &[0xC4, 0xE2, 0x71, 0xCF, 0xC2],
    },
    GfniMemoryCase {
        name: "VGF2P8MULB VEX.256 high registers",
        kind: X86VexGfniMemoryKind::Multiply,
        bytes: &[0xC4, 0x42, 0x0D, 0xCF, 0x4B, 0x20],
        width: VecWidth::V256,
        destination: 9,
        source1: 14,
        base: 11,
        displacement: 0x20,
        scratch: 0,
        emitted: &[0xC4, 0x62, 0x0D, 0xCF, 0xC8],
    },
    GfniMemoryCase {
        name: "VGF2P8AFFINEQB VEX.128 destination/source alias",
        kind: X86VexGfniMemoryKind::Affine,
        bytes: &[0xC4, 0xE3, 0xF1, 0xCE, 0x4B, 0x20, 0xA5],
        width: VecWidth::V128,
        destination: 1,
        source1: 1,
        base: 3,
        displacement: 0x20,
        scratch: 0,
        emitted: &[0xC4, 0xE3, 0xF1, 0xCE, 0xC8, 0xA5],
    },
    GfniMemoryCase {
        name: "VGF2P8AFFINEQB VEX.256 high registers",
        kind: X86VexGfniMemoryKind::Affine,
        bytes: &[0xC4, 0x43, 0x8D, 0xCE, 0x4B, 0x20, 0x63],
        width: VecWidth::V256,
        destination: 9,
        source1: 14,
        base: 11,
        displacement: 0x20,
        scratch: 0,
        emitted: &[0xC4, 0x63, 0x8D, 0xCE, 0xC8, 0x63],
    },
    GfniMemoryCase {
        name: "VGF2P8AFFINEINVQB VEX.128 low registers",
        kind: X86VexGfniMemoryKind::AffineInverse,
        bytes: &[0xC4, 0xE3, 0xF1, 0xCF, 0x43, 0x20, 0x00],
        width: VecWidth::V128,
        destination: 0,
        source1: 1,
        base: 3,
        displacement: 0x20,
        scratch: 2,
        emitted: &[0xC4, 0xE3, 0xF1, 0xCF, 0xC2, 0x00],
    },
    GfniMemoryCase {
        name: "VGF2P8AFFINEINVQB VEX.256 high registers",
        kind: X86VexGfniMemoryKind::AffineInverse,
        bytes: &[0xC4, 0x43, 0x8D, 0xCF, 0x4B, 0x20, 0xFF],
        width: VecWidth::V256,
        destination: 9,
        source1: 14,
        base: 11,
        displacement: 0x20,
        scratch: 0,
        emitted: &[0xC4, 0x63, 0x8D, 0xCF, 0xC8, 0xFF],
    },
];

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX GFNI width"),
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
        .position(|op| matches!(op.kind, OpKind::VLoad { .. }))
        .expect("VEX GFNI memory load")
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

fn sequence(function: &SmirFunction) -> crate::smir::lower::runtime::X86JitVexGfniMemorySequence {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    x86_jit_vex_gfni_memory_sequence(
        &function.blocks[0],
        index,
        true,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
    .expect("exact VEX GFNI memory sequence")
}

fn lower(function: &SmirFunction) -> Vec<u8> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer
        .lower_function(function)
        .expect("lower VEX GFNI memory source");
    lowerer.finalize().expect("finalize VEX GFNI memory source")
}

#[test]
fn all_kinds_widths_and_optimization_profiles_admit_and_emit_exact_rewrites() {
    let expected_consumed = [
        (X86VexGfniMemoryKind::Multiply, OptLevel::O0, 88),
        (X86VexGfniMemoryKind::Multiply, OptLevel::O1, 82),
        (X86VexGfniMemoryKind::Multiply, OptLevel::O2, 82),
        (X86VexGfniMemoryKind::Affine, OptLevel::O0, 112),
        (X86VexGfniMemoryKind::Affine, OptLevel::O1, 112),
        (X86VexGfniMemoryKind::Affine, OptLevel::O2, 112),
        (X86VexGfniMemoryKind::AffineInverse, OptLevel::O0, 1230),
        (X86VexGfniMemoryKind::AffineInverse, OptLevel::O1, 1152),
        (X86VexGfniMemoryKind::AffineInverse, OptLevel::O2, 1152),
    ];

    let mut admitted = 0usize;
    for case in CASES {
        for level in LEVELS {
            let mut function = lift_bytes(case.bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            let sequence = sequence(&function);
            assert_eq!(sequence.encoding.kind, case.kind, "{}", case.name);
            assert_eq!(sequence.encoding.width, case.width, "{}", case.name);
            assert_eq!(
                sequence.encoding.destination, case.destination,
                "{}",
                case.name
            );
            assert_eq!(sequence.encoding.source1, case.source1, "{}", case.name);
            assert_eq!(sequence.encoding.scratch, case.scratch, "{}", case.name);
            assert_eq!(
                sequence.encoding.register_instruction.as_slice(),
                case.emitted,
                "{}",
                case.name
            );
            assert_eq!(
                sequence.consumed,
                expected_consumed
                    .iter()
                    .find_map(|(kind, profile_level, consumed)| {
                        (*kind == case.kind && *profile_level == level).then_some(*consumed)
                    })
                    .unwrap(),
                "{} {level:?}",
                case.name
            );
            assert_eq!(sequence.memory_size, case.width.bytes(), "{}", case.name);

            let excluded = HashMap::new();
            assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
            assert!(!is_native_clobber_safe_excluding(
                &function, &excluded, false
            ));
            assert!(!is_x86_aarch64_native_clobber_safe_excluding(
                &function, &excluded
            ));
            assert!(uses_x86_native_vectors_excluding(&function, &excluded));
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));

            let requirements = x86_native_replay_feature_requirements(&function, &excluded);
            assert!(requirements.any, "{}", case.name);
            assert!(requirements.all_spans_support_avx_ymm16, "{}", case.name);
            assert!(requirements.needs_avx, "{}", case.name);
            assert!(requirements.needs_gfni, "{}", case.name);
            assert!(!requirements.needs_avx2, "{}", case.name);
            assert!(!requirements.needs_avx512bw, "{}", case.name);
            assert!(!requirements.needs_avx512vl, "{}", case.name);
            assert!(!requirements.needs_aes, "{}", case.name);
            assert!(!requirements.needs_pclmulqdq, "{}", case.name);
            assert!(!requirements.needs_vpclmulqdq, "{}", case.name);

            let code = lower(&function);
            assert!(
                code.windows(case.emitted.len())
                    .any(|window| window == case.emitted),
                "{} {level:?}: missing {:02X?}",
                case.name,
                case.emitted
            );
            assert!(
                code.windows(5)
                    .any(|window| window == [0xBA, 0x20, 0, 0, 0]),
                "{} {level:?}: missing reserved helper destination",
                case.name
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{} {level:?}: missing vector scratch",
                case.name
            );
            admitted += 1;
        }
    }
    assert_eq!(admitted, CASES.len() * LEVELS.len());
}

fn scanner_encoding(
    kind: X86VexGfniMemoryKind,
    ymm: bool,
    encoded_vvvv: u8,
    destination_low: u8,
    immediate_value: u8,
) -> Vec<u8> {
    let (map, w, opcode, immediate) = match kind {
        X86VexGfniMemoryKind::Multiply => (2, false, 0xCF, None),
        X86VexGfniMemoryKind::Affine => (3, true, 0xCE, Some(immediate_value)),
        X86VexGfniMemoryKind::AffineInverse => (3, true, 0xCF, Some(immediate_value)),
    };
    let mut bytes = vec![
        0xC4,
        0xE0 | map,
        (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 1,
        opcode,
        (destination_low << 3) | 3,
    ];
    bytes.extend(immediate);
    bytes
}

#[test]
fn scanner_universe_admits_and_lowers_all_768_defined_memory_cells() {
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for kind in [
        X86VexGfniMemoryKind::Multiply,
        X86VexGfniMemoryKind::Affine,
        X86VexGfniMemoryKind::AffineInverse,
    ] {
        for ymm in [false, true] {
            for encoded_vvvv in 0u8..16 {
                for destination_low in 0u8..8 {
                    let bytes = scanner_encoding(kind, ymm, encoded_vvvv, destination_low, 0xA5);
                    let function = lift_bytes(&bytes);
                    let sequence = sequence(&function);
                    assert_eq!(sequence.encoding.kind, kind, "{bytes:02X?}");
                    assert_eq!(
                        sequence.encoding.width,
                        if ymm { VecWidth::V256 } else { VecWidth::V128 },
                        "{bytes:02X?}"
                    );
                    assert!(is_native_clobber_safe_excluding(
                        &function,
                        &HashMap::new(),
                        true
                    ));
                    admitted += 1;

                    let code = lower(&function);
                    let emitted = sequence.encoding.register_instruction.as_slice();
                    assert!(
                        code.windows(emitted.len()).any(|window| window == emitted),
                        "{bytes:02X?}: missing {:02X?}",
                        emitted
                    );
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(admitted, 768);
    assert_eq!(lowered, 768);
}

#[test]
fn every_affine_immediate_keeps_a_supported_o0_o1_o2_profile() {
    let mut classified = 0usize;
    for kind in [
        X86VexGfniMemoryKind::Affine,
        X86VexGfniMemoryKind::AffineInverse,
    ] {
        for ymm in [false, true] {
            for immediate in u8::MIN..=u8::MAX {
                let bytes = scanner_encoding(kind, ymm, 1, 5, immediate);
                for level in LEVELS {
                    let mut function = lift_bytes(&bytes);
                    crate::smir::optimize::optimize_function(&mut function, level);
                    let sequence = sequence(&function);
                    assert_eq!(sequence.encoding.kind, kind, "{bytes:02X?} {level:?}");
                    assert_eq!(
                        sequence.encoding.width,
                        if ymm { VecWidth::V256 } else { VecWidth::V128 },
                        "{bytes:02X?} {level:?}"
                    );
                    assert_eq!(
                        sequence.encoding.immediate,
                        Some(immediate),
                        "{bytes:02X?} {level:?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 2 * 2 * 256 * LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let index = sequence_index(function);
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_vex_gfni_memory_sequence(
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
fn memory_sequence_fails_closed_for_provenance_profile_dataflow_and_boundary_changes() {
    let base = lift_bytes(CASES[5].bytes);
    let index = sequence_index(&base);

    let mut missing_provenance = base.clone();
    missing_provenance.x86_instruction_bytes.clear();

    let mut wrong_kind_provenance = base.clone();
    wrong_kind_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(CASES[3].bytes).unwrap(),
    );

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[index].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[index].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut child_hint = base.clone();
    child_hint.blocks[0].ops[index + 1].x86_hint =
        Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));

    let mut child_pc = base.clone();
    child_pc.blocks[0].ops[index + 1].guest_pc += 1;

    let mut wrong_kind = base.clone();
    wrong_kind.blocks[0].ops[index + 1].kind = OpKind::Nop;

    let mut extra_memory = base.clone();
    let loaded = match base.blocks[0].ops[index].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    extra_memory.blocks[0].ops[index + 1].kind = OpKind::VLoad {
        dst: loaded,
        addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
        width: VecWidth::V256,
    };

    let mut wrong_source = base.clone();
    let source_op = wrong_source.blocks[0]
        .ops
        .iter_mut()
        .skip(index + 1)
        .find(|op| {
            op.kind
                .source_vregs()
                .contains(&vector(CASES[5].source1, CASES[5].width))
        })
        .expect("architectural GFNI first source");
    match &mut source_op.kind {
        OpKind::VAnd { src1, .. }
        | OpKind::VOr { src1, .. }
        | OpKind::VXor { src1, .. }
        | OpKind::VSub { src1, .. } => *src1 = vector(3, CASES[5].width),
        OpKind::VShift { src, .. } => *src = vector(3, CASES[5].width),
        OpKind::VByteShuffle { src, .. } => *src = vector(3, CASES[5].width),
        _ => panic!("unexpected architectural source op"),
    }

    let mut wrong_destination = base.clone();
    let last = wrong_destination.blocks[0].ops.last_mut().unwrap();
    if let OpKind::VMov { dst, .. } = &mut last.kind {
        *dst = vector(3, CASES[5].width);
    }

    let mut truncated = base.clone();
    truncated.blocks[0].ops.pop();

    let mut preceding_same_pc = base.clone();
    preceding_same_pc.blocks[0]
        .ops
        .insert(index, SmirOp::new(OpId(0x7000), PC, OpKind::Nop));

    let mut same_pc_tail = base.clone();
    same_pc_tail.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7001), PC, OpKind::Nop));

    let mut noncontiguous_same_pc = base.clone();
    noncontiguous_same_pc.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7002), PC + 1, OpKind::Nop));
    noncontiguous_same_pc.blocks[0]
        .ops
        .push(SmirOp::new(OpId(0x7003), PC, OpKind::Nop));

    for (name, function) in [
        ("missing provenance", missing_provenance),
        ("provenance kind differs", wrong_kind_provenance),
        ("load hint differs", load_hint),
        ("address contains a virtual register", virtual_address),
        ("semantic child has a hint", child_hint),
        ("semantic child PC differs", child_pc),
        ("operation profile differs", wrong_kind),
        ("second memory access exists", extra_memory),
        ("architectural source differs", wrong_source),
        ("architectural destination differs", wrong_destination),
        ("sequence is truncated", truncated),
        ("same-PC operation precedes load", preceding_same_pc),
        ("same-PC operation follows sequence", same_pc_tail),
        ("same-PC group is noncontiguous", noncontiguous_same_pc),
    ] {
        assert_rejected(name, &function);
    }

    let virtuals = base.blocks[0]
        .ops
        .iter()
        .flat_map(|op| op.kind.dests())
        .filter(|reg| matches!(reg, VReg::Virtual(_)))
        .collect::<std::collections::HashSet<_>>();
    assert!(!virtuals.is_empty());
    let (_, base_uses) = virtual_counts(&base);
    let unused = virtuals
        .iter()
        .copied()
        .find(|reg| !base_uses.contains_key(reg))
        .expect("O0 inverse graph contains a defined but unused temporary");
    let used = virtuals
        .iter()
        .copied()
        .find(|reg| base_uses.contains_key(reg))
        .expect("O0 inverse graph contains a used temporary");
    for (ordinal, reg) in [unused, used].into_iter().enumerate() {
        let mut extra_use = base.clone();
        extra_use.blocks[0].ops.push(SmirOp::new(
            OpId(0x7100 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Reg(reg),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("temporary has an external use", &extra_use);

        let mut extra_definition = base.clone();
        extra_definition.blocks[0].ops.push(SmirOp::new(
            OpId(0x7200 + ordinal as u16),
            PC + 1,
            OpKind::Mov {
                dst: reg,
                src: SrcOperand::Imm(0),
                width: crate::smir::ir::types::OpWidth::W64,
            },
        ));
        assert_rejected("temporary has an external definition", &extra_definition);
    }

    let (definitions, uses) = virtual_counts(&base);
    assert!(
        x86_jit_vex_gfni_memory_sequence(
            &base.blocks[0],
            index,
            false,
            &base.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    );
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
fn source_values(ordinal: usize) -> [[u64; 8]; 4] {
    [
        [0; 8],
        [u64::MAX; 8],
        [
            0x0001_0204_0810_2040,
            0x8040_2010_0804_0201,
            0x63A5_5A96_C3F0_0F3C,
            0x1B11_0D09_0705_0301,
            0x0123_4567_89AB_CDEF,
            0xFEDC_BA98_7654_3210,
            0x6996_F00F_3CC3_A55A,
            0xA55A_3CC3_F00F_6996,
        ],
        std::array::from_fn(|word| {
            0x0011_2233_4455_6677u64.rotate_left((ordinal * 7 + word * 11) as u32)
                ^ (word as u64).wrapping_mul(0x0F1E_2D3C_4B5A_6978)
        }),
    ]
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
fn full_guest_regs(
    case: &GfniMemoryCase,
    ordinal: usize,
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};

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
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (ordinal as u64).wrapping_mul(0x0804_0201_1020_4081)
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
fn lower_native(function: &SmirFunction) -> (Vec<u8>, usize) {
    assert!(is_native_clobber_safe_excluding(
        function,
        &HashMap::new(),
        true
    ));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function,
        &HashMap::new()
    ));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .expect("lower native VEX GFNI memory source");
    (
        lowerer
            .finalize()
            .expect("finalize native VEX GFNI memory source"),
        result.entry_offset,
    )
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_sources_match_interpretation_and_fault_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("gfni") {
        eprintln!("skipping native VEX GFNI memory differential: host lacks AVX or GFNI");
        return;
    }

    let levels = [OptLevel::O0, OptLevel::O2];
    let expected_executions = CASES.len() * levels.len() * 4;
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (case_ordinal, case) in CASES.into_iter().enumerate() {
        for level in levels {
            let mut function = lift_bytes(case.bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            let sequence = sequence(&function);
            assert_eq!(sequence.encoding.destination, case.destination);
            assert_eq!(sequence.encoding.source1, case.source1);
            assert_ne!(sequence.encoding.scratch, case.destination);
            assert_ne!(sequence.encoding.scratch, case.source1);
            let requirements = x86_native_replay_feature_requirements(&function, &HashMap::new());
            assert!(requirements.needs_avx);
            assert!(requirements.needs_gfni);
            let (code, entry) = lower_native(&function);
            assert!(
                code.windows(case.emitted.len())
                    .any(|window| window == case.emitted)
            );
            let exec = ExecMem::new(&code).expect("map VEX GFNI memory replay");

            for (source_ordinal, source) in source_values(case_ordinal).into_iter().enumerate() {
                let ordinal = case_ordinal * 4 + source_ordinal;
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
                    "{level:?} {} source {source_ordinal}: success",
                    case.name
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
                    "{level:?} {} source {source_ordinal}: fault",
                    case.name
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
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
