//! Helper-backed VEX/EVEX AES memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, OpId, VReg, VecWidth, VirtualId, X86AesOp, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_aes_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB9C0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug)]
struct AesMemoryCase {
    name: &'static str,
    bytes: &'static [u8],
    width: VecWidth,
    destination: u8,
    source1: Option<u8>,
    base: u8,
    operation: X86AesOp,
    immediate: u8,
    emitted: &'static [u8],
    supports_avx_ymm16: bool,
    needs_aes: bool,
    needs_vaes: bool,
}

const CASES: [AesMemoryCase; 7] = [
    AesMemoryCase {
        name: "VEX.128 VAESENC",
        bytes: &[0xC4, 0xE2, 0x71, 0xDC, 0x43, 0x20],
        width: VecWidth::V128,
        destination: 0,
        source1: Some(1),
        base: 3,
        operation: X86AesOp::Enc,
        immediate: 0,
        emitted: &[0xC4, 0xE2, 0x71, 0xDC, 0xC2],
        supports_avx_ymm16: true,
        needs_aes: true,
        needs_vaes: false,
    },
    AesMemoryCase {
        name: "VEX.256 VAESENCLAST",
        bytes: &[0xC4, 0x42, 0x35, 0xDD, 0x4B, 0x20],
        width: VecWidth::V256,
        destination: 9,
        source1: Some(9),
        base: 11,
        operation: X86AesOp::EncLast,
        immediate: 0,
        emitted: &[0xC4, 0x62, 0x35, 0xDD, 0xC8],
        supports_avx_ymm16: true,
        needs_aes: false,
        needs_vaes: true,
    },
    AesMemoryCase {
        name: "EVEX.128 VAESDEC high registers",
        bytes: &[0x62, 0xC2, 0x75, 0x00, 0xDE, 0x43, 0x02],
        width: VecWidth::V128,
        destination: 16,
        source1: Some(17),
        base: 11,
        operation: X86AesOp::Dec,
        immediate: 0,
        emitted: &[0x62, 0xE2, 0x75, 0x00, 0xDE, 0xC0],
        supports_avx_ymm16: false,
        needs_aes: false,
        needs_vaes: true,
    },
    AesMemoryCase {
        name: "EVEX.256 VAESDECLAST high registers",
        bytes: &[0x62, 0x42, 0x35, 0x20, 0xDF, 0x43, 0x01],
        width: VecWidth::V256,
        destination: 24,
        source1: Some(25),
        base: 11,
        operation: X86AesOp::DecLast,
        immediate: 0,
        emitted: &[0x62, 0x62, 0x35, 0x20, 0xDF, 0xC0],
        supports_avx_ymm16: false,
        needs_aes: false,
        needs_vaes: true,
    },
    AesMemoryCase {
        name: "EVEX.512 VAESENC",
        bytes: &[0x62, 0x42, 0x0D, 0x40, 0xDC, 0xBB, 0x20, 0, 0, 0],
        width: VecWidth::V512,
        destination: 31,
        source1: Some(30),
        base: 11,
        operation: X86AesOp::Enc,
        immediate: 0,
        emitted: &[0x62, 0x62, 0x0D, 0x40, 0xDC, 0xF8],
        supports_avx_ymm16: false,
        needs_aes: false,
        needs_vaes: true,
    },
    AesMemoryCase {
        name: "VEX VAESIMC",
        bytes: &[0xC4, 0x42, 0x79, 0xDB, 0x4B, 0x20],
        width: VecWidth::V128,
        destination: 9,
        source1: None,
        base: 11,
        operation: X86AesOp::InvMixColumns,
        immediate: 0,
        emitted: &[0xC4, 0x62, 0x79, 0xDB, 0xC8],
        supports_avx_ymm16: true,
        needs_aes: true,
        needs_vaes: false,
    },
    AesMemoryCase {
        name: "VEX VAESKEYGENASSIST",
        bytes: &[0xC4, 0x43, 0x79, 0xDF, 0x4B, 0x20, 0x5A],
        width: VecWidth::V128,
        destination: 9,
        source1: None,
        base: 11,
        operation: X86AesOp::KeygenAssist,
        immediate: 0x5A,
        emitted: &[0xC4, 0x63, 0x79, 0xDF, 0xC8, 0x5A],
        supports_avx_ymm16: true,
        needs_aes: true,
        needs_vaes: false,
    },
];

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("AES has only 128-, 256-, and 512-bit vector forms"),
    }))
}

fn assert_exact_pair(ops: &[SmirOp], case: AesMemoryCase) {
    let [load, aes] = ops else {
        panic!("{}: expected exact VLoad + X86Aes pair: {ops:?}", case.name)
    };
    assert_eq!(load.x86_hint, None, "{}", case.name);
    let temporary = match load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            width,
            ..
        } => {
            assert_eq!(width, case.width, "{}", case.name);
            temporary
        }
        ref other => panic!("{}: unexpected load {other:?}", case.name),
    };
    assert_eq!(aes.x86_hint, None, "{}", case.name);
    assert_eq!(aes.guest_pc, load.guest_pc, "{}", case.name);
    let OpKind::X86Aes {
        dst,
        src1,
        src2,
        width,
        op,
        imm,
    } = aes.kind
    else {
        panic!("{}: unexpected consumer {:?}", case.name, aes.kind)
    };
    assert_eq!(dst, vector(case.destination, case.width), "{}", case.name);
    assert_eq!(width, case.width, "{}", case.name);
    assert_eq!(op, case.operation, "{}", case.name);
    assert_eq!(imm, case.immediate, "{}", case.name);
    if let Some(source1) = case.source1 {
        assert_eq!(src1, vector(source1, case.width), "{}", case.name);
        assert_eq!(src2, Some(temporary), "{}", case.name);
    } else {
        assert_eq!(src1, temporary, "{}", case.name);
        assert_eq!(src2, None, "{}", case.name);
    }
}

fn lift_case(case: AesMemoryCase) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, case.bytes, &mut context)
        .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
    assert_eq!(result.bytes_consumed, case.bytes.len(), "{}", case.name);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_exact_pair(&result.ops, case);

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(case.bytes).expect("x86 instruction fits provenance"),
    );
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

fn lower(function: &SmirFunction, case: AesMemoryCase) -> (Vec<u8>, usize) {
    let excluded = std::collections::HashMap::new();
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
    assert_eq!(requirements.needs_aes, case.needs_aes, "{}", case.name);
    assert_eq!(requirements.needs_vaes, case.needs_vaes, "{}", case.name);
    assert_eq!(
        requirements.needs_avx512bw, !case.supports_avx_ymm16,
        "{}",
        case.name
    );
    assert_eq!(
        requirements.needs_avx512vl,
        !case.supports_avx_ymm16 && case.width != VecWidth::V512,
        "{}",
        case.name
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(case.supports_avx_ymm16);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
    assert!(result.relocations.is_empty(), "{}", case.name);
    (
        lowerer.finalize().expect("finalize AES memory source"),
        result.entry_offset,
    )
}

#[test]
fn exact_aes_memory_source_pairs_cross_the_native_gate_at_every_opt_level() {
    for case in CASES {
        for level in LEVELS {
            let mut function = lift_case(case);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert_exact_pair(&function.blocks[0].ops, case);
            assert!(
                is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "{level:?} {}",
                case.name
            );
            assert!(
                !is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    false,
                ),
                "{level:?} {}",
                case.name
            );
            assert!(
                !is_x86_aarch64_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                ),
                "{level:?} {}",
                case.name
            );
            let (definitions, uses) = virtual_counts(&function);
            let sequence =
                x86_jit_aes_memory_sequence(&function.blocks[0], 0, true, &definitions, &uses)
                    .unwrap_or_else(|| panic!("{level:?} {}: classifier rejected", case.name));
            assert_eq!(sequence.consumed, 2, "{level:?} {}", case.name);
            assert_eq!(
                sequence.memory_size,
                case.width.bytes(),
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                sequence.destination, case.destination,
                "{level:?} {}",
                case.name
            );
            assert_eq!(sequence.source1, case.source1, "{level:?} {}", case.name);
            assert_eq!(
                sequence.supports_avx_ymm16, case.supports_avx_ymm16,
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
                "{level:?} {}: missing vector-scratch displacement",
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

fn assert_rejected(name: &str, function: &SmirFunction) {
    let (definitions, uses) = virtual_counts(function);
    assert!(
        x86_jit_aes_memory_sequence(&function.blocks[0], 0, true, &definitions, &uses,).is_none(),
        "{name}: classifier admitted malformed pair"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), true,),
        "{name}: native gate admitted malformed pair"
    );
}

#[test]
fn aes_memory_classifier_fails_closed_for_every_pair_invariant() {
    let case = CASES[0];
    let base = lift_case(case);
    let temporary = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::VMov {
            dst: vector(3, VecWidth::V128),
            src: temporary,
            width: VecWidth::V128,
        },
    ));

    let mut load_hint = base.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::Rep,
        opcode: 0x6F,
        width: VecWidth::V128,
        w: false,
    });

    let mut consumer_hint = base.clone();
    consumer_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0xDC,
        width: VecWidth::V128,
        w: false,
    });

    let mut width_mismatch = base.clone();
    if let OpKind::X86Aes { width, .. } = &mut width_mismatch.blocks[0].ops[1].kind {
        *width = VecWidth::V256;
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;

    let mut virtual_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut virtual_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut bypassed_temporary = base.clone();
    if let OpKind::X86Aes { src2, .. } = &mut bypassed_temporary.blocks[0].ops[1].kind {
        *src2 = Some(vector(2, VecWidth::V128));
    }

    let mut nonzero_round_immediate = base.clone();
    if let OpKind::X86Aes { imm, .. } = &mut nonzero_round_immediate.blocks[0].ops[1].kind {
        *imm = 1;
    }

    let mut invalid_unary = lift_case(CASES[5]);
    if let OpKind::X86Aes { src2, .. } = &mut invalid_unary.blocks[0].ops[1].kind {
        *src2 = Some(vector(2, VecWidth::V128));
    }

    let mut high_unary_destination = lift_case(CASES[5]);
    if let OpKind::X86Aes { dst, .. } = &mut high_unary_destination.blocks[0].ops[1].kind {
        *dst = vector(16, VecWidth::V128);
    }

    for (name, function) in [
        ("temporary has a second use", extra_use),
        ("load has an encoding hint", load_hint),
        ("consumer has an encoding hint", consumer_hint),
        ("load and consumer widths differ", width_mismatch),
        ("load and consumer guest PCs differ", wrong_pc),
        ("address contains a virtual register", virtual_address),
        ("round bypasses the loaded temporary", bypassed_temporary),
        ("round immediate is nonzero", nonzero_round_immediate),
        ("unary operation has a second source", invalid_unary),
        ("unary destination exceeds XMM15", high_unary_destination),
    ] {
        assert_rejected(name, &function);
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
fn full_guest_regs(case: AesMemoryCase, ordinal: usize) -> crate::smir::lower::runtime::GuestRegs {
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
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn interpreter_success(
    function: &SmirFunction,
    initial: &crate::smir::lower::runtime::GuestRegs,
    source: [u64; 8],
    address: u64,
    case: AesMemoryCase,
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
    memory.load(address as usize, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{}: {result:?}",
        case.name
    );

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
    let words = (case.width.bytes() / 8) as usize;
    expected.vector_scratch =
        std::array::from_fn(|word| if word < words { source[word] } else { 0 });
    expected
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_aes_memory_sources_match_interpretation_and_fault_without_commit() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native AES memory-source differential: host lacks AVX");
        return;
    }
    let aes = std::is_x86_feature_detected!("aes");
    let vaes = std::is_x86_feature_detected!("vaes");
    let cases = CASES
        .into_iter()
        .filter(|case| {
            case.supports_avx_ymm16 && (!case.needs_aes || aes) && (!case.needs_vaes || vaes)
        })
        .collect::<Vec<_>>();
    if cases.is_empty() {
        eprintln!("skipping native AES memory-source differential: host lacks AES/VAES");
        return;
    }

    let levels = [OptLevel::O0, OptLevel::O2];
    let expected_executions = cases.len() * levels.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in levels {
            let mut function = lift_case(case);
            crate::smir::optimize::optimize_function(&mut function, level);
            let (code, entry) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
            let source = source_value(ordinal);

            let mut context = VectorMemoryContext {
                value: source,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(0x20);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = interpreter_success(&function, &registers, source, address, case);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {}: success", case.name);
            assert_eq!(context.calls, 1, "{level:?} {}", case.name);
            assert_eq!(context.last_addr, address, "{level:?} {}", case.name);
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {}",
                case.name
            );
            assert_eq!(
                context.last_size,
                case.width.bytes(),
                "{level:?} {}",
                case.name
            );
            assert_eq!(context.last_zero_upper, 1, "{level:?} {}", case.name);
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
            let mut registers = full_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(0x20);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {}: fault", case.name);
            assert_eq!(context.calls, 1, "fault {level:?} {}", case.name);
            assert_eq!(context.last_addr, address, "fault {level:?} {}", case.name);
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {}",
                case.name
            );
            assert_eq!(
                context.last_size,
                case.width.bytes(),
                "fault {level:?} {}",
                case.name
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {}", case.name);
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}
