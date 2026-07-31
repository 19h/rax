//! Exact native replay coverage for register-only EVEX packed rotates.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x45A0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RotateShape {
    variable: bool,
    left: bool,
    quadword: bool,
    ll: u8,
}

fn shapes() -> Vec<RotateShape> {
    let mut shapes = Vec::with_capacity(24);
    for variable in [false, true] {
        for left in [false, true] {
            for quadword in [false, true] {
                for ll in 0..=2 {
                    shapes.push(RotateShape {
                        variable,
                        left,
                        quadword,
                        ll,
                    });
                }
            }
        }
    }
    shapes
}

fn encoding(
    shape: RotateShape,
    destination: u8,
    source: u8,
    count: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> Vec<u8> {
    assert!(destination < 32 && source < 32 && count < 32 && shape.ll < 3);
    assert!(mask < 8 && (!zeroing || mask != 0));

    let map = if shape.variable { 2 } else { 1 };
    let mut p0 = 0xF0 | map;
    let vvvv = if shape.variable { source } else { destination };
    let rm = if shape.variable { count } else { source };
    if shape.variable {
        if destination & 0x08 != 0 {
            p0 &= !0x80;
        }
        if destination & 0x10 != 0 {
            p0 &= !0x10;
        }
    }
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }

    let p1 = (((!vvvv) & 0x0F) << 3) | 0x05 | if shape.quadword { 0x80 } else { 0 };
    let p2 =
        (shape.ll << 5) | if vvvv < 16 { 0x08 } else { 0 } | mask | if zeroing { 0x80 } else { 0 };
    let opcode = if shape.variable {
        if shape.left { 0x15 } else { 0x14 }
    } else {
        0x72
    };
    let reg = if shape.variable {
        destination & 0x07
    } else {
        u8::from(shape.left)
    };
    let mut bytes = vec![0x62, p0, p1, p2, opcode, 0xC0 | (reg << 3) | (rm & 0x07)];
    if !shape.variable {
        bytes.push(immediate);
    }
    bytes
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
fn encoding_builder_matches_llvm_23_register_anchors() {
    assert_eq!(
        encoding(
            RotateShape {
                variable: false,
                left: false,
                quadword: false,
                ll: 0,
            },
            1,
            2,
            0,
            0,
            false,
            0,
        ),
        [0x62, 0xF1, 0x75, 0x08, 0x72, 0xC2, 0x00]
    );
    assert_eq!(
        encoding(
            RotateShape {
                variable: false,
                left: true,
                quadword: false,
                ll: 1,
            },
            17,
            18,
            0,
            7,
            true,
            0xFF,
        ),
        [0x62, 0xB1, 0x75, 0xA7, 0x72, 0xCA, 0xFF]
    );
    assert_eq!(
        encoding(
            RotateShape {
                variable: true,
                left: false,
                quadword: true,
                ll: 2,
            },
            29,
            30,
            31,
            0,
            false,
            0,
        ),
        [0x62, 0x02, 0x8D, 0x40, 0x14, 0xEF]
    );
    assert_eq!(
        encoding(
            RotateShape {
                variable: true,
                left: true,
                quadword: true,
                ll: 0,
            },
            31,
            16,
            17,
            1,
            false,
            0,
        ),
        [0x62, 0x22, 0xFD, 0x01, 0x15, 0xF9]
    );
}

#[test]
fn replay_admits_and_emits_576_legal_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [(1u8, 2u8, 3u8), (9, 10, 11), (17, 18, 19), (25, 26, 27)];
    let mask_modes = [(0u8, false), (1, false), (2, true)];
    let mut admitted = 0usize;
    for shape in shapes() {
        let immediates: &[u8] = if shape.variable {
            &[0]
        } else {
            &[0, 0x3F, 0xFF]
        };
        for (destination, source, count) in operands {
            for (mask, zeroing) in mask_modes {
                for &immediate in immediates {
                    let bytes =
                        encoding(shape, destination, source, count, mask, zeroing, immediate);
                    let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_register_packed_rotate_needs_vl()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    let mut function = function(&bytes);
                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );

                    let spans = crate::smir::ir::x86_native_replay_spans(
                        &function.blocks[0],
                        &function.x86_instruction_bytes,
                    );
                    let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(span.end, function.blocks[0].ops.len(), "{bytes:02X?}");
                    assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
                    assert_eq!(span.needs_avx512vl, needs_vl, "{bytes:02X?}");
                    assert!(!span.needs_avx512dq, "{bytes:02X?}");
                    assert!(!span.needs_avx512fp16, "{bytes:02X?}");
                    assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
                        "{bytes:02X?}"
                    );

                    let requirements = x86_native_replay_feature_requirements(
                        &function,
                        &std::collections::HashMap::new(),
                    );
                    assert!(requirements.any, "{bytes:02X?}");
                    assert!(requirements.needs_avx512bw, "{bytes:02X?}");
                    assert_eq!(requirements.needs_avx512vl, needs_vl, "{bytes:02X?}");
                    assert!(!requirements.needs_avx512dq, "{bytes:02X?}");
                    assert!(!requirements.needs_avx512fp16, "{bytes:02X?}");

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
    }
    assert_eq!(admitted, 576);
}

#[test]
fn replay_preserves_zero_count_rotate_provenance_at_every_optimization_level() {
    let levels = [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ];
    for shape in shapes().into_iter().filter(|shape| !shape.variable) {
        let bytes = encoding(shape, 17, 18, 0, 0, false, 0);
        for level in levels {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
            let spans = crate::smir::ir::x86_native_replay_spans(
                &function.blocks[0],
                &function.x86_instruction_bytes,
            );
            assert_eq!(
                spans.get(&0).map(|span| span.instruction.as_slice()),
                Some(bytes.as_slice()),
                "{level:?} {bytes:02X?}"
            );

            if level == crate::smir::optimize::OptLevel::O2 {
                assert!(
                    function.blocks[0]
                        .ops
                        .iter()
                        .any(|op| matches!(op.kind, crate::smir::ir::ops::OpKind::VMov { .. })),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn optimized_zero_count_replay_fails_closed_without_exact_register_provenance() {
    let shape = RotateShape {
        variable: false,
        left: false,
        quadword: false,
        ll: 0,
    };
    let register = encoding(shape, 1, 2, 0, 0, false, 0);
    let mut missing = function(&register);
    missing.x86_instruction_bytes.clear();
    crate::smir::optimize::optimize_function(&mut missing, crate::smir::optimize::OptLevel::O2);
    assert!(!is_native_clobber_safe(&missing));

    let mut unsafe_encodings = Vec::new();
    let mut memory = register.clone();
    memory[5] &= 0x3F;
    unsafe_encodings.push(memory);
    let mut broadcast = register.clone();
    broadcast[3] |= 0x10;
    unsafe_encodings.push(broadcast);
    let mut reserved_length = register.clone();
    reserved_length[3] = (reserved_length[3] & !0x60) | 0x60;
    unsafe_encodings.push(reserved_length);
    let mut zeroing_k0 = register.clone();
    zeroing_k0[3] |= 0x80;
    unsafe_encodings.push(zeroing_k0);
    let mut invalid_group = register.clone();
    invalid_group[5] = (invalid_group[5] & 0xC7) | (3 << 3);
    unsafe_encodings.push(invalid_group);

    for unsafe_encoding in unsafe_encodings {
        let mut malformed = function(&register);
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&unsafe_encoding).unwrap(),
        );
        crate::smir::optimize::optimize_function(
            &mut malformed,
            crate::smir::optimize::OptLevel::O2,
        );
        assert!(
            !is_native_clobber_safe(&malformed),
            "{unsafe_encoding:02X?}"
        );
    }
}

#[test]
fn immediate_group_ignored_r_bits_are_replayed_exactly() {
    let shape = RotateShape {
        variable: false,
        left: true,
        quadword: true,
        ll: 2,
    };
    let canonical = encoding(shape, 1, 2, 0, 1, false, 0x3F);
    for ignored_r_bits in [0x00, 0x10, 0x80, 0x90] {
        let mut bytes = canonical.clone();
        bytes[1] = (bytes[1] & !0x90) | ignored_r_bits;
        let mut function = function(&bytes);
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );
        let spans = crate::smir::ir::x86_native_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        );
        assert_eq!(
            spans.get(&0).map(|span| span.instruction.as_slice()),
            Some(bytes.as_slice()),
            "{bytes:02X?}"
        );
        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct RotateState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &RotateState) -> RotateState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
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
    RotateState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &RotateState) -> RotateState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut function = function(bytes);
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX packed-rotate replay");
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
    RotateState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_directions_counts_widths_aliases_masks_and_extensions() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native packed-rotate differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let base = RotateState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|lane| {
                0x8000_0001_7FFF_FFFFu64.rotate_left((register * 13 + lane * 19) as u32)
                    ^ ((register as u64) << 57)
                    ^ (lane as u64).wrapping_mul(0x8102_0408_1020_4081)
            })
        }),
        masks: [
            u64::MAX,
            0xA55A_3CC3_F00F_9696,
            0x5AA5_C33C_0FF0_6969,
            0,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (1, 1, 2),
        (1, 2, 1),
        (1, 1, 1),
    ];
    let mut executed = 0usize;
    let mut expected = 0usize;

    for shape in shapes() {
        if shape.ll != 2 && !has_vl {
            continue;
        }
        let immediates: &[u8] = if shape.variable {
            &[0]
        } else if shape.quadword {
            &[0, 1, 31, 32, 63, 64, 65, 0xFF]
        } else {
            &[0, 1, 15, 31, 32, 33, 63, 64, 0xFF]
        };
        for (destination, source, count) in operands {
            for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                for &immediate in immediates {
                    let bytes =
                        encoding(shape, destination, source, count, mask, zeroing, immediate);
                    let mut initial = base.clone();
                    if shape.variable {
                        initial.vectors[count as usize] = if shape.quadword {
                            [0, 1, 63, 64, 65, 127, 128, 0xFF]
                        } else {
                            [
                                0x0000_0001_0000_0000,
                                0x0000_0020_0000_001F,
                                0x0000_003F_0000_0021,
                                0x0000_00FF_0000_0040,
                                0x0000_0080_0000_007F,
                                0xFFFF_FFFF_8000_0000,
                                0x1234_5678_0000_0010,
                                0x0000_0000_FFFF_FFFF,
                            ]
                        };
                    }
                    assert_eq!(
                        execute_native(&bytes, &initial),
                        interpret(&bytes, &initial),
                        "{shape:?} {bytes:02X?}"
                    );
                    executed += 1;
                }
            }
        }
        expected += operands.len() * 3 * immediates.len();
    }
    assert!(expected > 0, "feature-selected packed-rotate shapes");
    assert_eq!(executed, expected);
}
