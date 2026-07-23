//! Native replay coverage for register-only EVEX one-source lane shuffles.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x3900;
type LaneShuffleShape = (u8, u8, bool, u8, bool);

fn lane_shuffle_shapes() -> Vec<LaneShuffleShape> {
    let mut shapes = Vec::new();
    for (opcode, pp, w) in [(0x12, 2, false), (0x16, 2, false), (0x12, 3, true)] {
        for ll in 0..=2 {
            shapes.push((opcode, pp, w, ll, false));
        }
    }
    for (pp, widths) in [
        (1, &[false][..]),
        (2, &[false, true][..]),
        (3, &[false, true][..]),
    ] {
        for &w in widths {
            for ll in 0..=2 {
                shapes.push((0x70, pp, w, ll, true));
            }
        }
    }
    shapes
}

fn lane_shuffle_encoding(
    shape: LaneShuffleShape,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> Vec<u8> {
    let (opcode, pp, w, ll, has_immediate) = shape;
    assert!(destination < 32 && source < 32 && mask < 8 && (!zeroing || mask != 0));
    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x10;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x80;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    let mut bytes = vec![
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ];
    if has_immediate {
        bytes.push(immediate);
    }
    bytes
}

fn lane_shuffle_function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn lane_shuffle_replay_admits_and_emits_96_legal_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = lane_shuffle_shapes();
    assert_eq!(shapes.len(), 24);
    assert_eq!(
        lane_shuffle_encoding((0x70, 2, true, 1, true), 24, 31, 2, true, 0xB1),
        [0x62, 0x01, 0xFE, 0xAA, 0x70, 0xC7, 0xB1]
    );

    let mut admitted = 0usize;
    for shape in shapes {
        for bucket in [0, 8, 16, 24] {
            let bytes = lane_shuffle_encoding(shape, 1 + bucket, bucket, 1, true, 0x93);
            let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_lane_shuffle_needs_vl()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            let mut function = lane_shuffle_function(&bytes);
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
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl"));
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

        // Fabricated memory provenance must never replace the explicit SMIR
        // load/fault sequence with host memory access.
        let register = lane_shuffle_encoding(shape, 1, 0, 1, false, 0x1B);
        let mut memory = register.clone();
        memory[5] &= 0x3F;
        let mut memory_metadata = lane_shuffle_function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");
    }
    assert_eq!(admitted, 96);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct LaneShuffleState {
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret_lane_shuffle(bytes: &[u8], initial: &LaneShuffleState) -> LaneShuffleState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = lane_shuffle_function(bytes);
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
    LaneShuffleState {
        vectors,
        masks: x86.k,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native_lane_shuffle(bytes: &[u8], initial: &LaneShuffleState) -> LaneShuffleState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = lane_shuffle_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX lane-shuffle replay");
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
    LaneShuffleState {
        vectors,
        masks: registers.k,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_native_lane_shuffle_matches_interpreter(bytes: &[u8], initial: &LaneShuffleState) {
    let interpreted = interpret_lane_shuffle(bytes, initial);
    let native = execute_native_lane_shuffle(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn lane_shuffle_replay_matches_interpreter_for_wig_masks_immediates_and_aliases() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native lane-shuffle differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|lane| {
            0x807F_FF00_0123_FEDCu64.rotate_left((register * 17 + lane * 11) as u32)
                ^ ((register as u64) << 57)
                ^ (lane as u64 * 0x0102_0408_1020_4081)
        })
    });
    let masks = std::array::from_fn(|index| {
        0xA55A_3CC3_F00F_9696u64.rotate_left((index * 7) as u32) ^ (1u64 << index)
    });
    let initial = LaneShuffleState {
        vectors,
        masks,
        mxcsr: 0x1F80,
    };

    let immediates = [0x00, 0x1B, 0x4E, 0x93, 0xB1, 0xE4, 0xFF];
    let mut executed = 0usize;
    for (index, shape) in lane_shuffle_shapes().into_iter().enumerate() {
        if shape.3 != 2 && !has_vl {
            continue;
        }
        let source = [2, 10, 18, 26][index % 4];
        let destination = if index % 4 == 0 {
            source
        } else {
            [1, 9, 17, 25][index % 4]
        };
        let (mask, zeroing) = match index % 3 {
            0 => (0, false),
            1 => (1, false),
            _ => (2, true),
        };
        let bytes = lane_shuffle_encoding(
            shape,
            destination,
            source,
            mask,
            zeroing,
            immediates[index % immediates.len()],
        );
        assert_native_lane_shuffle_matches_interpreter(&bytes, &initial);
        executed += 1;
    }
    assert_eq!(executed, if has_vl { 24 } else { 8 });
}
