//! Native replay coverage for register-only fixed-predicate EVEX packed compares.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2F00;
type FixedPackedCompareShape = (u8, u8, bool, u8);

fn fixed_packed_compare_shapes() -> Vec<FixedPackedCompareShape> {
    let mut shapes = Vec::new();
    for opcode in [0x64, 0x65, 0x74, 0x75] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((1, opcode, w, ll));
            }
        }
    }
    for (map, opcode, w) in [
        (1, 0x66, false),
        (1, 0x76, false),
        (2, 0x29, true),
        (2, 0x37, true),
    ] {
        for ll in 0..=2 {
            shapes.push((map, opcode, w, ll));
        }
    }
    shapes
}

fn fixed_packed_compare_encoding(
    shape: FixedPackedCompareShape,
    dst: u8,
    src1: u8,
    src2: u8,
    writemask: u8,
) -> [u8; 6] {
    let (map, opcode, w, ll) = shape;
    assert!(dst < 8 && src1 < 32 && src2 < 32 && writemask < 8);
    let mut p0 = 0xF0 | map;
    if src2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if src2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    let p1 = (((!src1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 };
    let p2 = (ll << 5) | if src1 < 16 { 0x08 } else { 0 } | writemask;
    [0x62, p0, p1, p2, opcode, 0xC0 | (dst << 3) | (src2 & 0x07)]
}

fn fixed_packed_compare_function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn fixed_packed_compare_replay_admits_and_emits_144_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = fixed_packed_compare_shapes();
    assert_eq!(shapes.len(), 36);
    assert_eq!(
        fixed_packed_compare_encoding((1, 0x74, false, 2), 5, 25, 26, 1),
        [0x62, 0x91, 0x35, 0x41, 0x74, 0xEA]
    );

    let mut admitted = 0usize;
    for shape in shapes {
        for rm_bucket in [0, 8, 16, 24] {
            let bytes = fixed_packed_compare_encoding(shape, 5, 1 + rm_bucket, rm_bucket, 1);
            let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_packed_compare_needs_vl()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            let mut function = fixed_packed_compare_function(&bytes);
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

        let register = fixed_packed_compare_encoding(shape, 1, 2, 0, 1);
        let mut memory = register;
        memory[5] &= 0x3F;
        let mut memory_metadata = fixed_packed_compare_function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");
    }
    assert_eq!(admitted, 144);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct FixedPackedCompareState {
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret_fixed_packed_compare(
    bytes: &[u8],
    initial: &FixedPackedCompareState,
) -> FixedPackedCompareState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = fixed_packed_compare_function(bytes);
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
    FixedPackedCompareState {
        vectors,
        masks: x86.k,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native_fixed_packed_compare(
    bytes: &[u8],
    initial: &FixedPackedCompareState,
) -> FixedPackedCompareState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = fixed_packed_compare_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX fixed packed-compare replay");
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
    FixedPackedCompareState {
        vectors,
        masks: registers.k,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_native_fixed_packed_compare_matches_interpreter(
    bytes: &[u8],
    initial: &FixedPackedCompareState,
) {
    let interpreted = interpret_fixed_packed_compare(bytes, initial);
    let native = execute_native_fixed_packed_compare(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn fixed_packed_compare_replay_matches_interpreter_for_wig_masks_and_aliases() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native fixed packed-compare differential: host lacks AVX-512F/BW");
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
    let initial = FixedPackedCompareState {
        vectors,
        masks,
        mxcsr: 0x1F80,
    };

    let mut executed = 0usize;
    for (index, shape) in fixed_packed_compare_shapes().into_iter().enumerate() {
        if shape.3 != 2 && !has_vl {
            continue;
        }
        let dst = (index % 8) as u8;
        let writemask = match index % 3 {
            0 => 0,
            1 if dst != 0 => dst,
            _ => (dst + 1) & 7,
        };
        let src1 = if index % 2 == 0 { 25 } else { 2 };
        let src2 = if index % 4 == 0 { src1 } else { 26 };
        let bytes = fixed_packed_compare_encoding(shape, dst, src1, src2, writemask);
        assert_native_fixed_packed_compare_matches_interpreter(&bytes, &initial);
        executed += 1;
    }

    // Destination/writemask aliasing must consume the incoming mask before
    // overwriting it. This also aliases both vector sources in high registers.
    let alias = fixed_packed_compare_encoding((1, 0x74, true, 2), 5, 25, 25, 5);
    assert_native_fixed_packed_compare_matches_interpreter(&alias, &initial);
    assert!(executed >= 12);
}
