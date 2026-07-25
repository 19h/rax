//! Native replay coverage for register-only AMD AVX VEX FMA4 instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0xF4A4;
const OPCODES: [u8; 20] = [
    0x5C, 0x5D, 0x5E, 0x5F, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x78, 0x79, 0x7A, 0x7B,
    0x7C, 0x7D, 0x7E, 0x7F,
];
const OPERANDS: [(u8, u8, u8, u8); 9] = [
    (1, 2, 3, 4),
    (9, 10, 11, 12),
    (1, 1, 2, 3),
    (1, 2, 1, 3),
    (1, 2, 3, 1),
    (1, 2, 2, 3),
    (1, 2, 3, 2),
    (1, 2, 3, 3),
    (1, 1, 1, 1),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    F32,
    F64,
}

impl Element {
    fn bytes(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Add,
    Sub,
    AddSub,
    SubAdd,
    NegativeMultiplyAdd,
    NegativeMultiplySub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Spec {
    element: Element,
    kind: Kind,
    scalar: bool,
}

fn spec(opcode: u8) -> Spec {
    let element = if opcode & 1 == 0 {
        Element::F32
    } else {
        Element::F64
    };
    let (kind, scalar) = match opcode {
        0x5C | 0x5D => (Kind::AddSub, false),
        0x5E | 0x5F => (Kind::SubAdd, false),
        0x68 | 0x69 => (Kind::Add, false),
        0x6A | 0x6B => (Kind::Add, true),
        0x6C | 0x6D => (Kind::Sub, false),
        0x6E | 0x6F => (Kind::Sub, true),
        0x78 | 0x79 => (Kind::NegativeMultiplyAdd, false),
        0x7A | 0x7B => (Kind::NegativeMultiplyAdd, true),
        0x7C | 0x7D => (Kind::NegativeMultiplySub, false),
        0x7E | 0x7F => (Kind::NegativeMultiplySub, true),
        _ => panic!("not an FMA4 opcode: {opcode:02X}"),
    };
    Spec {
        element,
        kind,
        scalar,
    }
}

fn encoding(
    opcode: u8,
    w: bool,
    l: bool,
    dst: u8,
    src1: u8,
    rm: u8,
    is4: u8,
    ignored_low: u8,
    clear_ignored_x: bool,
) -> [u8; 6] {
    assert!(OPCODES.contains(&opcode));
    assert!(dst < 16 && src1 < 16 && rm < 16 && is4 < 16 && ignored_low < 16);
    let mut p0 = 0xE3;
    if dst >= 8 {
        p0 &= !0x80;
    }
    if clear_ignored_x {
        p0 &= !0x40;
    }
    if rm >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(w) << 7) | ((!src1 & 0x0F) << 3) | (u8::from(l) << 2) | 1,
        opcode,
        0xC0 | ((dst & 7) << 3) | (rm & 7),
        (is4 << 4) | ignored_low,
    ]
}

fn function(bytes: &[u8; 6]) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

#[test]
fn replay_feature_aggregation_selects_avx_fma4_ymm16_boundary_and_rejects_mixed_vectors() {
    let bytes = encoding(0x7F, true, true, 15, 14, 13, 12, 15, true);
    let function = function(&bytes);
    let requirements =
        x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
    assert!(requirements.any);
    assert!(requirements.all_spans_are_fma4);
    assert!(requirements.needs_avx);
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_fma);
    assert!(requirements.needs_fma4);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(!requirements.needs_avx512cd);
    assert!(!requirements.needs_gfni);
    assert!(!requirements.needs_avx512vp2intersect);
    assert!(!requirements.needs_vpclmulqdq);

    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx") && x86_host_has_fma4()
        );
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx") && x86_host_has_fma4()
        );
    }
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut mixed = function.clone();
    let mut vector_block = crate::smir::ir::SmirBlock::new(BlockId(1), PC + 0x100);
    let mut move_op = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(10_000),
        PC + 0x100,
        crate::smir::ir::ops::OpKind::VMov {
            dst: crate::smir::ir::types::VReg::Arch(crate::smir::ir::types::ArchReg::X86(
                crate::smir::ir::types::X86Reg::Ymm(1),
            )),
            src: crate::smir::ir::types::VReg::Arch(crate::smir::ir::types::ArchReg::X86(
                crate::smir::ir::types::X86Reg::Ymm(2),
            )),
            width: crate::smir::ir::types::VecWidth::V256,
        },
    );
    move_op.x86_hint = Some(crate::smir::ir::ops::X86OpHint::VexOp {
        map: crate::smir::ir::ops::X86VecMap::Map0F,
        pp: crate::smir::ir::ops::X86SsePrefix::None,
        opcode: 0x28,
        width: crate::smir::ir::types::VecWidth::V256,
        w: false,
    });
    vector_block.push_op(move_op);
    vector_block.set_terminator(Terminator::Return { values: Vec::new() });
    mixed.add_block(vector_block);
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        &mixed,
        &std::collections::HashMap::new()
    ));

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn replay_admits_emits_1_440_o0_o2_family_width_role_alias_and_is4_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for opcode in OPCODES {
        for w in [false, true] {
            for l in [false, true] {
                for (operand_index, (dst, src1, rm, is4)) in OPERANDS.into_iter().enumerate() {
                    let bytes = encoding(
                        opcode,
                        w,
                        l,
                        dst,
                        src1,
                        rm,
                        is4,
                        operand_index as u8,
                        operand_index & 1 != 0,
                    );
                    for level in [
                        crate::smir::optimize::OptLevel::O0,
                        crate::smir::optimize::OptLevel::O2,
                    ] {
                        let mut function = function(&bytes);
                        crate::smir::optimize::optimize_function(&mut function, level);
                        assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
                        assert!(
                            uses_x86_native_vectors_excluding(
                                &function,
                                &std::collections::HashMap::new()
                            ),
                            "{level:?} {bytes:02X?}"
                        );
                        let mut lowerer = X86_64Lowerer::new();
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                        let code = lowerer
                            .finalize()
                            .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                        assert!(
                            code.windows(bytes.len()).any(|window| window == bytes),
                            "{level:?} {bytes:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 1_440);

    let bytes = encoding(0x68, false, false, 1, 2, 3, 4, 0, false);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes;
    memory_bytes[4] &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FmaState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SemanticCase {
    opcode: u8,
    w: bool,
    l: bool,
    dst: u8,
    src1: u8,
    rm: u8,
    is4: u8,
    ignored_low: u8,
    clear_ignored_x: bool,
}

fn element_bits(vector: &[u64; 8], element: Element, lane: usize) -> u64 {
    match element {
        Element::F32 => (vector[lane / 2] >> ((lane % 2) * 32)) & u64::from(u32::MAX),
        Element::F64 => vector[lane],
    }
}

fn set_element_bits(vector: &mut [u64; 8], element: Element, lane: usize, bits: u64) {
    match element {
        Element::F32 => {
            let shift = (lane % 2) * 32;
            let mask = u64::from(u32::MAX) << shift;
            vector[lane / 2] = (vector[lane / 2] & !mask) | ((bits & u64::from(u32::MAX)) << shift);
        }
        Element::F64 => vector[lane] = bits,
    }
}

const EXACT_VALUES: [f64; 16] = [
    0.25, -0.25, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 4.0, -4.0, 8.0, -8.0, 16.0, -16.0, 32.0, -32.0,
];

fn exact_vector(element: Element, register: usize, ordinal: usize) -> [u64; 8] {
    let mut vector = [0u64; 8];
    for lane in 0..64 / element.bytes() {
        let value = EXACT_VALUES[(ordinal + register * 3 + lane * 5) % EXACT_VALUES.len()];
        let bits = match element {
            Element::F32 => u64::from((value as f32).to_bits()),
            Element::F64 => value.to_bits(),
        };
        set_element_bits(&mut vector, element, lane, bits);
    }
    vector
}

fn exact_initial_state(case: SemanticCase, ordinal: usize) -> FmaState {
    let element = spec(case.opcode).element;
    FmaState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| exact_vector(element, register, ordinal)),
        masks: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            u64::MAX,
        ],
        rflags: 0x2 | 0x0CD5,
        mxcsr: 0x1F80,
    }
}

fn exact_fma(element: Element, kind: Kind, lane: usize, a: u64, b: u64, c: u64) -> u64 {
    let subtract = matches!(kind, Kind::Sub | Kind::NegativeMultiplySub)
        || (kind == Kind::AddSub && lane & 1 == 0)
        || (kind == Kind::SubAdd && lane & 1 != 0);
    let negative_product = matches!(kind, Kind::NegativeMultiplyAdd | Kind::NegativeMultiplySub);
    match element {
        Element::F32 => {
            let mut a = f32::from_bits(a as u32);
            let b = f32::from_bits(b as u32);
            let mut c = f32::from_bits(c as u32);
            if negative_product {
                a = -a;
            }
            if subtract {
                c = -c;
            }
            u64::from(a.mul_add(b, c).to_bits())
        }
        Element::F64 => {
            let mut a = f64::from_bits(a);
            let b = f64::from_bits(b);
            let mut c = f64::from_bits(c);
            if negative_product {
                a = -a;
            }
            if subtract {
                c = -c;
            }
            a.mul_add(b, c).to_bits()
        }
    }
}

fn architectural_expected(case: SemanticCase, initial: &FmaState) -> FmaState {
    let instruction = spec(case.opcode);
    let src1 = initial.vectors[usize::from(case.src1)];
    let rm = initial.vectors[usize::from(case.rm)];
    let is4 = initial.vectors[usize::from(case.is4)];
    let (src2, src3) = if case.w { (is4, rm) } else { (rm, is4) };
    let width = if instruction.scalar || !case.l {
        16
    } else {
        32
    };
    let lanes = if instruction.scalar {
        1
    } else {
        width / instruction.element.bytes()
    };

    let mut expected = initial.clone();
    let destination = &mut expected.vectors[usize::from(case.dst)];
    destination.fill(0);
    for lane in 0..lanes {
        let result = exact_fma(
            instruction.element,
            instruction.kind,
            lane,
            element_bits(&src1, instruction.element, lane),
            element_bits(&src2, instruction.element, lane),
            element_bits(&src3, instruction.element, lane),
        );
        set_element_bits(destination, instruction.element, lane, result);
    }
    expected
}

fn optimized_function(
    bytes: &[u8; 6],
    level: crate::smir::optimize::OptLevel,
    halt: bool,
) -> crate::smir::ir::SmirFunction {
    let mut function = function(bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
    }
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn interpret(
    bytes: &[u8; 6],
    initial: &FmaState,
    level: crate::smir::optimize::OptLevel,
) -> FmaState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
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
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    FmaState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_amd_fused_equations_roles_aliases_scalar_and_upper_zeroing() {
    let mut checked = 0usize;
    for opcode in OPCODES {
        for w in [false, true] {
            for l in [false, true] {
                for (operand_index, (dst, src1, rm, is4)) in OPERANDS.into_iter().enumerate() {
                    let case = SemanticCase {
                        opcode,
                        w,
                        l,
                        dst,
                        src1,
                        rm,
                        is4,
                        ignored_low: operand_index as u8,
                        clear_ignored_x: operand_index & 1 != 0,
                    };
                    let bytes = encoding(
                        case.opcode,
                        case.w,
                        case.l,
                        case.dst,
                        case.src1,
                        case.rm,
                        case.is4,
                        case.ignored_low,
                        case.clear_ignored_x,
                    );
                    let initial = exact_initial_state(case, checked);
                    let expected = architectural_expected(case, &initial);
                    for level in [
                        crate::smir::optimize::OptLevel::O0,
                        crate::smir::optimize::OptLevel::O2,
                    ] {
                        assert_eq!(
                            interpret(&bytes, &initial, level),
                            expected,
                            "{level:?} {case:?} {bytes:02X?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1_440);
}

#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u64; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x3F80_0000,
    0xBF80_0000,
    0x4000_0000,
    0x3F00_0000,
    0x0000_0001,
    0x8000_0001,
    0x0080_0000,
    0x7F7F_FFFF,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC1_2345,
    0x7F81_2345,
    0x3F80_0001,
    0x3EAA_AAAB,
];

#[cfg(target_arch = "x86_64")]
const F64_PATTERNS: [u64; 16] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0xBFF0_0000_0000_0000,
    0x4000_0000_0000_0000,
    0x3FE0_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x8000_0000_0000_0001,
    0x0010_0000_0000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_2468_ACE0_1357,
    0x7FF0_2468_ACE0_1357,
    0x3FF0_0000_0000_0001,
    0x3FD5_5555_5555_5555,
];

#[cfg(target_arch = "x86_64")]
fn patterned_vector(element: Element, register: usize, ordinal: usize) -> [u64; 8] {
    let patterns: &[u64] = if element == Element::F32 {
        &F32_PATTERNS
    } else {
        &F64_PATTERNS
    };
    let mut bytes = [0u8; 64];
    for lane in 0..64 / element.bytes() {
        let value = patterns[(ordinal + lane + register * 5) % patterns.len()].to_le_bytes();
        let base = lane * element.bytes();
        bytes[base..base + element.bytes()].copy_from_slice(&value[..element.bytes()]);
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn adversarial_initial_state(case: SemanticCase, ordinal: usize) -> FmaState {
    let element = spec(case.opcode).element;
    let prior_status = (ordinal as u32).rotate_left(3) & 0x3F;
    let rc = ((ordinal as u32 >> 2) & 3) << 13;
    let denormal_controls = if ordinal & 1 == 0 {
        0
    } else {
        (1 << 6) | (1 << 15)
    };
    FmaState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(element, register, ordinal)),
        masks: [
            0x6996_F00F_3CC3_A55A,
            0xA55A_3CC3_F00F_9696,
            0,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            1,
        ],
        rflags: 0x2 | 0x0CD5,
        // All exception masks remain set. The CPU-level JIT boundary rejects
        // replay when a mask is clear, preventing host SIGFPE while preserving
        // status, rounding-control, DAZ, and FTZ coverage.
        mxcsr: 0x1F80 | prior_status | rc | denormal_controls,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &FmaState,
    level: crate::smir::optimize::OptLevel,
) -> FmaState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map VEX FMA4 replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    exec.run(lowered.entry_offset, &mut registers);

    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    FmaState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn avx_ymm16_bridge_executes_without_avx512_and_fma4_postlude_clears_only_its_destination() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping AVX YMM16 bridge execution: host lacks AVX");
        return;
    }

    let case = SemanticCase {
        opcode: 0x68,
        w: true,
        l: true,
        dst: 9,
        src1: 10,
        rm: 11,
        is4: 12,
        ignored_low: 0x0F,
        clear_ignored_x: true,
    };
    let bytes = encoding(
        case.opcode,
        case.w,
        case.l,
        case.dst,
        case.src1,
        case.rm,
        case.is4,
        case.ignored_low,
        case.clear_ignored_x,
    );
    let function = optimized_function(&bytes, crate::smir::optimize::OptLevel::O2, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer.lower_function(&function).unwrap();
    let mut code = lowerer.finalize().unwrap();
    let offsets: Vec<_> = code
        .windows(bytes.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == bytes).then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 1, "{bytes:02X?}");

    // Execute the real lowerer-generated state postlude and trampoline without
    // requiring FMA4 on this host. Replacing only the source instruction with
    // flag-neutral NOPs isolates the AVX bridge and dynamic upper-clear logic.
    code[offsets[0]..offsets[0] + bytes.len()].fill(0x90);
    let exec = ExecMem::new(&code).expect("map patched FMA4 postlude");
    let initial = exact_initial_state(case, 17);
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: X86_VECTOR_STATE_YMM16,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    exec.run(lowered.entry_offset, &mut registers);

    assert_eq!(registers.gpr, initial.gprs);
    assert_eq!(registers.rflags, initial.rflags);
    assert_eq!(registers.k, initial.masks);
    assert_eq!(registers.mxcsr, initial.mxcsr);
    for (index, initial_vector) in initial.vectors.iter().enumerate() {
        let mut expected = *initial_vector;
        if index == usize::from(case.dst) {
            expected[4..].fill(0);
        }
        assert_eq!(registers.get_zmm(index), expected, "ZMM{index}");
    }
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<(crate::smir::optimize::OptLevel, SemanticCase)> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for opcode in OPCODES {
            for w in [false, true] {
                for l in [false, true] {
                    let (dst, src1, rm, is4) = OPERANDS[ordinal % OPERANDS.len()];
                    cases.push((
                        level,
                        SemanticCase {
                            opcode,
                            w,
                            l,
                            dst,
                            src1,
                            rm,
                            is4,
                            ignored_low: (ordinal & 0x0F) as u8,
                            clear_ignored_x: ordinal & 1 != 0,
                        },
                    ));
                    ordinal += 1;
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FMA4_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(
    cases: &[(crate::smir::optimize::OptLevel, SemanticCase)],
    range: std::ops::Range<usize>,
) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &(level, case)) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(
            case.opcode,
            case.w,
            case.l,
            case.dst,
            case.src1,
            case.rm,
            case.is4,
            case.ignored_low,
            case.clear_ignored_x,
        );
        let initial = adversarial_initial_state(case, ordinal);
        assert_eq!(
            execute_native(&bytes, &initial, level),
            interpret(&bytes, &initial, level),
            "{level:?} {case:?} {bytes:02X?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX FMA4 differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    assert_eq!(cases.len(), 160);
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let (level, case) = cases[start];
    let bytes = encoding(
        case.opcode,
        case.w,
        case.l,
        case.dst,
        case.src1,
        case.rm,
        case.is4,
        case.ignored_low,
        case.clear_ignored_x,
    );
    panic!(
        "isolated native VEX FMA4 failure at case {start}/{}: \
         {level:?} {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_o0_o2_interpretation_for_all_families_roles_aliases_mxcsr_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !x86_host_has_fma4() {
        eprintln!("skipping native VEX FMA4 differential: host lacks AVX/FMA4");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_fma4_replay::\
         replay_matches_o0_o2_interpretation_for_all_families_roles_aliases_mxcsr_and_full_state",
    );
}
