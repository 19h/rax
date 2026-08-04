//! Exact helper-backed EVEX scalar floating-point arithmetic memory coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, MemWidth, SourceArch, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    GuestRegs, X86_VECTOR_STATE_K64, X86JitEvexScalarFpArithmeticMemorySequence,
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, x86_jit_evex_scalar_fp_arithmetic_memory_sequence,
    x86_native_replay_feature_requirements, x86_native_vector_features_supported_excluding,
    x86_native_vector_uses_avx_ymm16_only_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

const PC: u64 = 0xE3A0;
const MEMORY_ADDRESS: u64 = 0x2000;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArithmeticOperation {
    Sqrt,
    Add,
    Mul,
    Sub,
    Min,
    Div,
    Max,
}

impl ArithmeticOperation {
    const ALL: [Self; 7] = [
        Self::Sqrt,
        Self::Add,
        Self::Mul,
        Self::Sub,
        Self::Min,
        Self::Div,
        Self::Max,
    ];

    const fn opcode(self) -> u8 {
        match self {
            Self::Sqrt => 0x51,
            Self::Add => 0x58,
            Self::Mul => 0x59,
            Self::Sub => 0x5C,
            Self::Min => 0x5D,
            Self::Div => 0x5E,
            Self::Max => 0x5F,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarFormat {
    F16,
    F32,
    F64,
}

impl ScalarFormat {
    const ALL: [Self; 3] = [Self::F16, Self::F32, Self::F64];

    const fn elem(self) -> VecElementType {
        match self {
            Self::F16 => VecElementType::F16,
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }

    const fn map(self) -> u8 {
        match self {
            Self::F16 => 5,
            Self::F32 | Self::F64 => 1,
        }
    }

    const fn pp(self) -> u8 {
        match self {
            Self::F16 | Self::F32 => 2,
            Self::F64 => 3,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::F64)
    }

    const fn memory_width(self) -> MemWidth {
        match self {
            Self::F16 => MemWidth::B2,
            Self::F32 => MemWidth::B4,
            Self::F64 => MemWidth::B8,
        }
    }

    const fn memory_size(self) -> usize {
        match self {
            Self::F16 => 2,
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskControl {
    None,
    Merge,
    Zero,
}

impl MaskControl {
    const ALL: [Self; 3] = [Self::None, Self::Merge, Self::Zero];

    const fn fields(self) -> (u8, bool) {
        match self {
            Self::None => (0, false),
            Self::Merge => (3, false),
            Self::Zero => (7, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarFpMemoryCase {
    operation: ArithmeticOperation,
    format: ScalarFormat,
    source1: u8,
    ll: u8,
    control: MaskControl,
}

impl ScalarFpMemoryCase {
    const fn destination(self) -> u8 {
        match self.source1 {
            1 => 0,
            17 => 17,
            30 => 31,
            _ => 16,
        }
    }

    const fn mask(self) -> u8 {
        self.control.fields().0
    }

    const fn zeroing(self) -> bool {
        self.control.fields().1
    }

    fn bytes(self) -> [u8; 6] {
        memory_encoding(
            self.operation,
            self.format,
            self.destination(),
            self.source1,
            self.ll,
            self.mask(),
            self.zeroing(),
            3,
        )
    }

    fn stack_instruction(self) -> [u8; 7] {
        stack_encoding(
            self.operation,
            self.format,
            self.destination(),
            self.source1,
            self.ll,
            self.mask(),
            self.zeroing(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn memory_encoding(
    operation: ArithmeticOperation,
    format: ScalarFormat,
    destination: u8,
    source1: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    base: u8,
) -> [u8; 6] {
    assert!(destination < 32 && source1 < 32 && base < 16);
    assert!(ll < 3 && mask < 8 && (!zeroing || mask != 0));
    [
        0x62,
        (if destination & 8 == 0 { 0x80 } else { 0 })
            | 0x40
            | (if base & 8 == 0 { 0x20 } else { 0 })
            | (if destination & 16 == 0 { 0x10 } else { 0 })
            | format.map(),
        (u8::from(format.w()) << 7) | (((!source1) & 0x0F) << 3) | 0x04 | format.pp(),
        (u8::from(zeroing) << 7) | (ll << 5) | (if source1 & 16 == 0 { 0x08 } else { 0 }) | mask,
        operation.opcode(),
        ((destination & 7) << 3) | (base & 7),
    ]
}

#[allow(clippy::too_many_arguments)]
fn stack_encoding(
    operation: ArithmeticOperation,
    format: ScalarFormat,
    destination: u8,
    source1: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 7] {
    let encoding = memory_encoding(
        operation,
        format,
        destination,
        source1,
        ll,
        mask,
        zeroing,
        4,
    );
    [
        encoding[0],
        encoding[1] | 0x20,
        encoding[2] | 0x04,
        encoding[3],
        encoding[4],
        encoding[5],
        0x24,
    ]
}

fn lift_case(case: ScalarFpMemoryCase) -> SmirFunction {
    let bytes = case.bytes();
    function_from_bytes(&bytes, case)
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
        X86InstructionBytes::new(&bytes).expect("EVEX scalar FP arithmetic provenance"),
    );
    function
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

fn sequence(function: &SmirFunction) -> Option<X86JitEvexScalarFpArithmeticMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    (0..function.blocks[0].ops.len()).find_map(|index| {
        x86_jit_evex_scalar_fp_arithmetic_memory_sequence(
            &function.blocks[0],
            index,
            true,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
    })
}

fn all_cases() -> Vec<ScalarFpMemoryCase> {
    let mut cases = Vec::new();
    for operation in ArithmeticOperation::ALL {
        for format in ScalarFormat::ALL {
            for source1 in [1, 17, 30] {
                for ll in 0..=2 {
                    for control in MaskControl::ALL {
                        cases.push(ScalarFpMemoryCase {
                            operation,
                            format,
                            source1,
                            ll,
                            control,
                        });
                    }
                }
            }
        }
    }
    cases
}

fn lower(function: &SmirFunction, case: ScalarFpMemoryCase) -> (Vec<u8>, usize) {
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
    assert!(!requirements.needs_fma, "{case:?}");
    assert_eq!(
        requirements.needs_avx512fp16,
        case.format == ScalarFormat::F16,
        "{case:?}"
    );
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(function, &excluded),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && (case.format != ScalarFormat::F16 || std::is_x86_feature_detected!("avx512fp16")),
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
    let result = lowerer.lower_function(function).unwrap_or_else(|error| {
        panic!("{case:?}: helper-backed scalar EVEX arithmetic: {error:?}")
    });
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed scalar EVEX arithmetic"),
        result.entry_offset,
    )
}

fn scalar_bits(format: ScalarFormat, operation: ArithmeticOperation, ordinal: usize) -> u64 {
    let selector = (ordinal + operation.opcode() as usize) % 4;
    match format {
        ScalarFormat::F16 => [0x3E00u16, 0x4000, 0x3555, 0xBC00][selector] as u64,
        ScalarFormat::F32 => {
            [0x3FC0_0000u32, 0x4000_0000, 0x3EAA_AAAB, 0xBF80_0000][selector] as u64
        }
        ScalarFormat::F64 => [
            0x3FF8_0000_0000_0000u64,
            0x4000_0000_0000_0000,
            0x3FD5_5555_5555_5555,
            0xBFF0_0000_0000_0000,
        ][selector],
    }
}

fn initial_registers(case: ScalarFpMemoryCase, ordinal: usize, active: bool) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.gpr[3] = MEMORY_ADDRESS;
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        });
    }
    let scalar = scalar_bits(case.format, case.operation, ordinal ^ 0x55);
    let mask = match case.format {
        ScalarFormat::F16 => u64::from(u16::MAX),
        ScalarFormat::F32 => u64::from(u32::MAX),
        ScalarFormat::F64 => u64::MAX,
    };
    let word = &mut registers.zmm[usize::from(case.source1)][0];
    *word = (*word & !mask) | scalar;
    if case.mask() != 0 {
        let opmask = &mut registers.k[usize::from(case.mask())];
        *opmask = (*opmask & !1) | u64::from(active);
    }
    registers
}

fn interpreter_context(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
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
    case: ScalarFpMemoryCase,
) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x3000);
    memory.load(
        MEMORY_ADDRESS as usize,
        &source.to_le_bytes()[..case.format.memory_size()],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    interpreter_registers(&context, initial)
}
