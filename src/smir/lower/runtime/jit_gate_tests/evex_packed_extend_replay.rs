//! Native replay coverage for register-only EVEX packed widening moves.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x3100;
type PackedExtendShape = (u8, bool, u8);

fn packed_extend_shapes() -> Vec<PackedExtendShape> {
    let mut shapes = Vec::new();
    for opcode in (0x20..=0x25).chain(0x30..=0x35) {
        let widths: &[bool] = if matches!(opcode, 0x25 | 0x35) {
            &[false]
        } else {
            &[false, true]
        };
        for &w in widths {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn packed_extend_encoding(
    shape: PackedExtendShape,
    reg: u8,
    rm: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(reg < 32 && rm < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF2;
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
        0x7D | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 }) | (ll << 5) | 0x08 | mask,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
    ]
}

fn packed_extend_function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn packed_extend_replay_admits_and_emits_264_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = packed_extend_shapes();
    assert_eq!(shapes.len(), 66);
    assert_eq!(
        packed_extend_encoding((0x20, false, 2), 25, 26, 1, true),
        [0x62, 0x02, 0x7D, 0xC9, 0x20, 0xCA]
    );

    let mut admitted = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = packed_extend_encoding(shape, 1 + rm, rm, 1, false);
            let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_packed_extend_needs_vl()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            let mut function = packed_extend_function(&bytes);
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

        let register = packed_extend_encoding(shape, 1, 0, 1, false);
        let mut memory = register;
        memory[5] = 0x08;
        let mut memory_metadata = packed_extend_function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");
    }
    assert_eq!(admitted, 264);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PackedExtendState {
    vectors: [[u64; 8]; 32],
    mask: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret_packed_extend(bytes: &[u8], initial: &PackedExtendState) -> PackedExtendState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = packed_extend_function(bytes);
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
    PackedExtendState {
        vectors,
        mask: x86.k[1],
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native_packed_extend(bytes: &[u8], initial: &PackedExtendState) -> PackedExtendState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = packed_extend_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX packed-extend replay");
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
    PackedExtendState {
        vectors,
        mask: registers.k[1],
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_native_packed_extend_matches_interpreter(bytes: &[u8], initial: &PackedExtendState) {
    let interpreted = interpret_packed_extend(bytes, initial);
    let native = execute_native_packed_extend(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn packed_extend_replay_matches_interpreter_for_shapes_wig_masks_and_aliases() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed-extend differential: host lacks AVX-512F/BW state");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|lane| {
            0x80FF_7F01_FFFE_0200u64.rotate_left((register * 11 + lane * 7) as u32)
                ^ ((register as u64) << 59)
                ^ (lane as u64 * 0x0101_0101_0101_0101)
        })
    });
    let initial = PackedExtendState {
        vectors,
        mask: 0xA55A_3CC3_F00F_9696,
        mxcsr: 0x1F80,
    };

    let mut executed = 0usize;
    for (index, shape) in packed_extend_shapes().into_iter().enumerate() {
        if shape.2 != 2 && !has_vl {
            continue;
        }
        let bytes = packed_extend_encoding(shape, 2, 3, 1, index % 2 == 0);
        assert_native_packed_extend_matches_interpreter(&bytes, &initial);
        executed += 1;
    }

    // Aliasing reads the complete narrow source before writing the widened
    // destination. High registers exercise every EVEX extension channel.
    for bytes in [
        packed_extend_encoding((0x20, true, 2), 2, 2, 1, false),
        packed_extend_encoding((0x34, true, 2), 3, 3, 1, true),
        packed_extend_encoding((0x35, false, 2), 2, 2, 0, false),
        packed_extend_encoding((0x22, false, 2), 25, 26, 1, true),
        packed_extend_encoding((0x33, true, 2), 25, 26, 1, false),
    ] {
        assert_native_packed_extend_matches_interpreter(&bytes, &initial);
        executed += 1;
    }
    assert!(executed >= 27);
}
