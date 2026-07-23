//! Native replay coverage for register-only EVEX packed moves.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x3000;
type PackedMoveShape = (u8, u8, bool, u8);

fn packed_move_shapes() -> Vec<PackedMoveShape> {
    let mut shapes = Vec::new();

    for opcode in [0x10, 0x11, 0x28, 0x29] {
        for (pp, w) in [(0, false), (1, true)] {
            for ll in 0..=2 {
                shapes.push((opcode, pp, w, ll));
            }
        }
    }
    for opcode in [0x6F, 0x7F] {
        for pp in 1..=3 {
            for w in [false, true] {
                for ll in 0..=2 {
                    shapes.push((opcode, pp, w, ll));
                }
            }
        }
    }
    shapes
}

fn generated_move_encoding(shape: PackedMoveShape, rm: u8) -> [u8; 6] {
    let (opcode, pp, w, ll) = shape;
    let mut p0 = 0xF1;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x09,
        opcode,
        0xC8 | (rm & 0x07),
    ]
}

fn native_move_encoding(
    shape: PackedMoveShape,
    reg: u8,
    rm: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (opcode, pp, w, ll) = shape;
    assert!(reg < 32 && rm < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF1;
    if reg & 0x08 != 0 {
        p0 &= !0x80;
    }
    if reg & 0x10 != 0 {
        p0 &= !0x10;
    }
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 }) | (ll << 5) | 0x08 | mask,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
    ]
}

fn move_function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn packed_move_replay_admits_and_emits_240_generated_register_forms() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = packed_move_shapes();
    assert_eq!(shapes.len(), 60);
    assert_eq!(
        native_move_encoding((0x10, 0, false, 2), 25, 26, 1, true),
        [0x62, 0x01, 0x7C, 0xC9, 0x10, 0xCA]
    );
    let mut admitted = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_move_encoding(shape, rm);
            let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_packed_move_needs_vl()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            let mut function = move_function(&bytes);
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

        let register = generated_move_encoding(shape, 0);
        let mut memory = register;
        memory[5] = 0x08;
        let mut memory_metadata = move_function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");
    }
    assert_eq!(admitted, 240);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MoveState {
    vectors: [[u64; 8]; 32],
    mask: u64,
    mxcsr: u32,
}

fn interpret_move(bytes: &[u8], initial: &MoveState) -> MoveState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = move_function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k[1] = initial.mask;
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
    MoveState {
        vectors,
        mask: x86.k[1],
        mxcsr: x86.mxcsr,
    }
}

fn packed_move_element_bits((opcode, pp, w, _): PackedMoveShape) -> u32 {
    match (opcode, pp, w) {
        (0x10 | 0x11 | 0x28 | 0x29, 0, false) => 32,
        (0x10 | 0x11 | 0x28 | 0x29, 1, true) => 64,
        (0x6F | 0x7F, 3, false) => 8,
        (0x6F | 0x7F, 3, true) => 16,
        (0x6F | 0x7F, 1 | 2, false) => 32,
        (0x6F | 0x7F, 1 | 2, true) => 64,
        _ => unreachable!(),
    }
}

fn packed_move_lane(vector: &[u64; 8], lane: usize, element_bits: u32) -> u64 {
    let bit = lane * element_bits as usize;
    let mask = if element_bits == 64 {
        u64::MAX
    } else {
        (1u64 << element_bits) - 1
    };
    (vector[bit / 64] >> (bit % 64)) & mask
}

fn set_packed_move_lane(vector: &mut [u64; 8], lane: usize, element_bits: u32, value: u64) {
    let bit = lane * element_bits as usize;
    let mask = if element_bits == 64 {
        u64::MAX
    } else {
        (1u64 << element_bits) - 1
    };
    let shift = bit % 64;
    vector[bit / 64] = (vector[bit / 64] & !(mask << shift)) | ((value & mask) << shift);
}

fn expected_packed_move(
    shape: PackedMoveShape,
    reg: u8,
    rm: u8,
    encoded_mask: u8,
    zeroing: bool,
    initial: &MoveState,
) -> MoveState {
    let opcode = shape.0;
    let vector_bits = 128usize << shape.3;
    let element_bits = packed_move_element_bits(shape);
    let lanes = vector_bits / element_bits as usize;
    let (dst, src) = if matches!(opcode, 0x10 | 0x28 | 0x6F) {
        (reg as usize, rm as usize)
    } else {
        (rm as usize, reg as usize)
    };
    let old_dst = initial.vectors[dst];
    let source = initial.vectors[src];
    let mut expected = initial.clone();
    expected.vectors[dst] = [0; 8];
    for lane in 0..lanes {
        let active = encoded_mask == 0 || (initial.mask >> lane) & 1 != 0;
        let value = if active {
            packed_move_lane(&source, lane, element_bits)
        } else if zeroing {
            0
        } else {
            packed_move_lane(&old_dst, lane, element_bits)
        };
        set_packed_move_lane(&mut expected.vectors[dst], lane, element_bits, value);
    }
    expected
}

#[test]
fn packed_move_interpretation_matches_manual_for_all_widths_masks_directions_and_aliases() {
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 13 + word * 9) as u32)
                ^ ((register as u64) << 60)
                ^ (word as u64 * 0x1111_1111_1111_1111)
        })
    });
    let initial = MoveState {
        vectors,
        mask: 0xA55A_3CC3_F00F_9696,
        mxcsr: 0x1F80,
    };

    let mut executed = 0usize;
    for shape in packed_move_shapes() {
        for (reg, rm) in [(2, 3), (25, 26), (2, 2)] {
            for (mask, zeroing) in [(0, false), (1, false), (1, true)] {
                let bytes = native_move_encoding(shape, reg, rm, mask, zeroing);
                let interpreted = interpret_move(&bytes, &initial);
                let expected = expected_packed_move(shape, reg, rm, mask, zeroing, &initial);
                assert_eq!(interpreted, expected, "{bytes:02X?}");
                executed += 1;
            }
        }
    }
    assert_eq!(executed, 540);
}

#[cfg(target_arch = "x86_64")]
fn execute_native_move(bytes: &[u8], initial: &MoveState) -> MoveState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = move_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX packed-move replay");
    let mut registers = GuestRegs {
        vector_active: 1,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    registers.k[1] = initial.mask;
    exec.run(lowered.entry_offset, &mut registers);

    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    MoveState {
        vectors,
        mask: registers.k[1],
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_native_move_matches_interpreter(bytes: &[u8], initial: &MoveState) {
    let interpreted = interpret_move(bytes, initial);
    let native = execute_native_move(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn packed_move_replay_matches_interpreter_for_widths_masks_directions_and_aliases() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed-move differential: host lacks AVX-512F/BW state");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|lane| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 13 + lane * 9) as u32)
                ^ ((register as u64) << 60)
                ^ (lane as u64 * 0x1111_1111_1111_1111)
        })
    });
    let initial = MoveState {
        vectors,
        mask: 0xA55A_3CC3_F00F_9696,
        mxcsr: 0x1F80,
    };

    let mut executed = 0usize;
    for (index, shape) in packed_move_shapes().into_iter().enumerate() {
        if shape.3 != 2 && !has_vl {
            continue;
        }
        let bytes = native_move_encoding(shape, 2, 3, 1, index % 2 == 0);
        assert_native_move_matches_interpreter(&bytes, &initial);
        executed += 1;
    }

    // Source/destination aliasing stresses both opcode directions under merge,
    // zeroing, and unmasked operation. ZMM25/ZMM26 cases exercise every EVEX
    // register-extension channel without altering nonoperand ZMM state.
    for bytes in [
        native_move_encoding((0x10, 0, false, 2), 2, 2, 1, false),
        native_move_encoding((0x29, 1, true, 2), 3, 3, 1, true),
        native_move_encoding((0x7F, 2, true, 2), 2, 2, 0, false),
        native_move_encoding((0x10, 0, false, 2), 25, 26, 1, true),
        native_move_encoding((0x7F, 3, true, 2), 25, 26, 1, false),
    ] {
        assert_native_move_matches_interpreter(&bytes, &initial);
        executed += 1;
    }
    assert!(executed >= 25);
}
