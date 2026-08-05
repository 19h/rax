//! Exact helper-backed EVEX scalar floating-point-to-integer memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, MemWidth, OpWidth, SourceArch, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, X86JitEvexScalarFpToIntMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_evex_scalar_fp_to_int_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0x2D7A_9101;
const MEMORY_ADDRESS: u64 = 0x2000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceFormat {
    F32,
    F64,
    F16,
}

impl SourceFormat {
    const ALL: [Self; 3] = [Self::F32, Self::F64, Self::F16];

    const fn fields(self) -> (u8, u8, VecElementType, MemWidth, bool) {
        match self {
            Self::F32 => (1, 2, VecElementType::F32, MemWidth::B4, false),
            Self::F64 => (1, 3, VecElementType::F64, MemWidth::B8, false),
            Self::F16 => (5, 2, VecElementType::F16, MemWidth::B2, true),
        }
    }

    const fn element(self) -> VecElementType {
        self.fields().2
    }

    const fn memory_width(self) -> MemWidth {
        self.fields().3
    }

    const fn memory_size(self) -> usize {
        self.memory_width().bytes() as usize
    }

    const fn needs_fp16(self) -> bool {
        self.fields().4
    }

    const fn positive_two_and_half(self) -> u64 {
        match self {
            Self::F16 => 0x4100,
            Self::F32 => 0x4020_0000,
            Self::F64 => 0x4004_0000_0000_0000,
        }
    }

    const fn negative_two_and_half(self) -> u64 {
        match self {
            Self::F16 => 0xC100,
            Self::F32 => 0xC020_0000,
            Self::F64 => 0xC004_0000_0000_0000,
        }
    }

    const fn quiet_nan(self) -> u64 {
        match self {
            Self::F16 => 0x7E55,
            Self::F32 => 0x7FC1_2345,
            Self::F64 => 0x7FF8_1234_5678_9ABC,
        }
    }

    const fn positive_infinity(self) -> u64 {
        match self {
            Self::F16 => 0x7C00,
            Self::F32 => 0x7F80_0000,
            Self::F64 => 0x7FF0_0000_0000_0000,
        }
    }

    const fn positive_subnormal(self) -> u64 {
        1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFpToIntMemoryCase {
    format: SourceFormat,
    signed: bool,
    truncate: bool,
    w: bool,
    ll: u8,
    destination: u8,
    base: u8,
}

impl ScalarFpToIntMemoryCase {
    const fn opcode(self) -> u8 {
        match (self.signed, self.truncate) {
            (true, false) => 0x2D,
            (true, true) => 0x2C,
            (false, false) => 0x79,
            (false, true) => 0x78,
        }
    }

    const fn int_width(self) -> OpWidth {
        if self.w { OpWidth::W64 } else { OpWidth::W32 }
    }

    const fn memory_size(self) -> usize {
        self.format.memory_size()
    }

    fn bytes(self) -> [u8; 6] {
        memory_encoding(self)
    }

    fn register_instruction(self) -> [u8; 6] {
        let memory = self.bytes();
        [
            0x62,
            (memory[1] & 0x97) | 0x60,
            memory[2] | 0x04,
            memory[3],
            memory[4],
            0xC0 | (memory[5] & 0x38),
        ]
    }
}

fn memory_encoding(case: ScalarFpToIntMemoryCase) -> [u8; 6] {
    assert!(case.ll < 3 && case.destination < 16 && case.base < 16);
    assert!(
        !matches!(case.base & 7, 4 | 5),
        "six-byte encoding needs a direct base"
    );
    let (map, pp, _, _, _) = case.format.fields();
    [
        0x62,
        (u8::from(case.destination & 8 == 0) << 7)
            | 0x40
            | (u8::from(case.base & 8 == 0) << 5)
            | 0x10
            | map,
        (u8::from(case.w) << 7) | 0x7C | pp,
        (case.ll << 5) | 0x08,
        case.opcode(),
        ((case.destination & 7) << 3) | (case.base & 7),
    ]
}

fn function_from_bytes(bytes: &[u8], label: impl std::fmt::Debug) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{label:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{label:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("scalar FP-to-integer provenance"),
    );
    function
}

fn lift_case(case: ScalarFpToIntMemoryCase) -> SmirFunction {
    function_from_bytes(&case.bytes(), case)
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
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

fn sequence(function: &SmirFunction) -> Option<X86JitEvexScalarFpToIntMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    (0..function.blocks[0].ops.len()).find_map(|index| {
        x86_jit_evex_scalar_fp_to_int_memory_sequence(
            &function.blocks[0],
            index,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
    })
}

fn scanner_cases() -> Vec<ScalarFpToIntMemoryCase> {
    let mut cases = Vec::new();
    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for truncate in [false, true] {
                for w in [false, true] {
                    for ll in 0..=2 {
                        cases.push(ScalarFpToIntMemoryCase {
                            format,
                            signed,
                            truncate,
                            w,
                            ll,
                            destination: 0,
                            base: 2,
                        });
                    }
                }
            }
        }
    }
    cases
}

fn lower(function: &SmirFunction, case: ScalarFpToIntMemoryCase) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");
    assert!(requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.needs_avx512er, "{case:?}");
    assert_eq!(requirements.needs_avx512fp16, case.format.needs_fp16());
    assert!(!requirements.needs_fma, "{case:?}");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (!case.format.needs_fp16() || std::is_x86_feature_detected!("avx512fp16")),
        "{case:?}"
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        function, &excluded
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: helper-backed FP-to-integer: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed scalar FP-to-integer"),
        result.entry_offset,
    )
}

fn initial_registers(case: ScalarFpToIntMemoryCase, seed: usize, mxcsr: u32) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.gpr[usize::from(case.base)] = MEMORY_ADDRESS;
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    registers
}

fn interpreter_context(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    context.pc = PC;
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

fn interpreter_registers(context: &SmirContext, initial: &GuestRegs) -> GuestRegs {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut result = *initial;
    result.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        result.zmm[index].copy_from_slice(&value[..8]);
    }
    result.k = x86.k;
    result.rflags = x86.rflags;
    result.mxcsr = x86.mxcsr;
    result
}

fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: u64,
    case: ScalarFpToIntMemoryCase,
) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x3000);
    memory.load(
        MEMORY_ADDRESS as usize,
        &source.to_le_bytes()[..case.memory_size()],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    interpreter_registers(&context, initial)
}
