//! Native replay coverage for register-only AVX-512F dword/qword permutes.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2000;
type PermuteShape = (u8, u8, bool, u8, bool);

fn avx512f_permute_shapes() -> Vec<PermuteShape> {
    let mut shapes = Vec::new();

    for opcode in [0x16, 0x36] {
        for w in [false, true] {
            for ll in [1, 2] {
                shapes.push((2, opcode, w, ll, false));
            }
        }
    }
    for opcode in [0x76, 0x77, 0x7E, 0x7F] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((2, opcode, w, ll, false));
            }
        }
    }
    for (map, opcode, w, immediate) in [
        (2, 0x0C, false, false),
        (2, 0x0D, true, false),
        (3, 0x04, false, true),
        (3, 0x05, true, true),
    ] {
        for ll in 0..=2 {
            shapes.push((map, opcode, w, ll, immediate));
        }
    }
    for opcode in [0x00, 0x01] {
        for ll in [1, 2] {
            shapes.push((3, opcode, true, ll, true));
        }
    }
    shapes
}

fn generated_permute_encoding(shape: PermuteShape, rm: u8) -> Vec<u8> {
    let (map, opcode, w, ll, immediate) = shape;
    let mut p0 = 0xF0 | map;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    let p1 = 0x7D | if w { 0x80 } else { 0 };
    let p2 = (ll << 5) | 0x09;
    let mut bytes = vec![0x62, p0, p1, p2, opcode, 0xC8 | (rm & 0x07)];
    if immediate {
        bytes.push(3);
    }
    bytes
}

fn native_permute_encoding(shape: PermuteShape, vvvv: u8, rm: u8, zeroing: bool) -> Vec<u8> {
    let (map, opcode, w, ll, immediate) = shape;
    assert!(vvvv < 16 && rm < 8);
    let p1 = if immediate {
        0x7D
    } else {
        (((!vvvv) & 0x0F) << 3) | 0x05
    } | if w { 0x80 } else { 0 };
    let p2 = (if zeroing { 0x80 } else { 0 }) | (ll << 5) | 0x09;
    let mut bytes = vec![0x62, 0xF0 | map, p1, p2, opcode, 0xC8 | rm];
    if immediate {
        bytes.push(3);
    }
    bytes
}

fn permute_function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn avx512f_permute_replay_admits_and_emits_192_generated_register_forms() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = avx512f_permute_shapes();
    assert_eq!(shapes.len(), 48);
    let mut admitted = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_permute_encoding(shape, rm);
            let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_avx512f_permute_needs_vl()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            let mut function = permute_function(&bytes);
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

        let register = generated_permute_encoding(shape, 0);
        let mut memory = register.clone();
        memory[5] = 0x08;
        let mut memory_metadata = permute_function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");
    }
    assert_eq!(admitted, 192);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PermuteState {
    vectors: [[u64; 8]; 5],
    mask: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret_permute(bytes: &[u8], initial: &PermuteState) -> PermuteState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = permute_function(bytes);
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
    let mut vectors = [[0u64; 8]; 5];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    PermuteState {
        vectors,
        mask: x86.k[1],
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native_permute(bytes: &[u8], initial: &PermuteState) -> PermuteState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = permute_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map AVX-512F permute replay");
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

    let mut vectors = [[0u64; 8]; 5];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    PermuteState {
        vectors,
        mask: registers.k[1],
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_native_permute_matches_interpreter(bytes: &[u8], initial: &PermuteState) {
    let interpreted = interpret_permute(bytes, initial);
    let native = execute_native_permute(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn avx512f_permute_replay_matches_interpreter_for_widths_masks_aliases_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native permute differential: host lacks AVX-512F/BW state");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|lane| {
            0x1020_3040_5060_7080u64.rotate_left((register * 11 + lane * 7) as u32)
                ^ ((register as u64) << 56)
                ^ (lane as u64 * 0x0101_0101_0101_0101)
        })
    });
    let initial = PermuteState {
        vectors,
        mask: 0xA55A_3CC3_F00F_9696,
        mxcsr: 0x1F80,
    };

    let mut executed = 0usize;
    for (index, shape) in avx512f_permute_shapes().into_iter().enumerate() {
        if shape.3 != 2 && !has_vl {
            continue;
        }
        let bytes = native_permute_encoding(shape, 2, 3, index % 2 == 0);
        assert_native_permute_matches_interpreter(&bytes, &initial);
        executed += 1;
    }

    // Explicit destination/source aliasing stresses the destructive table and
    // index semantics of VPERMI2*, VPERMT2*, VPERMIL*, and immediate VPERMQ.
    for bytes in [
        native_permute_encoding((2, 0x76, false, 2, false), 1, 1, false),
        native_permute_encoding((2, 0x7F, true, 2, false), 1, 1, true),
        native_permute_encoding((2, 0x0C, false, 2, false), 1, 1, false),
        native_permute_encoding((3, 0x00, true, 2, true), 0, 1, true),
    ] {
        assert_native_permute_matches_interpreter(&bytes, &initial);
        executed += 1;
    }
    assert!(executed >= 22);
}
