//! Exact helper-backed AVX_NE_CONVERT VEX memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, SignExtend, VReg, VecWidth,
    VirtualId, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, X86VexNeConvertKind,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_vex_ne_convert_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xAEC1;
const DISP: i64 = 0x20;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
const KINDS: [X86VexNeConvertKind; 7] = [
    X86VexNeConvertKind::BroadcastBf16,
    X86VexNeConvertKind::BroadcastFp16,
    X86VexNeConvertKind::EvenBf16,
    X86VexNeConvertKind::EvenFp16,
    X86VexNeConvertKind::OddBf16,
    X86VexNeConvertKind::OddFp16,
    X86VexNeConvertKind::Fp32ToBf16,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryCase {
    kind: X86VexNeConvertKind,
    width: VecWidth,
    destination: u8,
    base: u8,
    clear_ignored_x: bool,
}

impl MemoryCase {
    fn opcode_and_pp(self) -> (u8, u8) {
        match self.kind {
            X86VexNeConvertKind::BroadcastBf16 => (0xB1, 2),
            X86VexNeConvertKind::BroadcastFp16 => (0xB1, 1),
            X86VexNeConvertKind::EvenBf16 => (0xB0, 2),
            X86VexNeConvertKind::EvenFp16 => (0xB0, 1),
            X86VexNeConvertKind::OddBf16 => (0xB0, 3),
            X86VexNeConvertKind::OddFp16 => (0xB0, 0),
            X86VexNeConvertKind::Fp32ToBf16 => (0x72, 2),
        }
    }

    fn bytes(self) -> Vec<u8> {
        let (opcode, pp) = self.opcode_and_pp();
        vec![
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.clear_ignored_x { 0 } else { 0x40 })
                | (if self.base < 8 { 0x20 } else { 0 })
                | 2,
            0x78 | (u8::from(self.width == VecWidth::V256) << 2) | pp,
            opcode,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
            DISP as u8,
        ]
    }

    fn memory_size(self) -> u32 {
        if self.kind.broadcast() {
            2
        } else {
            self.width.bytes()
        }
    }

    fn stack_instruction(self) -> [u8; 6] {
        let (opcode, pp) = self.opcode_and_pp();
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 }) | 0x62,
            0x78 | (u8::from(self.width == VecWidth::V256) << 2) | pp,
            opcode,
            ((self.destination & 7) << 3) | 4,
            0x24,
        ]
    }
}

fn cases() -> Vec<MemoryCase> {
    let shapes = [(0, 3), (1, 11), (9, 3), (15, 11)];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for kind in KINDS {
        for width in [VecWidth::V128, VecWidth::V256] {
            for (destination, base) in shapes {
                cases.push(MemoryCase {
                    kind,
                    width,
                    destination,
                    base,
                    clear_ignored_x: ordinal & 1 != 0,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    x86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("AVX_NE_CONVERT VEX width"),
    })
}

fn expected_address(case: MemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base)),
        offset: DISP,
        disp_size: DispSize::Disp8,
    }
}

fn lift_bytes(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("architectural x86 instruction length"),
    );
    function
}

fn lift_case(case: MemoryCase) -> SmirFunction {
    let function = lift_bytes(&case.bytes());
    assert_exact_pair(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_exact_pair(function: &SmirFunction, case: MemoryCase) {
    let [load, conversion] = function.blocks[0].ops.as_slice() else {
        panic!("{case:?}: expected exact two-op decomposition")
    };
    assert_eq!(load.guest_pc, PC);
    assert_eq!(conversion.guest_pc, PC);
    assert_eq!(conversion.x86_hint, None);

    let loaded = if case.kind.broadcast() {
        assert_eq!(load.x86_hint, None);
        match &load.kind {
            OpKind::Load {
                dst: loaded @ VReg::Virtual(_),
                addr,
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(addr, &expected_address(case), "{case:?}");
                *loaded
            }
            other => panic!("{case:?}: expected unsigned 2-byte Load, got {other:?}"),
        }
    } else {
        assert!(
            matches!(
                load.x86_hint,
                None | Some(X86OpHint::VecAlign(
                    X86VecAlign::Aligned | X86VecAlign::Unaligned
                ))
            ),
            "{case:?}: {:?}",
            load.x86_hint
        );
        match &load.kind {
            OpKind::VLoad {
                dst: loaded @ VReg::Virtual(_),
                addr,
                width,
            } => {
                assert_eq!(addr, &expected_address(case), "{case:?}");
                assert_eq!(*width, case.width, "{case:?}");
                *loaded
            }
            other => panic!("{case:?}: expected vector load, got {other:?}"),
        }
    };

    if case.kind == X86VexNeConvertKind::Fp32ToBf16 {
        assert!(
            matches!(
                conversion.kind,
                OpKind::VCvtFP32ToBF16 {
                    dst,
                    src1,
                    src2: None,
                    mask: None,
                    width,
                    zeroing: false,
                } if dst == vector(case.destination, VecWidth::V128)
                    && src1 == loaded
                    && width == case.width
            ),
            "{case:?}: {:?}",
            conversion.kind
        );
    } else {
        assert!(
            matches!(
                conversion.kind,
                OpKind::X86Convert16ToFp32 {
                    dst,
                    src,
                    width,
                    fp16,
                    odd,
                    broadcast,
                } if dst == vector(case.destination, case.width)
                    && src == loaded
                    && width == case.width
                    && fp16 == case.kind.fp16()
                    && odd == case.kind.odd()
                    && broadcast == case.kind.broadcast()
            ),
            "{case:?}: {:?}",
            conversion.kind
        );
    }
}

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<crate::smir::lower::runtime::X86JitVexNeConvertMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_vex_ne_convert_memory_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn lower(function: &SmirFunction, case: MemoryCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx_ne_convert, "{case:?}");
    assert!(!requirements.needs_avx2, "{case:?}");
    assert!(!requirements.needs_avx_ifma, "{case:?}");
    assert!(!requirements.needs_avx_vnni, "{case:?}");
    assert!(!requirements.needs_avx_vnni_int8, "{case:?}");
    assert!(!requirements.needs_avx_vnni_int16, "{case:?}");
    assert!(!requirements.needs_f16c, "{case:?}");
    assert!(!requirements.needs_fma, "{case:?}");
    assert!(!requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512fp16, "{case:?}");

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        requirements.x86_host_supported(),
        std::is_x86_feature_detected!("avx")
            && crate::smir::lower::runtime::x86_host_has_avx_ne_convert(),
        "{case:?}"
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed AVX_NE_CONVERT lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed AVX_NE_CONVERT"),
        result.entry_offset,
    )
}

#[test]
fn every_kind_width_extension_shape_and_optimizer_profile_is_admitted_and_lowered() {
    let cases = cases();
    assert_eq!(cases.len(), KINDS.len() * 2 * 4);
    let expected = cases.len() * LEVELS.len();
    let mut lowered = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_exact_pair(&function, case);
            let admitted = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {case:?}: sequence rejected"));
            assert_eq!(admitted.consumed, 2, "{level:?} {case:?}");
            assert_eq!(admitted.encoding.kind, case.kind, "{level:?} {case:?}");
            assert_eq!(admitted.encoding.width, case.width, "{level:?} {case:?}");
            assert_eq!(
                admitted.encoding.destination, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(
                admitted.encoding.memory_size,
                case.memory_size(),
                "{level:?} {case:?}"
            );
            assert_ne!(
                admitted.encoding.scratch, case.destination,
                "{level:?} {case:?}"
            );
            assert_eq!(
                admitted.encoding.stack_instruction.as_slice(),
                case.stack_instruction(),
                "{level:?} {case:?}"
            );
            assert!(sequence(&function, false).is_none(), "{level:?} {case:?}");

            let (code, _) = lower(&function, case);
            let stack = case.stack_instruction();
            assert!(
                code.windows(stack.len()).any(|window| window == stack),
                "{level:?} {case:?}: missing {stack:02X?}"
            );
            if !case.kind.broadcast() {
                assert!(
                    code.windows(4).any(|window| {
                        window == crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET.to_le_bytes()
                    }),
                    "{level:?} {case:?}: missing vector scratch displacement"
                );
            }
            lowered += 1;
        }
    }
    assert_eq!(lowered, expected);
}

#[test]
fn complete_address_shapes_prefixes_and_llvm_23_encodings_admit_and_lower() {
    let encodings: [&[u8]; 9] = [
        &[0xC4, 0x62, 0x7A, 0xB1, 0x48, 0x11],
        &[0xC4, 0x62, 0x7D, 0xB1, 0x48, 0x11],
        &[0xC4, 0x62, 0x7A, 0xB0, 0x48, 0x11],
        &[0xC4, 0x62, 0x7C, 0xB0, 0x48, 0x11],
        &[0xC4, 0x62, 0x7E, 0x72, 0x48, 0x11],
        &[0xC4, 0xE2, 0x7A, 0xB0, 0x0C, 0x8B],
        &[0xC4, 0xE2, 0x7A, 0xB0, 0x0D, 0x20, 0, 0, 0],
        &[0x64, 0xC4, 0xE2, 0x7A, 0xB0, 0x4B, 0x20],
        &[0x67, 0x65, 0xC4, 0xE2, 0x7A, 0xB0, 0x4C, 0x8B, 0x20],
    ];

    let mut lowered = 0usize;
    for bytes in encodings {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            let admitted = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {bytes:02X?}: sequence rejected"));
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_mem_helpers(true);
            lowerer.set_preserve_vector_mem_helpers(true);
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer.set_guest_pcrel_lea_immediates(true);
            lowerer.set_jit_fault_deopt_guards(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            assert!(
                code.windows(admitted.encoding.stack_instruction.as_slice().len())
                    .any(|window| window == admitted.encoding.stack_instruction.as_slice())
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, encodings.len() * DIFFERENTIAL_LEVELS.len());
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        sequence(function, true).is_none(),
        "{name}: classifier admitted malformed sequence"
    );
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate admitted malformed sequence"
    );
}

fn assert_mutation_rejected(
    base: &SmirFunction,
    name: &str,
    mutate: impl FnOnce(&mut SmirFunction),
) {
    let mut function = base.clone();
    mutate(&mut function);
    assert_rejected(name, &function);
}

#[test]
fn classifier_and_gate_fail_closed_for_provenance_dataflow_semantics_and_boundaries() {
    let scalar_case = MemoryCase {
        kind: X86VexNeConvertKind::BroadcastFp16,
        width: VecWidth::V256,
        destination: 9,
        base: 11,
        clear_ignored_x: true,
    };
    let vector_case = MemoryCase {
        kind: X86VexNeConvertKind::OddFp16,
        width: VecWidth::V256,
        destination: 9,
        base: 11,
        clear_ignored_x: true,
    };
    let bf16_case = MemoryCase {
        kind: X86VexNeConvertKind::Fp32ToBf16,
        width: VecWidth::V256,
        destination: 9,
        base: 11,
        clear_ignored_x: true,
    };

    for case in [scalar_case, vector_case, bf16_case] {
        let base = lift_case(case);
        let loaded = match base.blocks[0].ops[0].kind {
            OpKind::Load { dst, .. } | OpKind::VLoad { dst, .. } => dst,
            ref other => panic!("expected memory load, got {other:?}"),
        };

        assert_mutation_rejected(&base, "missing provenance", |function| {
            function.x86_instruction_bytes.clear();
        });
        assert_mutation_rejected(&base, "register-form provenance", |function| {
            let mut bytes = case.bytes();
            bytes.truncate(5);
            bytes[4] |= 0xC0;
            function
                .x86_instruction_bytes
                .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        });
        assert_mutation_rejected(&base, "memory gate disabled", |function| {
            let (definitions, uses) = virtual_counts(function);
            assert!(
                x86_jit_vex_ne_convert_memory_sequence(
                    &function.blocks[0],
                    0,
                    false,
                    &function.x86_instruction_bytes,
                    &definitions,
                    &uses,
                )
                .is_none()
            );
            function.x86_instruction_bytes.clear();
        });
        assert_mutation_rejected(&base, "virtual used twice", |function| {
            function.blocks[0].ops.push(SmirOp::new(
                OpId(2),
                PC + 1,
                OpKind::VMov {
                    dst: vector(4, VecWidth::V128),
                    src: loaded,
                    width: VecWidth::V128,
                },
            ));
        });
        assert_mutation_rejected(&base, "virtual defined twice", |function| {
            function.blocks[0].ops.push(SmirOp::new(
                OpId(2),
                PC + 1,
                OpKind::Mov {
                    dst: loaded,
                    src: crate::smir::ir::types::SrcOperand::Imm(0),
                    width: crate::smir::ir::types::OpWidth::W64,
                },
            ));
        });
        assert_mutation_rejected(&base, "virtual address", |function| {
            match &mut function.blocks[0].ops[0].kind {
                OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => {
                    *addr = Address::Direct(VReg::Virtual(VirtualId(0xFFFF)));
                }
                _ => unreachable!(),
            }
        });
        assert_mutation_rejected(&base, "load PC differs", |function| {
            function.blocks[0].ops[0].guest_pc += 1;
        });
        assert_mutation_rejected(&base, "consumer PC differs", |function| {
            function.blocks[0].ops[1].guest_pc += 1;
        });
        assert_mutation_rejected(&base, "consumer hint present", |function| {
            function.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
        });
        assert_mutation_rejected(&base, "consumer kind differs", |function| {
            function.blocks[0].ops[1].kind = OpKind::Nop;
        });
        assert_mutation_rejected(&base, "trailing same-PC operation", |function| {
            function.blocks[0]
                .ops
                .push(SmirOp::new(OpId(2), PC, OpKind::Nop));
        });
        assert_mutation_rejected(&base, "preceding same-PC operation", |function| {
            function.blocks[0]
                .ops
                .insert(0, SmirOp::new(OpId(u16::MAX), PC, OpKind::Nop));
        });
    }

    let scalar = lift_case(scalar_case);
    assert_mutation_rejected(&scalar, "scalar load width differs", |function| {
        if let OpKind::Load { width, .. } = &mut function.blocks[0].ops[0].kind {
            *width = MemWidth::B4;
        }
    });
    assert_mutation_rejected(&scalar, "scalar load signed", |function| {
        if let OpKind::Load { sign, .. } = &mut function.blocks[0].ops[0].kind {
            *sign = SignExtend::Sign;
        }
    });
    assert_mutation_rejected(&scalar, "scalar load hint present", |function| {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    });

    let vector_function = lift_case(vector_case);
    assert_mutation_rejected(&vector_function, "vector load width differs", |function| {
        if let OpKind::VLoad { width, .. } = &mut function.blocks[0].ops[0].kind {
            *width = VecWidth::V128;
        }
    });
    assert_mutation_rejected(
        &vector_function,
        "conversion destination differs",
        |function| {
            if let OpKind::X86Convert16ToFp32 { dst, .. } = &mut function.blocks[0].ops[1].kind {
                *dst = vector(3, vector_case.width);
            }
        },
    );
    assert_mutation_rejected(&vector_function, "conversion source differs", |function| {
        if let OpKind::X86Convert16ToFp32 { src, .. } = &mut function.blocks[0].ops[1].kind {
            *src = VReg::Virtual(VirtualId(0xFFFE));
        }
    });
    assert_mutation_rejected(&vector_function, "conversion width differs", |function| {
        if let OpKind::X86Convert16ToFp32 { width, .. } = &mut function.blocks[0].ops[1].kind {
            *width = VecWidth::V128;
        }
    });
    for (name, field) in [("fp16", 0), ("odd", 1), ("broadcast", 2)] {
        assert_mutation_rejected(&vector_function, name, |function| {
            if let OpKind::X86Convert16ToFp32 {
                fp16,
                odd,
                broadcast,
                ..
            } = &mut function.blocks[0].ops[1].kind
            {
                match field {
                    0 => *fp16 = !*fp16,
                    1 => *odd = !*odd,
                    _ => *broadcast = !*broadcast,
                }
            }
        });
    }

    let bf16 = lift_case(bf16_case);
    assert_mutation_rejected(&bf16, "BF16 destination differs", |function| {
        if let OpKind::VCvtFP32ToBF16 { dst, .. } = &mut function.blocks[0].ops[1].kind {
            *dst = vector(3, VecWidth::V128);
        }
    });
    assert_mutation_rejected(&bf16, "BF16 source differs", |function| {
        if let OpKind::VCvtFP32ToBF16 { src1, .. } = &mut function.blocks[0].ops[1].kind {
            *src1 = VReg::Virtual(VirtualId(0xFFFD));
        }
    });
    assert_mutation_rejected(&bf16, "BF16 mask present", |function| {
        if let OpKind::VCvtFP32ToBF16 { mask, .. } = &mut function.blocks[0].ops[1].kind {
            *mask = Some(x86(X86Reg::K(1)));
        }
    });
    assert_mutation_rejected(&bf16, "BF16 second source present", |function| {
        if let OpKind::VCvtFP32ToBF16 { src2, .. } = &mut function.blocks[0].ops[1].kind {
            *src2 = Some(vector(2, bf16_case.width));
        }
    });
    assert_mutation_rejected(&bf16, "BF16 zeroing enabled", |function| {
        if let OpKind::VCvtFP32ToBF16 { zeroing, .. } = &mut function.blocks[0].ops[1].kind {
            *zeroing = true;
        }
    });
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, Default)]
struct ScalarMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn scalar_load_helper(
    context: *mut ScalarMemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 16 | 32)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn initial_registers(case: MemoryCase, ordinal: usize) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};

    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (ordinal as u64).wrapping_mul(0x0804_0201_1020_4081)
        });
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((ordinal & 0x0F) as u64) * 0x80;
    registers
}

#[cfg(target_arch = "x86_64")]
fn vector_payload(case: MemoryCase, ordinal: usize) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    if case.kind == X86VexNeConvertKind::Fp32ToBf16 {
        let values = [
            0x0000_0000u32,
            0x8000_0000,
            0x0000_0001,
            0x007F_FFFF,
            0x0080_0000,
            0x3F80_0000,
            0x3F80_8000,
            0x3F81_8000,
            0x7F80_0000,
            0xFF80_0000,
            0x7FC1_2345,
            0x7F81_2345,
            0xBF80_8000,
            0x7F7F_FFFF,
            0x0080_8000,
            0x8080_8000,
        ];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(values) {
            chunk.copy_from_slice(&value.rotate_left((ordinal & 1) as u32).to_le_bytes());
        }
    } else {
        let values = if case.kind.fp16() {
            [
                0x0000u16, 0x8000, 0x0001, 0x03FF, 0x0400, 0x3C00, 0xBC00, 0x7C00, 0xFC00, 0x7E01,
                0x7C01, 0x3555, 0x3BFF, 0x0401, 0x83FF, 0xFBFF,
            ]
        } else {
            [
                0x0000u16, 0x8000, 0x0001, 0x007F, 0x0080, 0x3F80, 0xBF80, 0x7F80, 0xFF80, 0x7FC1,
                0x7F81, 0x3F81, 0x3FFF, 0x0081, 0x8081, 0xFF7F,
            ]
        };
        for (chunk, value) in bytes.chunks_exact_mut(2).zip(values) {
            chunk.copy_from_slice(&value.rotate_left((ordinal & 1) as u32).to_le_bytes());
        }
    }
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn scalar_payload(case: MemoryCase, ordinal: usize) -> u16 {
    let values = if case.kind.fp16() {
        [
            0x0000u16, 0x8000, 0x0001, 0x03FF, 0x0400, 0x3C00, 0xBC00, 0x7C00, 0xFC00, 0x7E01,
            0x7C01,
        ]
    } else {
        [
            0x0000u16, 0x8000, 0x0001, 0x007F, 0x0080, 0x3F80, 0xBF80, 0x7F80, 0xFF80, 0x7FC1,
            0x7F81,
        ]
    };
    values[ordinal % values.len()]
}

#[cfg(target_arch = "x86_64")]
fn patched_probe_instruction(case: MemoryCase) -> [u8; 6] {
    let stack = case.stack_instruction();
    if case.kind.broadcast() {
        [
            0xC4,
            stack[1],
            0x79 | (stack[2] & 0x04),
            0x79, // VPBROADCASTW xmm/ymm,[rsp]
            stack[4],
            stack[5],
        ]
    } else {
        [
            0xC4,
            (stack[1] & 0x80) | 0x61,
            0x7A | (stack[2] & 0x04),
            0x6F, // VMOVDQU xmm/ymm,[rsp]
            stack[4],
            stack[5],
        ]
    }
}

#[cfg(target_arch = "x86_64")]
fn patch_conversion_to_probe(code: &mut [u8], case: MemoryCase) {
    let source = case.stack_instruction();
    let offsets = code
        .windows(source.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == source).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "{case:?}: {source:02X?}");
    let offset = offsets[0];
    code[offset..offset + source.len()].copy_from_slice(&patched_probe_instruction(case));
}

#[cfg(target_arch = "x86_64")]
fn patched_success_expected(
    mut registers: crate::smir::lower::runtime::GuestRegs,
    case: MemoryCase,
    scalar: u16,
    payload: [u64; 8],
) -> crate::smir::lower::runtime::GuestRegs {
    let mut bytes = [0u8; 64];
    if case.kind.broadcast() {
        let scalar = scalar.to_le_bytes();
        for lane in bytes[..case.width.bytes() as usize].chunks_exact_mut(2) {
            lane.copy_from_slice(&scalar);
        }
    } else {
        for (chunk, word) in bytes[..case.width.bytes() as usize]
            .chunks_exact_mut(8)
            .zip(payload)
        {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        let words = (case.width.bytes() / 8) as usize;
        registers.vector_scratch =
            std::array::from_fn(|word| if word < words { payload[word] } else { 0 });
    }
    registers.zmm[usize::from(case.destination)] = std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    });
    registers
}

#[cfg(target_arch = "x86_64")]
#[test]
fn patched_native_boundary_executes_success_and_fault_frontiers_without_stack_or_state_leaks() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping patched AVX_NE_CONVERT helper boundary: host lacks AVX");
        return;
    }
    let has_avx2 = std::is_x86_feature_detected!("avx2");
    let executable_cases = cases()
        .into_iter()
        .filter(|case| !case.kind.broadcast() || has_avx2)
        .collect::<Vec<_>>();
    let expected_executions = executable_cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in executable_cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (mut code, entry) = lower(&function, case);
            patch_conversion_to_probe(&mut code, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let scalar = scalar_payload(case, ordinal);
            let payload = vector_payload(case, ordinal);

            if case.kind.broadcast() {
                let mut context = ScalarMemoryContext {
                    value: u64::from(scalar),
                    ok: 1,
                    ..ScalarMemoryContext::default()
                };
                let mut registers = initial_registers(case, ordinal);
                let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                registers.load_fn = scalar_load_helper as usize as u64;
                let mut expected = patched_success_expected(registers, case, scalar, payload);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: scalar success");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, address, "{level:?} {case:?}");
                assert_eq!(context.last_size, 2, "{level:?} {case:?}");
                assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            } else {
                let mut context = VectorMemoryContext {
                    value: payload,
                    ok: 1,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal);
                let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as usize as u64;
                let mut expected = patched_success_expected(registers, case, scalar, payload);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: vector success");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, address, "{level:?} {case:?}");
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                    "{level:?} {case:?}"
                );
                assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
                assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            }
            successes += 1;

            if case.kind.broadcast() {
                let mut context = ScalarMemoryContext {
                    value: u64::from(scalar ^ u16::MAX),
                    ok: 0,
                    ..ScalarMemoryContext::default()
                };
                let mut registers = initial_registers(case, ordinal ^ 0x55);
                let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
                registers.load_fn = scalar_load_helper as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: scalar fault");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, address, "{level:?} {case:?}");
                assert_eq!(context.last_size, 2, "{level:?} {case:?}");
                assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
            } else {
                let mut context = VectorMemoryContext {
                    value: payload,
                    ok: 0,
                    calls: 0,
                    last_addr: 0,
                    last_index: 0,
                    last_size: 0,
                    last_zero_upper: 0,
                };
                let mut registers = initial_registers(case, ordinal ^ 0x55);
                let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
                registers.vec_load_fn = vector_load_helper as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: vector fault");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, address, "{level:?} {case:?}");
                assert_eq!(
                    context.last_index,
                    crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                    "{level:?} {case:?}"
                );
                assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
                assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            }
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}

#[cfg(target_arch = "x86_64")]
fn interpreter_success(
    function: &SmirFunction,
    initial: &crate::smir::lower::runtime::GuestRegs,
    case: MemoryCase,
    scalar: u16,
    payload: [u64; 8],
    address: u64,
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;

    let mut memory = FlatMemory::new(0x10000);
    if case.kind.broadcast() {
        memory.load(address as usize, &scalar.to_le_bytes());
    } else {
        let mut bytes = [0u8; 64];
        for (chunk, word) in bytes.chunks_exact_mut(8).zip(payload) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        memory.load(address as usize, &bytes[..case.memory_size() as usize]);
    }
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&value[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    if !case.kind.broadcast() {
        let words = (case.memory_size() / 8) as usize;
        expected.vector_scratch =
            std::array::from_fn(|word| if word < words { payload[word] } else { 0 });
    }
    expected
}

#[cfg(target_arch = "x86_64")]
const NATIVE_CHILD_ENV: &str = "RAX_VEX_NE_CONVERT_MEMORY_CHILD";

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_sources_match_interpretation_on_an_avx_ne_convert_host() {
    if std::env::var_os(NATIVE_CHILD_ENV).is_some() {
        use crate::smir::lower::runtime::ExecMem;

        for (ordinal, case) in cases().into_iter().enumerate() {
            for level in DIFFERENTIAL_LEVELS {
                let function = optimize(lift_case(case), level);
                let (code, entry) = lower(&function, case);
                let exec = ExecMem::new(&code)
                    .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                let scalar = scalar_payload(case, ordinal);
                let payload = vector_payload(case, ordinal);

                if case.kind.broadcast() {
                    let mut helper = ScalarMemoryContext {
                        value: u64::from(scalar),
                        ok: 1,
                        ..ScalarMemoryContext::default()
                    };
                    let mut registers = initial_registers(case, ordinal);
                    let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                    registers.ctx = (&mut helper as *mut ScalarMemoryContext) as u64;
                    registers.load_fn = scalar_load_helper as usize as u64;
                    let mut expected =
                        interpreter_success(&function, &registers, case, scalar, payload, address);

                    exec.run(entry, &mut registers);
                    expected.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(registers, expected, "{level:?} {case:?}");
                    assert_eq!(helper.calls, 1, "{level:?} {case:?}");
                    assert_eq!(helper.last_size, 2, "{level:?} {case:?}");
                } else {
                    let mut helper = VectorMemoryContext {
                        value: payload,
                        ok: 1,
                        calls: 0,
                        last_addr: 0,
                        last_index: 0,
                        last_size: 0,
                        last_zero_upper: 0,
                    };
                    let mut registers = initial_registers(case, ordinal);
                    let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
                    registers.ctx = (&mut helper as *mut VectorMemoryContext) as u64;
                    registers.vec_load_fn = vector_load_helper as usize as u64;
                    let mut expected =
                        interpreter_success(&function, &registers, case, scalar, payload, address);

                    exec.run(entry, &mut registers);
                    expected.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(registers, expected, "{level:?} {case:?}");
                    assert_eq!(helper.calls, 1, "{level:?} {case:?}");
                    assert_eq!(helper.last_size, case.memory_size(), "{level:?} {case:?}");
                }
            }
        }
        return;
    }
    if !std::is_x86_feature_detected!("avx")
        || !crate::smir::lower::runtime::x86_host_has_avx_ne_convert()
    {
        eprintln!("skipping native AVX_NE_CONVERT memory differential: host feature unavailable");
        return;
    }

    let test_name = "smir::lower::runtime::jit_gate_tests::vex_ne_convert_memory_source::\
                     native_memory_sources_match_interpretation_on_an_avx_ne_convert_host";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(NATIVE_CHILD_ENV, "1")
        .output()
        .expect("spawn isolated AVX_NE_CONVERT memory differential");
    assert!(
        output.status.success(),
        "isolated AVX_NE_CONVERT memory differential failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
