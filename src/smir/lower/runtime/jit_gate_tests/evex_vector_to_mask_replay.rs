//! Native replay coverage for register-only EVEX vector-to-opmask conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x3800;
type VectorToMaskShape = (u8, bool, u8);

fn vector_to_mask_shapes() -> Vec<VectorToMaskShape> {
    let mut shapes = Vec::new();
    for opcode in [0x29, 0x39] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn vector_to_mask_encoding(shape: VectorToMaskShape, destination: u8, source: u8) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(destination < 8 && source < 32);
    let mut p0 = 0xF2;
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7E | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08,
        opcode,
        0xC0 | (destination << 3) | (source & 0x07),
    ]
}

fn vector_to_mask_function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn vector_to_mask_replay_admits_and_emits_48_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = vector_to_mask_shapes();
    assert_eq!(shapes.len(), 12);
    assert_eq!(
        vector_to_mask_encoding((0x39, true, 2), 3, 16),
        [0x62, 0xB2, 0xFE, 0x48, 0x39, 0xD8]
    );

    let mut admitted = 0usize;
    for shape in shapes {
        for (destination, source) in [(1, 0), (2, 8), (3, 16), (4, 24)] {
            let bytes = vector_to_mask_encoding(shape, destination, source);
            let (needs_vl, needs_dq) = crate::smir::ir::X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_vector_to_mask_requirements()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            let mut function = vector_to_mask_function(&bytes);
            if admitted == 0 {
                let mut missing_provenance = function.clone();
                missing_provenance.x86_instruction_bytes.clear();
                crate::smir::optimize::optimize_function(
                    &mut missing_provenance,
                    crate::smir::optimize::OptLevel::O2,
                );
                assert!(!is_native_clobber_safe(&missing_provenance));
            }

            crate::smir::optimize::optimize_function(
                &mut function,
                crate::smir::optimize::OptLevel::O2,
            );
            assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{bytes:02X?}"
            );

            #[cfg(target_arch = "x86_64")]
            let expected_features = std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl"))
                && (!needs_dq || std::is_x86_feature_detected!("avx512dq"));
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
            assert!(
                code.windows(bytes.len()).any(|window| window == bytes),
                "{bytes:02X?}"
            );
            admitted += 1;
        }

        // A fabricated memory provenance record must not convert the semantic
        // vector extraction sequence into raw native replay.
        let register = vector_to_mask_encoding(shape, 1, 0);
        let mut memory = register;
        memory[5] &= 0x3F;
        let mut memory_metadata = vector_to_mask_function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");
    }
    assert_eq!(admitted, 48);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct VectorToMaskState {
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret_vector_to_mask(bytes: &[u8], initial: &VectorToMaskState) -> VectorToMaskState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = vector_to_mask_function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
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
    VectorToMaskState {
        vectors,
        masks: x86.k,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native_vector_to_mask(bytes: &[u8], initial: &VectorToMaskState) -> VectorToMaskState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = vector_to_mask_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX vector-to-mask replay");
    let mut registers = GuestRegs {
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
    VectorToMaskState {
        vectors,
        masks: registers.k,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_native_vector_to_mask_matches_interpreter(bytes: &[u8], initial: &VectorToMaskState) {
    let interpreted = interpret_vector_to_mask(bytes, initial);
    let native = execute_native_vector_to_mask(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn vector_to_mask_replay_matches_interpreter_for_all_shapes_and_register_buckets() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native vector-to-mask differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_dq = std::is_x86_feature_detected!("avx512dq");
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|lane| {
            0x807F_FF00_0123_FEDCu64.rotate_left((register * 17 + lane * 11) as u32)
                ^ ((register as u64) << 57)
                ^ (lane as u64 * 0x8102_0408_1020_4081)
        })
    });
    let masks = std::array::from_fn(|index| {
        0xA55A_3CC3_F00F_9696u64.rotate_left((index * 9) as u32) ^ (1u64 << index)
    });
    let initial = VectorToMaskState {
        vectors,
        masks,
        mxcsr: 0x1F80,
    };

    let source_buckets = [0u8, 8, 16, 24];
    let mut executed = 0usize;
    for (index, shape) in vector_to_mask_shapes().into_iter().enumerate() {
        if (shape.2 != 2 && !has_vl) || (shape.0 == 0x39 && !has_dq) {
            continue;
        }
        let source = source_buckets[index % source_buckets.len()] + (index as u8 & 0x07);
        let destination = (index % 8) as u8;
        let bytes = vector_to_mask_encoding(shape, destination, source);
        assert_native_vector_to_mask_matches_interpreter(&bytes, &initial);
        executed += 1;
    }
    let shapes_per_opcode = if has_vl { 6 } else { 2 };
    assert_eq!(executed, shapes_per_opcode * if has_dq { 2 } else { 1 });
}
