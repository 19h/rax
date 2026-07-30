//! Exact helper-backed VEX.128 `VPHMINPOSUW` memory-source coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, OpId, VReg, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_YMM16, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xC410;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhminposuwMemoryCase {
    destination: u8,
    w: bool,
}

impl PhminposuwMemoryCase {
    const fn base(self) -> u8 {
        if self.destination < 8 { 3 } else { 11 }
    }

    fn scratch(self) -> u8 {
        (0..16)
            .find(|index| *index != self.destination)
            .expect("one VEX destination leaves fifteen scratch registers")
    }

    fn bytes(self) -> Vec<u8> {
        let base = self.base();
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.w { 0 } else { 0x40 })
                | (if base < 8 { 0x20 } else { 0 })
                | 0x02,
            (u8::from(self.w) << 7) | 0x79,
            0x41,
            0x40 | ((self.destination & 7) << 3) | (base & 7),
            DISP as u8,
        ]
    }

    fn emitted_instruction(self) -> Vec<u8> {
        let scratch = self.scratch();
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.w { 0 } else { 0x40 })
                | (if scratch < 8 { 0x20 } else { 0 })
                | 0x02,
            (u8::from(self.w) << 7) | 0x79,
            0x41,
            0xC0 | ((self.destination & 7) << 3) | scratch,
        ]
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

fn expected_address(case: PhminposuwMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base())),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn assert_exact_pair(ops: &[SmirOp], case: PhminposuwMemoryCase) {
    let [load, minimum] = ops else {
        panic!("{case:?}: expected exact VLoad/X86Phminposuw pair, got {ops:?}")
    };
    let temporary = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width: VecWidth::V128,
        } => {
            assert_eq!(addr, &expected_address(case), "{case:?}");
            *temporary
        }
        other => panic!("{case:?}: expected virtual VLoad, got {other:?}"),
    };
    assert!(
        matches!(
            load.x86_hint,
            Some(X86OpHint::VecAlign(
                X86VecAlign::Unaligned | X86VecAlign::Aligned
            ))
        ),
        "{case:?}: {:?}",
        load.x86_hint
    );
    assert_eq!(minimum.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(
        minimum.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x41,
            width: VecWidth::V128,
            w: case.w,
        }),
        "{case:?}"
    );
    assert!(
        matches!(
            minimum.kind,
            OpKind::X86Phminposuw { dst, src }
                if dst == vector(case.destination) && src == temporary
        ),
        "{case:?}: {:?}",
        minimum.kind
    );
}

fn lift_instruction(bytes: &[u8]) -> SmirFunction {
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

fn lift_case(case: PhminposuwMemoryCase) -> SmirFunction {
    let function = lift_instruction(&case.bytes());
    assert_exact_pair(&function.blocks[0].ops, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize) {
    let excluded = std::collections::HashMap::new();
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
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed VEX VPHMINPOSUW lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed VEX VPHMINPOSUW"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<PhminposuwMemoryCase> {
    let mut cases = Vec::new();
    for destination in 0..16 {
        for w in [false, true] {
            cases.push(PhminposuwMemoryCase { destination, w });
        }
    }
    cases
}

#[test]
fn all_32_destination_and_wig_cells_are_lifted_optimized_admitted_and_lowered() {
    let cases = all_cases();
    assert_eq!(cases.len(), 16 * 2);
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function.blocks[0].ops, case);
            let (code, _) = lower(&function);
            let expected = case.emitted_instruction();
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                }),
                "{level:?} {case:?}: missing vector-scratch displacement"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 32 * LEVELS.len());
}

#[test]
fn complete_address_shapes_and_llvm_23_encodings_rewrite_and_lower_exactly() {
    for (name, bytes, expected) in [
        (
            "LLVM r11 disp8 to xmm9",
            &[0xC4, 0x42, 0x79, 0x41, 0x4B, 0x20][..],
            &[0xC4, 0x62, 0x79, 0x41, 0xC8][..],
        ),
        (
            "RSP SIB",
            &[0xC4, 0xE2, 0x79, 0x41, 0x44, 0x24, 0x20][..],
            &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
        ),
        (
            "RBP disp8",
            &[0xC4, 0xE2, 0x79, 0x41, 0x45, 0x20][..],
            &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
        ),
        (
            "R12 SIB",
            &[0xC4, 0xC2, 0x79, 0x41, 0x44, 0x24, 0x20][..],
            &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
        ),
        (
            "R13 disp8",
            &[0xC4, 0xC2, 0x79, 0x41, 0x45, 0x20][..],
            &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
        ),
        (
            "LLVM extended SIB disp32 to xmm14",
            &[0xC4, 0x02, 0x79, 0x41, 0xB4, 0x7E, 0x44, 0x33, 0x22, 0x11][..],
            &[0xC4, 0x22, 0x79, 0x41, 0xF0][..],
        ),
        (
            "RIP relative",
            &[0xC4, 0xE2, 0x79, 0x41, 0x05, 0x44, 0x33, 0x22, 0x11][..],
            &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
        ),
        (
            "FS base",
            &[0x64, 0xC4, 0xE2, 0x79, 0x41, 0x00][..],
            &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
        ),
        (
            "GS address-size absolute W1",
            &[
                0x65, 0x67, 0xC4, 0xE2, 0xF9, 0x41, 0x04, 0x25, 0x44, 0x33, 0x22, 0x11,
            ][..],
            &[0xC4, 0xE2, 0xF9, 0x41, 0xC1][..],
        ),
    ] {
        let function = optimize(lift_instruction(bytes), OptLevel::O2);
        assert_eq!(function.blocks[0].ops.len(), 2, "{name}");
        let (code, _) = lower(&function);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    let excluded = std::collections::HashMap::new();
    assert!(
        !is_native_clobber_safe_excluding(function, &excluded, true),
        "{name}: clobber gate admitted malformed pair"
    );
    assert!(
        !x86_native_replay_feature_requirements(function, &excluded).any,
        "{name}: feature classifier admitted malformed pair"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed pair"
    );
}

#[test]
fn classifier_and_lowerer_fail_closed_for_every_pair_and_provenance_invariant() {
    let case = PhminposuwMemoryCase {
        destination: 3,
        w: false,
    };
    let base = lift_case(case);
    let temporary = match base.blocks[0].ops[0].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };

    let mut extra_use = base.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC + 1,
        OpKind::VMov {
            dst: vector(4),
            src: temporary,
            width: VecWidth::V128,
        },
    ));

    let mut extra_definition = base.clone();
    extra_definition.blocks[0].ops.insert(
        0,
        SmirOp::new(
            OpId(2),
            PC - 1,
            OpKind::VMov {
                dst: temporary,
                src: vector(4),
                width: VecWidth::V128,
            },
        ),
    );

    let mut nonvirtual_load = base.clone();
    if let OpKind::VLoad { dst, .. } = &mut nonvirtual_load.blocks[0].ops[0].kind {
        *dst = vector(5);
    }

    let mut no_load_hint = base.clone();
    no_load_hint.blocks[0].ops[0].x86_hint = None;

    let mut wrong_load_hint = base.clone();
    wrong_load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0x10,
        width: VecWidth::V128,
        w: false,
    });

    let mut load_width = base.clone();
    if let OpKind::VLoad { width, .. } = &mut load_width.blocks[0].ops[0].kind {
        *width = VecWidth::V256;
    }

    let mut invalid_address = base.clone();
    if let OpKind::VLoad { addr, .. } = &mut invalid_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
    }

    let mut wrong_pc = base.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;

    let mut previous_boundary = base.clone();
    previous_boundary.blocks[0].ops.insert(
        0,
        SmirOp::new(
            OpId(2),
            PC,
            OpKind::VMov {
                dst: vector(4),
                src: vector(5),
                width: VecWidth::V128,
            },
        ),
    );

    let mut next_boundary = base.clone();
    next_boundary.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::VMov {
            dst: vector(4),
            src: vector(5),
            width: VecWidth::V128,
        },
    ));

    let mut wrong_source = base.clone();
    if let OpKind::X86Phminposuw { src, .. } = &mut wrong_source.blocks[0].ops[1].kind {
        *src = vector(2);
    }

    let mut high_destination = base.clone();
    if let OpKind::X86Phminposuw { dst, .. } = &mut high_destination.blocks[0].ops[1].kind {
        *dst = vector(16);
    }

    let mut wrong_namespace = base.clone();
    if let OpKind::X86Phminposuw { dst, .. } = &mut wrong_namespace.blocks[0].ops[1].kind {
        *dst = x86(X86Reg::Ymm(3));
    }

    let mut wrong_consumer = base.clone();
    wrong_consumer.blocks[0].ops[1].kind = OpKind::VMov {
        dst: vector(3),
        src: temporary,
        width: VecWidth::V128,
    };

    let mut no_consumer_hint = base.clone();
    no_consumer_hint.blocks[0].ops[1].x86_hint = None;

    let mut wrong_map = base.clone();
    wrong_map.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0x41,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_prefix = base.clone();
    wrong_prefix.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::Rep,
        opcode: 0x41,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_opcode = base.clone();
    wrong_opcode.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x40,
        width: VecWidth::V128,
        w: false,
    });

    let mut wrong_hint_width = base.clone();
    wrong_hint_width.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x41,
        width: VecWidth::V256,
        w: false,
    });

    let mut wrong_hint_w = base.clone();
    wrong_hint_w.blocks[0].ops[1].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x41,
        width: VecWidth::V128,
        w: true,
    });

    let mut evex_hint = base.clone();
    evex_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x41,
        width: VecWidth::V128,
        w: false,
    });

    let mut missing_bytes = base.clone();
    missing_bytes.x86_instruction_bytes.clear();

    let mut byte_destination_mismatch = base.clone();
    let other_destination = PhminposuwMemoryCase {
        destination: 4,
        ..case
    };
    byte_destination_mismatch.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&other_destination.bytes()).unwrap(),
    );

    let mut byte_w_mismatch = base.clone();
    let other_w = PhminposuwMemoryCase { w: true, ..case };
    byte_w_mismatch.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&other_w.bytes()).unwrap(),
    );

    let mut register_bytes = base.clone();
    register_bytes.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0xC4, 0xE2, 0x79, 0x41, 0xD8]).unwrap(),
    );

    let malformed = [
        ("temporary used twice", extra_use),
        ("temporary defined twice", extra_definition),
        ("load destination is architectural", nonvirtual_load),
        ("load alignment hint missing", no_load_hint),
        ("load carries an encoding hint", wrong_load_hint),
        ("load width is 256 bits", load_width),
        ("virtual address component", invalid_address),
        ("different guest PCs", wrong_pc),
        ("same-PC operation precedes pair", previous_boundary),
        ("same-PC operation follows pair", next_boundary),
        ("consumer bypasses temporary", wrong_source),
        ("high EVEX-only destination", high_destination),
        ("destination register namespace mismatch", wrong_namespace),
        ("wrong consumer operation", wrong_consumer),
        ("missing VEX hint", no_consumer_hint),
        ("wrong VEX map", wrong_map),
        ("wrong mandatory prefix", wrong_prefix),
        ("wrong opcode", wrong_opcode),
        ("wrong hint width", wrong_hint_width),
        ("encoded and hinted W mismatch", wrong_hint_w),
        ("EVEX consumer", evex_hint),
        ("missing instruction-byte provenance", missing_bytes),
        ("encoded destination mismatch", byte_destination_mismatch),
        ("encoded W mismatch", byte_w_mismatch),
        ("register-form provenance", register_bytes),
    ];
    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    let mut aligned = base;
    aligned.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    let _ = lower(&aligned);
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

fn source_vector(case: PhminposuwMemoryCase) -> [u64; 8] {
    let mut bytes = [0xA5; 64];
    let minimum = match ((case.destination / 4) + u8::from(case.w)) & 3 {
        0 => 0x0000u16,
        1 => 0x0001,
        2 => 0x7FFF,
        _ => 0x8000,
    };
    let minimum_position = usize::from(case.destination & 7);
    for lane in 0..8 {
        let value = if lane & 1 == 0 {
            0xFFFFu16
        } else {
            0x9000 + lane as u16
        };
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes[minimum_position * 2..minimum_position * 2 + 2].copy_from_slice(&minimum.to_le_bytes());
    if case.destination & 1 == 0 && minimum_position < 7 {
        bytes[(minimum_position + 1) * 2..(minimum_position + 2) * 2]
            .copy_from_slice(&minimum.to_le_bytes());
    }
    bytes_to_words(bytes)
}

fn independent_result(source: [u64; 8]) -> u64 {
    let bytes = words_to_bytes(source);
    let mut minimum = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
    let mut position = 0u64;
    for lane in 1..8usize {
        let candidate = u16::from_le_bytes(bytes[lane * 2..lane * 2 + 2].try_into().unwrap());
        if candidate < minimum {
            minimum = candidate;
            position = lane as u64;
        }
    }
    u64::from(minimum) | (position << 16)
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
        || size != 16
        || zero_upper != 1
    {
        return 0;
    }

    let mut value = [0; 8];
    value[..2].copy_from_slice(&context.value[..2]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: PhminposuwMemoryCase, ordinal: usize) -> GuestRegs {
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
    registers.gpr[usize::from(case.base())] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: PhminposuwMemoryCase,
    source: [u64; 8],
) -> GuestRegs {
    registers.zmm[usize::from(case.destination)] =
        [independent_result(source), 0, 0, 0, 0, 0, 0, 0];
    registers.vector_scratch = [source[0], source[1], 0, 0, 0, 0, 0, 0];
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source: [u64; 8],
    address: u64,
    case: PhminposuwMemoryCase,
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
    let bytes = words_to_bytes(source);
    memory.load(address as usize, &bytes[..16]);
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
fn native_vphminposuw_matches_unsigned_first_tie_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VPHMINPOSUW memory differential: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let source = source_vector(case);

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
            let address = registers.gpr[usize::from(case.base())].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, source);

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
            assert_eq!(context.last_size, 16, "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function, &initial, &expected, source, address, case, level,
            );
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
            assert_eq!(context.last_size, 16, "fault {level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }

    assert_eq!(expected_executions, 64);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VPHMINPOSUW memory cases"
    );
}
