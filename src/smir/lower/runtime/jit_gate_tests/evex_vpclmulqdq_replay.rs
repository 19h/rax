//! Native replay coverage for register-only EVEX VPCLMULQDQ instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x4800;
type VpclmulqdqShape = (bool, u8);

fn shapes() -> Vec<VpclmulqdqShape> {
    let mut shapes = Vec::new();
    for ll in 0..=2 {
        for w in [false, true] {
            shapes.push((w, ll));
        }
    }
    shapes
}

fn encoding(
    shape: VpclmulqdqShape,
    destination: u8,
    source1: u8,
    source2: u8,
    immediate: u8,
) -> [u8; 7] {
    let (w, ll) = shape;
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3);
    let mut p0 = 0xF3;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 },
        (ll << 5) | if source1 < 16 { 0x08 } else { 0 },
        0x44,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
        immediate,
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
fn replay_feature_aggregation_requires_vpclmulqdq_bw_and_exact_vl() {
    for shape in [(false, 0), (true, 1), (false, 2)] {
        let bytes = encoding(shape, 17, 18, 19, 0x11);
        let function = function(&bytes);
        let requirements =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(requirements.any, "{bytes:02X?}");
        assert!(requirements.needs_avx512bw, "{bytes:02X?}");
        assert_eq!(requirements.needs_avx512vl, shape.1 != 2, "{bytes:02X?}");
        assert!(!requirements.needs_avx512dq, "{bytes:02X?}");
        assert!(!requirements.needs_avx512fp16, "{bytes:02X?}");
        assert!(!requirements.needs_avx512cd, "{bytes:02X?}");
        assert!(!requirements.needs_gfni, "{bytes:02X?}");
        assert!(requirements.needs_vpclmulqdq, "{bytes:02X?}");

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{bytes:02X?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_family_host_gate_conjoins_gfni_and_vpclmulqdq() {
    let host_gfni = std::is_x86_feature_detected!("gfni");
    let host_vpclmulqdq = std::is_x86_feature_detected!("vpclmulqdq");
    for (needs_gfni, needs_vpclmulqdq) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let requirements = X86NativeReplayFeatureRequirements {
            needs_gfni,
            needs_vpclmulqdq,
            ..X86NativeReplayFeatureRequirements::default()
        };
        assert_eq!(
            requirements.x86_host_supported(),
            (!needs_gfni || host_gfni) && (!needs_vpclmulqdq || host_vpclmulqdq)
        );
    }
}

#[test]
fn replay_admits_and_emits_144_legal_register_encodings() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    assert_eq!(
        encoding((true, 2), 25, 26, 27, 0xEF),
        [0x62, 0x03, 0xAD, 0x40, 0x44, 0xCB, 0xEF]
    );
    let operands = [(1u8, 2u8, 3u8), (9, 10, 11), (17, 18, 19), (25, 26, 27)];
    let immediates = [0x00u8, 0x01, 0x10, 0x11, 0xAA, 0xEF];
    let mut admitted = 0usize;
    let mut missing_provenance_checked = false;
    for shape in shapes() {
        for (destination, source1, source2) in operands {
            for immediate in immediates {
                let bytes = encoding(shape, destination, source1, source2, immediate);
                let needs_vl = crate::smir::ir::X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_vpclmulqdq_needs_vl()
                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                let mut function = function(&bytes);
                if !missing_provenance_checked {
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
                    uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                    "{bytes:02X?}"
                );

                #[cfg(target_arch = "x86_64")]
                let expected_features = std::is_x86_feature_detected!("avx512f")
                    && std::is_x86_feature_detected!("avx512bw")
                    && std::is_x86_feature_detected!("vpclmulqdq")
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

        let register = encoding(shape, 1, 2, 3, 0x11);
        let mut unsafe_encodings = Vec::new();
        let mut memory = register;
        memory[5] &= 0x3F;
        unsafe_encodings.push(memory);
        let mut embedded_control = register;
        embedded_control[3] |= 0x10;
        unsafe_encodings.push(embedded_control);
        let mut reserved_length = register;
        reserved_length[3] = (reserved_length[3] & !0x60) | 0x60;
        unsafe_encodings.push(reserved_length);
        let mut masked = register;
        masked[3] |= 0x01;
        unsafe_encodings.push(masked);
        let mut zeroing = register;
        zeroing[3] |= 0x80;
        unsafe_encodings.push(zeroing);

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
    assert_eq!(admitted, 144);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct VpclmulqdqState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &VpclmulqdqState) -> VpclmulqdqState {
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
    VpclmulqdqState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &VpclmulqdqState) -> VpclmulqdqState {
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
    let exec = ExecMem::new(&code).expect("map EVEX VPCLMULQDQ replay");
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
    VpclmulqdqState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_shapes_wig_extensions_aliases_selectors_and_edges() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("vpclmulqdq")
    {
        eprintln!(
            "skipping native EVEX VPCLMULQDQ differential: host lacks AVX-512F/BW or VPCLMULQDQ"
        );
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let mut initial = VpclmulqdqState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|lane| {
                0x8000_0000_0000_0001u64.rotate_left((register * 11 + lane * 17) as u32)
                    ^ ((register as u64) << 56)
                    ^ (lane as u64).wrapping_mul(0x1D2C_3B4A_5968_7786)
            })
        }),
        masks: std::array::from_fn(|index| {
            0xA55A_3CC3_F00F_9696u64.rotate_left((index * 9) as u32) ^ (1u64 << index)
        }),
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    initial.vectors[3] = [0; 8];
    initial.vectors[11] = [u64::MAX; 8];
    initial.vectors[19] = [0x8000_0000_0000_0000; 8];
    initial.vectors[27] = [0xAAAA_AAAA_5555_5555; 8];

    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (1, 1, 2),
        (1, 2, 1),
        (1, 1, 1),
    ];
    let immediates = [0x00u8, 0x01, 0x10, 0x11, 0xAA, 0xEF];

    let mut executed = 0usize;
    let mut expected = 0usize;
    for shape in shapes() {
        if shape.1 != 2 && !has_vl {
            continue;
        }
        for (destination, source1, source2) in operands {
            for immediate in immediates {
                let bytes = encoding(shape, destination, source1, source2, immediate);
                assert_eq!(
                    execute_native(&bytes, &initial),
                    interpret(&bytes, &initial),
                    "{bytes:02X?}"
                );
                executed += 1;
            }
        }
        expected += operands.len() * immediates.len();
    }
    assert!(expected > 0, "feature-selected VPCLMULQDQ shapes");
    assert_eq!(executed, expected);
}
