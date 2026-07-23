//! Native replay coverage for EVEX VPBROADCASTB/W/D/Q GPR sources.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x3B00;
type GprBroadcastShape = (u8, bool, u8);

fn shapes() -> Vec<GprBroadcastShape> {
    let mut shapes = Vec::new();
    for (opcode, w) in [(0x7A, false), (0x7B, false), (0x7C, false), (0x7C, true)] {
        for ll in 0..=2 {
            shapes.push((opcode, w, ll));
        }
    }
    shapes
}

fn encoding(
    shape: GprBroadcastShape,
    destination: u8,
    source: u8,
    ignored_x: bool,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(destination < 32 && source < 16 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
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
fn replay_admits_and_emits_288_safe_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let shapes = shapes();
    assert_eq!(shapes.len(), 12);
    assert_eq!(
        encoding((0x7C, true, 2), 25, 13, true, 2, true),
        [0x62, 0x02, 0xFD, 0xCA, 0x7C, 0xCD]
    );

    let mut admitted = 0usize;
    for shape in shapes {
        for (destination, source) in [(1, 0), (9, 3), (17, 12), (25, 13)] {
            for ignored_x in [false, true] {
                for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                    let bytes = encoding(shape, destination, source, ignored_x, mask, zeroing);
                    let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_register_gpr_broadcast_needs_vl()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    let mut function = function(&bytes);
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
                        uses_x86_native_vectors_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
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
            }
        }

        let register = encoding(shape, 1, 0, false, 0, false);
        let mut memory = register;
        memory[5] &= 0x3F;
        let mut memory_metadata = function(&register);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{memory:02X?}");

        for unsafe_source in [4u8, 5] {
            let bytes = encoding(shape, 1, unsafe_source, false, 0, false);
            let unsafe_function = function(&bytes);
            assert!(!is_native_clobber_safe(&unsafe_function), "{bytes:02X?}");
        }
    }
    assert_eq!(admitted, 288);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct GprBroadcastState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &GprBroadcastState) -> GprBroadcastState {
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
    GprBroadcastState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &GprBroadcastState) -> GprBroadcastState {
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
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX GPR-broadcast replay");
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
    GprBroadcastState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_all_shapes_extensions_masks_and_ignored_x() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native GPR-broadcast differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let initial = GprBroadcastState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64 * 0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|lane| {
                0x807F_FF00_0123_FEDCu64.rotate_left((register * 17 + lane * 11) as u32)
                    ^ ((register as u64) << 57)
                    ^ (lane as u64 * 0x8102_0408_1020_4081)
            })
        }),
        masks: std::array::from_fn(|index| {
            0xA55A_3CC3_F00F_9696u64.rotate_left((index * 9) as u32) ^ (1u64 << index)
        }),
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };

    let destinations = [1u8, 9, 17, 25];
    let sources = [0u8, 3, 8, 12, 13, 15];
    let mut executed = 0usize;
    for (shape_index, shape) in shapes().into_iter().enumerate() {
        if shape.2 != 2 && !has_vl {
            continue;
        }
        for (destination_index, destination) in destinations.into_iter().enumerate() {
            for ignored_x in [false, true] {
                for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                    let source = sources[(shape_index + destination_index) % sources.len()];
                    let bytes = encoding(shape, destination, source, ignored_x, mask, zeroing);
                    assert_eq!(
                        execute_native(&bytes, &initial),
                        interpret(&bytes, &initial),
                        "{bytes:02X?}"
                    );
                    executed += 1;
                }
            }
        }
    }
    let available_shapes = if has_vl { 12 } else { 4 };
    assert_eq!(executed, available_shapes * 24);
}
