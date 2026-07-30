//! Packed-string memory/register equivalence and native helper differentials.

use super::*;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};

const STATUS_FLAGS: u64 = 0x08D5;
const INTERPRETER_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn input_pair(index: usize) -> ([u8; 16], [u8; 16]) {
    const INPUTS: [([u8; 16], [u8; 16]); 8] = [
        (*b"abc\0ABCDEFGHIJKL", *b"xbycz\0ABCDEFGHIJ"),
        (
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
        ),
        (
            [
                0x80, 0xFF, 0x7F, 0, 0x81, 1, 0xFE, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            ],
            [
                0x80, 0x7F, 0xFF, 1, 0x82, 2, 0xFD, 3, 4, 5, 6, 7, 8, 9, 10, 0,
            ],
        ),
        (
            [1, 0, 2, 0, 0, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0],
            [2, 0, 1, 0, 3, 0, 0, 0, 5, 0, 4, 0, 7, 0, 6, 0],
        ),
        (
            [0xFE, 0xFF, 2, 0, 0, 0, 4, 0, 6, 0, 8, 0, 10, 0, 12, 0],
            [0xFD, 0xFF, 0xFE, 0xFF, 2, 0, 0, 0, 3, 0, 5, 0, 7, 0, 9, 0],
        ),
        ([0xFF; 16], [0; 16]),
        ([0; 16], [0xFF; 16]),
        (
            [
                0x7F, 0x80, 0x00, 0xFF, 0x01, 0xFE, 0x02, 0xFD, 3, 252, 4, 251, 5, 250, 6, 249,
            ],
            [
                0x80, 0x7F, 0xFF, 0x00, 0xFE, 0x01, 0xFD, 0x02, 252, 3, 251, 4, 250, 5, 249, 6,
            ],
        ),
    ];
    INPUTS[index % INPUTS.len()]
}

fn initial_guest_regs(case: PackedStringMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x0102_0408_1020_4081_u64.rotate_left(((ordinal + index * 7) & 63) as u32)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & STATUS_FLAGS),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x6996_F00F_3CC3_A55A_u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        cr0: 1,
        cr4: 1 << 18,
        xcr0: 0b110,
        cs_l: 1,
        ..GuestRegs::default()
    };
    for (register, vector) in registers.zmm.iter_mut().enumerate() {
        *vector = std::array::from_fn(|word| {
            0xA55A_6996_F00F_3CC3_u64
                .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
        });
    }
    const LENGTHS: [(u64, u64); 8] = [
        (3, 5),
        ((-3i64) as u64, 5),
        (0x0000_0001_0000_0003, 0x0000_0001_0000_0005),
        (i64::MIN as u64, i64::MAX as u64),
        (u64::MAX, 0),
        (16, 8),
        (17, 9),
        ((-17i64) as u64, (-9i64) as u64),
    ];
    (registers.gpr[0], registers.gpr[2]) = LENGTHS[ordinal % LENGTHS.len()];
    registers.gpr[usize::from(case.base)] = 0x2000 - DISP as u64;

    let (source, _) = input_pair(ordinal);
    let mut source_bytes = words_to_bytes(registers.zmm[usize::from(case.source1)]);
    source_bytes[..16].copy_from_slice(&source);
    registers.zmm[usize::from(case.source1)] = bytes_to_words(source_bytes);
    registers
}

fn memory_bytes(ordinal: usize) -> [u8; 16] {
    input_pair(ordinal).1
}

fn helper_payload(memory: [u8; 16]) -> [u64; 8] {
    let mut payload = [0u8; 64];
    payload[..16].copy_from_slice(&memory);
    bytes_to_words(payload)
}

fn interpret(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: Option<[u8; 16]>,
    address: u64,
) -> GuestRegs {
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
        x86.cr0 = initial.cr0;
        x86.cr4 = initial.cr4;
        x86.xcr0 = initial.xcr0;
        x86.cs_l = initial.cs_l != 0;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    if let Some(value) = memory_value {
        memory.load(address as usize, &value);
    }
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    context.flags.materialize_all();

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut result = *initial;
    result.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        result.zmm[index].copy_from_slice(&value[..8]);
    }
    result.k = x86.k;
    result.rflags =
        (initial.rflags & !STATUS_FLAGS) | (context.flags.materialized.to_rflags() & STATUS_FLAGS);
    result.mxcsr = x86.mxcsr;
    result
}

fn assert_memory_register_equivalent(
    case: PackedStringMemoryCase,
    ordinal: usize,
    level: OptLevel,
) {
    let memory_function = optimize(lift_case(case), level);
    let initial = initial_guest_regs(case, ordinal);
    let memory = memory_bytes(ordinal);
    let address = initial.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
    let memory_result = interpret(&memory_function, &initial, Some(memory), address);

    let mut register_initial = initial;
    register_initial.zmm[usize::from(case.scratch())] = helper_payload(memory);
    let register_function = optimize(lift_bytes(&case.register_bytes()), level);
    let register_result = interpret(&register_function, &register_initial, None, address);

    assert_eq!(memory_result.gpr, register_result.gpr, "{level:?} {case:?}");
    assert_eq!(memory_result.k, register_result.k, "{level:?} {case:?}");
    assert_eq!(
        memory_result.rflags, register_result.rflags,
        "{level:?} {case:?}"
    );
    assert_eq!(
        memory_result.mxcsr, register_result.mxcsr,
        "{level:?} {case:?}"
    );
    for register in 0..32 {
        if register != usize::from(case.scratch()) {
            assert_eq!(
                memory_result.zmm[register], register_result.zmm[register],
                "{level:?} {case:?}: ZMM{register}"
            );
        }
    }
    assert_eq!(
        memory_result.zmm[usize::from(case.scratch())],
        initial.zmm[usize::from(case.scratch())],
        "{level:?} {case:?}: memory form changed the borrowed register"
    );
    assert_eq!(
        register_result.zmm[usize::from(case.scratch())],
        helper_payload(memory),
        "{level:?} {case:?}: register form changed its explicit memory payload"
    );
}

#[test]
fn memory_and_byte_rewritten_register_interpretation_match_all_4_224_cells() {
    let mut compared = 0usize;
    for kind in families() {
        for w in [false, true] {
            for immediate in u8::MIN..=u8::MAX {
                let case = PackedStringMemoryCase {
                    kind,
                    w,
                    source1: 9,
                    base: 11,
                    immediate,
                };
                for level in INTERPRETER_LEVELS {
                    assert_memory_register_equivalent(
                        case,
                        usize::from(immediate)
                            + usize::from(w) * 256
                            + usize::from(case.opcode() - 0x60) * 512,
                        level,
                    );
                    compared += 1;
                }
            }
        }
    }
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        for level in INTERPRETER_LEVELS {
            assert_memory_register_equivalent(case, 0x1000 + ordinal, level);
            compared += 1;
        }
    }
    assert_eq!(compared, (4 * 2 * 256 + all_cases().len()) * 2);
    assert_eq!(compared, 4_224);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u8; 16],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
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
        || size != 16
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0u8; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    value[..16].copy_from_slice(&context.value);
    state.vector_scratch = bytes_to_words(value);
    1
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: PackedStringMemoryCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, 16, "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: PackedStringMemoryCase,
    ordinal: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    const IMMEDIATES: [u8; 13] = [
        0x00, 0x01, 0x02, 0x03, 0x0C, 0x18, 0x24, 0x30, 0x40, 0x47, 0x7F, 0x80, 0xFF,
    ];
    let mut cases = Vec::new();
    for level in INTERPRETER_LEVELS {
        for kind in families() {
            for w in [false, true] {
                for (index, immediate) in IMMEDIATES.into_iter().enumerate() {
                    let (source1, base) =
                        OPERANDS[(index + usize::from(w) + usize::from(kind.returns_mask()))
                            % OPERANDS.len()];
                    let kind_index = match kind {
                        X86PackedStringKind::ExplicitMask => 0usize,
                        X86PackedStringKind::ExplicitIndex => 1,
                        X86PackedStringKind::ImplicitMask => 2,
                        X86PackedStringKind::ImplicitIndex => 3,
                    };
                    cases.push(NativeCase {
                        level,
                        instruction: PackedStringMemoryCase {
                            kind,
                            w,
                            source1,
                            base,
                            immediate,
                        },
                        ordinal: index
                            + usize::from(w) * IMMEDIATES.len()
                            + kind_index * IMMEDIATES.len() * 2,
                    });
                }
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn execute_raw_native_case(case: NativeCase) {
    use crate::smir::lower::runtime::ExecMem;

    let instruction = case.instruction;
    let function = optimize(lift_case(instruction), case.level);
    let memory = memory_bytes(case.ordinal);
    let (code, entry, _) = lower(&function, instruction);
    assert!(
        code.windows(instruction.register_bytes().len())
            .any(|window| window == instruction.register_bytes()),
        "{case:?}"
    );
    let exec = ExecMem::new(&code).unwrap_or_else(|error| panic!("{case:?}: {error:?}"));

    let mut context = VectorMemoryContext {
        value: memory,
        ok: 1,
        calls: 0,
        last_addr: 0,
        last_index: 0,
        last_size: 0,
        last_zero_upper: 0,
    };
    let mut registers = initial_guest_regs(instruction, case.ordinal);
    let address = registers.gpr[usize::from(instruction.base)].wrapping_add(DISP as u64);
    registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
    registers.vec_load_fn = vector_load_helper as usize as u64;
    let mut expected = interpret(&function, &registers, Some(memory), address);
    expected.vector_scratch = helper_payload(memory);

    exec.run(entry, &mut registers);
    expected.host_mxcsr = registers.host_mxcsr;
    assert_eq!(registers, expected, "{case:?}: success");
    assert_helper_observation(&context, address, instruction, "success");

    let mut context = VectorMemoryContext {
        value: memory,
        ok: 0,
        calls: 0,
        last_addr: 0,
        last_index: 0,
        last_size: 0,
        last_zero_upper: 0,
    };
    let mut registers = initial_guest_regs(instruction, case.ordinal ^ 0x55);
    let address = registers.gpr[usize::from(instruction.base)].wrapping_add(DISP as u64);
    registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
    registers.vec_load_fn = vector_load_helper as usize as u64;
    let mut expected = registers;
    expected.exit_pc = PC;

    exec.run(entry, &mut registers);
    expected.host_mxcsr = registers.host_mxcsr;
    assert_eq!(registers, expected, "{case:?}: helper fault");
    assert_helper_observation(&context, address, instruction, "fault");
}

#[cfg(target_arch = "x86_64")]
const RAW_NATIVE_CHILD_RANGE_ENV: &str = "RAX_VEX_PACKED_STRING_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn raw_native_child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(RAW_NATIVE_CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {RAW_NATIVE_CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {RAW_NATIVE_CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {RAW_NATIVE_CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_raw_native_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for &case in &cases[range] {
        execute_raw_native_case(case);
    }
}

#[cfg(target_arch = "x86_64")]
fn run_raw_native_child_range(
    test_name: &str,
    range: std::ops::Range<usize>,
) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(
            RAW_NATIVE_CHILD_RANGE_ENV,
            format!("{}:{}", range.start, range.end),
        )
        .output()
        .expect("run isolated native VEX packed-string memory-source differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_raw_native_differential(test_name: &str, cases: &[NativeCase]) {
    if let Some(range) = raw_native_child_range() {
        execute_raw_native_case_range(cases, range);
        return;
    }

    let whole = run_raw_native_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_raw_native_child_range(test_name, start..middle)
            .status
            .success()
        {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_raw_native_child_range(test_name, start..end);
    let case = cases[start];
    panic!(
        "isolated native VEX packed-string memory-source failure at case {start}/{}: \
         {case:?} {:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case.instruction.bytes(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn raw_native_memory_sources_match_interpreter_and_fault_precisely_for_all_208_cells() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed-string memory differential: host lacks AVX");
        return;
    }
    let cases = native_cases();
    assert_eq!(cases.len(), 208);
    run_isolated_raw_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_packed_string_memory_source::semantics::\
         raw_native_memory_sources_match_interpreter_and_fault_precisely_for_all_208_cells",
        &cases,
    );
}
