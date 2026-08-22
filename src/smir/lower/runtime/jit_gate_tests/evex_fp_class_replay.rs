//! Native replay coverage for register-only EVEX VFPCLASS*.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x4000;
type FpClassShape = (u8, u8, bool, u8);

fn shapes() -> Vec<FpClassShape> {
    let mut shapes = Vec::new();
    for (pp, w) in [(0, false), (1, false), (1, true)] {
        for ll in 0u8..=2 {
            shapes.push((0x66, pp, w, ll));
        }
        for ll in 0u8..=3 {
            shapes.push((0x67, pp, w, ll));
        }
    }
    shapes
}

fn requirements(shape: FpClassShape) -> (bool, bool, bool) {
    let (opcode, pp, _, ll) = shape;
    (opcode == 0x66 && ll != 2, pp == 1, pp == 0)
}

fn encoding(shape: FpClassShape, destination: u8, source: u8, mask: u8, immediate: u8) -> [u8; 7] {
    let (opcode, pp, w, ll) = shape;
    assert!(matches!(opcode, 0x66 | 0x67));
    assert!(destination < 8 && source < 32 && mask < 8);
    assert!(opcode == 0x67 || ll < 3);
    let mut p0 = 0xF3;
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask,
        opcode,
        0xC0 | (destination << 3) | (source & 0x07),
        immediate,
    ]
}

fn expected_replay(bytes: [u8; 7]) -> [u8; 7] {
    let instruction = crate::smir::ir::X86InstructionBytes::new(&bytes).unwrap();
    let replay = instruction
        .evex_scalar_fp_class_llig_canonical_ll0()
        .unwrap_or(instruction);
    replay.as_slice().try_into().unwrap()
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn replay_admits_and_emits_756_legal_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    assert_eq!(
        encoding((0x66, 1, true, 2), 7, 26, 2, 0xFF),
        [0x62, 0x93, 0xFD, 0x4A, 0x66, 0xFA, 0xFF]
    );
    let operands = [(1u8, 2u8), (2, 10), (3, 18), (4, 26)];
    let mut admitted = 0usize;
    let mut missing_provenance_checked = false;
    for shape in shapes() {
        for (destination, source) in operands {
            for mask in [0u8, 1, 2] {
                for immediate in [0u8, 0xD1, 0xFF] {
                    let bytes = encoding(shape, destination, source, mask, immediate);
                    let (needs_vl, needs_dq, needs_fp16) =
                        crate::smir::ir::X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_fp_class_requirements()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    let mut function = function(&bytes);
                    if !missing_provenance_checked && mask != 0 {
                        let mut missing_provenance = function.clone();
                        missing_provenance.x86_instruction_bytes.clear();
                        crate::smir::optimize::optimize_function(
                            &mut missing_provenance,
                            crate::smir::optimize::OptLevel::O2,
                        );
                        assert!(!is_native_clobber_safe(&missing_provenance));
                        missing_provenance_checked = true;
                    }

                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );
                    assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
                        "{bytes:02X?}"
                    );

                    #[cfg(target_arch = "x86_64")]
                    let expected_features = std::is_x86_feature_detected!("avx512f")
                        && std::is_x86_feature_detected!("avx512bw")
                        && (!needs_vl || std::is_x86_feature_detected!("avx512vl"))
                        && (!needs_dq || std::is_x86_feature_detected!("avx512dq"))
                        && (!needs_fp16 || std::is_x86_feature_detected!("avx512fp16"));
                    #[cfg(not(target_arch = "x86_64"))]
                    let expected_features = false;
                    assert_eq!(
                        x86_native_vector_features_supported_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
                        expected_features,
                        "{bytes:02X?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer
                        .lower_function(&function)
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    let code = lowerer
                        .finalize()
                        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                    let replay = expected_replay(bytes);
                    assert!(
                        code.windows(replay.len()).any(|window| window == replay),
                        "guest={bytes:02X?} replay={replay:02X?}"
                    );
                    admitted += 1;
                }
            }
        }

        let register = encoding(shape, 1, 2, 1, 0xFF);
        let mut unsafe_encodings = Vec::new();
        let mut memory = register;
        memory[5] &= 0x3F;
        unsafe_encodings.push(memory);
        let mut embedded_broadcast = register;
        embedded_broadcast[3] |= 0x10;
        unsafe_encodings.push(embedded_broadcast);
        let mut reserved_zeroing = register;
        reserved_zeroing[3] |= 0x80;
        unsafe_encodings.push(reserved_zeroing);
        let mut reserved_vvvv = register;
        reserved_vvvv[2] &= !0x08;
        unsafe_encodings.push(reserved_vvvv);
        let mut reserved_v_prime = register;
        reserved_v_prime[3] &= !0x08;
        unsafe_encodings.push(reserved_v_prime);
        let mut extended_destination_r = register;
        extended_destination_r[1] &= !0x80;
        unsafe_encodings.push(extended_destination_r);
        let mut extended_destination_r_prime = register;
        extended_destination_r_prime[1] &= !0x10;
        unsafe_encodings.push(extended_destination_r_prime);
        if shape.0 == 0x66 {
            let mut reserved_ll = register;
            reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
            unsafe_encodings.push(reserved_ll);
        }

        for unsafe_encoding in unsafe_encodings {
            let mut unsafe_metadata = function(&register);
            unsafe_metadata.x86_instruction_bytes.insert(
                (BlockId(0), PC),
                crate::smir::ir::X86InstructionBytes::new(&unsafe_encoding).unwrap(),
            );
            assert!(
                !is_native_clobber_safe(&unsafe_metadata),
                "{unsafe_encoding:02X?}"
            );
        }
    }
    assert!(missing_provenance_checked);
    assert_eq!(admitted, 756);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct FpClassState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
const F16_PATTERNS: [u64; 10] = [
    0x7E01, 0x0000, 0x8000, 0x7C00, 0xFC00, 0x0001, 0x8001, 0xBC00, 0x7C01, 0x3C00,
];
#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u64; 10] = [
    0x7FC0_0001,
    0x0000_0000,
    0x8000_0000,
    0x7F80_0000,
    0xFF80_0000,
    0x0000_0001,
    0x8000_0001,
    0xBF80_0000,
    0x7F80_0001,
    0x3F80_0000,
];
#[cfg(target_arch = "x86_64")]
const F64_PATTERNS: [u64; 10] = [
    0x7FF8_0000_0000_0001,
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x8000_0000_0000_0001,
    0xBFF0_0000_0000_0000,
    0x7FF0_0000_0000_0001,
    0x3FF0_0000_0000_0000,
];

#[cfg(target_arch = "x86_64")]
fn patterned_vector(register: usize) -> [u64; 8] {
    let (element_size, patterns): (usize, &[u64]) = match register % 3 {
        0 => (2, &F16_PATTERNS),
        1 => (4, &F32_PATTERNS),
        _ => (8, &F64_PATTERNS),
    };
    let mut bytes = [0u8; 64];
    for lane in 0..64 / element_size {
        let value = patterns[(lane + register) % patterns.len()].to_le_bytes();
        let base = lane * element_size;
        bytes[base..base + element_size].copy_from_slice(&value[..element_size]);
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &FpClassState) -> FpClassState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
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
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    FpClassState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &FpClassState) -> FpClassState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let replay = expected_replay(bytes.try_into().unwrap());
    assert!(
        code.windows(replay.len()).any(|window| window == replay),
        "guest={bytes:02X?} replay={replay:02X?}"
    );
    let exec = ExecMem::new(&code).expect("map EVEX FPCLASS replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: 1,
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
    FpClassState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_shapes_extensions_masks_classes_and_daz() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX FPCLASS differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_dq = std::is_x86_feature_detected!("avx512dq");
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    if !has_dq && !has_fp16 {
        eprintln!("skipping native EVEX FPCLASS differential: host lacks AVX-512DQ/FP16");
        return;
    }
    let baseline = FpClassState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64 * 0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(patterned_vector),
        masks: std::array::from_fn(|index| {
            0xA55A_3CC3_F00F_9696u64.rotate_left((index * 9) as u32) ^ (1u64 << index)
        }),
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    let operands = [
        (1u8, 2u8),
        (2, 10),
        (3, 18),
        (4, 26),
        (1, 1),
        (7, 31),
        (2, 2),
    ];

    let mut executed = 0usize;
    let mut available_shapes = 0usize;
    for shape in shapes() {
        let (needs_vl, needs_dq, needs_fp16) = requirements(shape);
        if (needs_vl && !has_vl) || (needs_dq && !has_dq) || (needs_fp16 && !has_fp16) {
            continue;
        }
        available_shapes += 1;
        for (destination, source) in operands {
            for mask in [0u8, 1, 2] {
                for immediate in [0u8, 0xD1, 0xFF] {
                    for daz in [false, true] {
                        let bytes = encoding(shape, destination, source, mask, immediate);
                        let mut initial = baseline.clone();
                        if daz {
                            initial.mxcsr |= 1 << 6;
                        }
                        assert_eq!(
                            execute_native(&bytes, &initial),
                            interpret(&bytes, &initial),
                            "{bytes:02X?} DAZ={daz}"
                        );
                        executed += 1;
                    }
                }
            }
        }
    }
    assert!(available_shapes > 0, "feature-selected FPCLASS shapes");
    assert_eq!(executed, available_shapes * 126);
}
