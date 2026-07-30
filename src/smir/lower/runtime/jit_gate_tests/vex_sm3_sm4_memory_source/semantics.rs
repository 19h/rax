//! SM3/SM4 memory/register equivalence and native helper-boundary differentials.

use super::*;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_YMM16};

const INTERPRETER_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];
#[cfg(target_arch = "x86_64")]
const NATIVE_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn memory_bytes(case: Sm3Sm4MemoryCase, ordinal: usize) -> [u8; 64] {
    let family = match case.kind {
        X86VexSm3Sm4MemoryKind::Sm3Msg1 => 0x11,
        X86VexSm3Sm4MemoryKind::Sm3Msg2 => 0x22,
        X86VexSm3Sm4MemoryKind::Sm3Rounds2 => 0x33,
        X86VexSm3Sm4MemoryKind::Sm4Key4 => 0x44,
        X86VexSm3Sm4MemoryKind::Sm4Rounds4 => 0x55,
    };
    std::array::from_fn(|index| {
        0x69_u8
            .wrapping_add((ordinal * 17) as u8)
            .rotate_left((index & 7) as u32)
            ^ (index as u8).wrapping_mul(0x3D)
            ^ family
            ^ case.immediate.rotate_left((index & 3) as u32)
    })
}

fn initial_guest_regs(case: Sm3Sm4MemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x0102_0408_1020_4081_u64.rotate_left(((ordinal + index * 7) & 63) as u32)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
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
    registers.gpr[usize::from(case.base)] = 0x2000 - DISP as u64;
    registers
}

fn helper_payload(case: Sm3Sm4MemoryCase, memory: [u8; 64]) -> [u64; 8] {
    let mut payload = [0u8; 64];
    payload[..case.width.bytes() as usize].copy_from_slice(&memory[..case.width.bytes() as usize]);
    bytes_to_words(payload)
}

fn interpret(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: Option<[u8; 64]>,
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

fn sm3_semantic_cases() -> Vec<Sm3Sm4MemoryCase> {
    let mut cases = all_cases()
        .into_iter()
        .filter(|case| case.kind.needs_sm3())
        .collect::<Vec<_>>();
    cases.extend((u8::MIN..=u8::MAX).map(|immediate| Sm3Sm4MemoryCase {
        kind: X86VexSm3Sm4MemoryKind::Sm3Rounds2,
        width: VecWidth::V128,
        destination: 9,
        source1: 10,
        base: 11,
        immediate,
    }));
    cases
}

fn sm4_semantic_cases() -> Vec<Sm3Sm4MemoryCase> {
    all_cases()
        .into_iter()
        .filter(|case| case.kind.needs_sm4())
        .collect()
}

fn assert_memory_register_equivalent(case: Sm3Sm4MemoryCase, ordinal: usize, level: OptLevel) {
    let memory_function = optimize(lift_case(case), level);
    let initial = initial_guest_regs(case, ordinal);
    let memory = memory_bytes(case, ordinal);
    let address = initial.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
    let memory_result = interpret(&memory_function, &initial, Some(memory), address);

    let mut register_initial = initial;
    register_initial.zmm[usize::from(case.scratch())] = helper_payload(case, memory);
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
        helper_payload(case, memory),
        "{level:?} {case:?}: register form changed its explicit memory payload"
    );
}

#[test]
fn memory_and_byte_rewritten_register_interpretation_match_all_624_cells() {
    let mut compared = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        for level in INTERPRETER_LEVELS {
            assert_memory_register_equivalent(case, ordinal, level);
            compared += 1;
        }
    }
    for immediate in u8::MIN..=u8::MAX {
        let case = Sm3Sm4MemoryCase {
            kind: X86VexSm3Sm4MemoryKind::Sm3Rounds2,
            width: VecWidth::V128,
            destination: 9,
            source1: 10,
            base: 11,
            immediate,
        };
        for level in INTERPRETER_LEVELS {
            assert_memory_register_equivalent(case, 0x100 + usize::from(immediate), level);
            compared += 1;
        }
    }
    assert_eq!(compared, (all_cases().len() + 256) * 2);
    assert_eq!(compared, 624);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u8; 64],
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
        || !matches!(size, 16 | 32)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0u8; 64]
    } else {
        words_to_bytes(state.vector_scratch)
    };
    value[..size as usize].copy_from_slice(&context.value[..size as usize]);
    state.vector_scratch = bytes_to_words(value);
    1
}

#[cfg(target_arch = "x86_64")]
fn vxorps_bytes(case: Sm3Sm4MemoryCase) -> [u8; 5] {
    let scratch = case.scratch();
    [
        0xC4,
        (if case.destination < 8 { 0x80 } else { 0 })
            | 0x40
            | (if scratch < 8 { 0x20 } else { 0 })
            | 1,
        (((!case.source1) & 0x0F) << 3) | (u8::from(case.width == VecWidth::V256) << 2),
        0x57,
        0xC0 | ((case.destination & 7) << 3) | (scratch & 7),
    ]
}

#[cfg(target_arch = "x86_64")]
fn patch_sm3_sm4_to_vxorps(code: &mut [u8], case: Sm3Sm4MemoryCase) {
    let source = case.register_bytes();
    let offsets = code
        .windows(source.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == source).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "{case:?}");
    let offset = offsets[0];
    code[offset..offset + source.len()].fill(0x90);
    code[offset..offset + 5].copy_from_slice(&vxorps_bytes(case));
}

#[cfg(target_arch = "x86_64")]
fn vxorps_expected(
    mut registers: GuestRegs,
    case: Sm3Sm4MemoryCase,
    memory: [u8; 64],
) -> GuestRegs {
    let source1 = words_to_bytes(registers.zmm[usize::from(case.source1)]);
    let payload = words_to_bytes(helper_payload(case, memory));
    let mut destination = [0u8; 64];
    for byte in 0..case.width.bytes() as usize {
        destination[byte] = source1[byte] ^ payload[byte];
    }
    registers.zmm[usize::from(case.destination)] = bytes_to_words(destination);
    registers.vector_scratch = bytes_to_words(payload);
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: Sm3Sm4MemoryCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, case.width.bytes(), "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn patched_native_boundary_executes_112_success_and_112_fault_cells() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping patched SM3/SM4 helper boundary: host lacks AVX");
        return;
    }

    let cases = all_cases();
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (mut code, entry, _) = lower(&function, case);
            patch_sm3_sm4_to_vxorps(&mut code, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory = memory_bytes(case, ordinal);

            let mut context = VectorMemoryContext {
                value: memory,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = initial_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = vxorps_expected(registers, case, memory);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_observation(&context, address, case, "success");
            successes += 1;

            let mut context = VectorMemoryContext {
                value: memory,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = initial_guest_regs(case, ordinal ^ 0x55);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: helper fault");
            assert_helper_observation(&context, address, case, "fault");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
}

#[cfg(target_arch = "x86_64")]
fn execute_raw_native_case(case: Sm3Sm4MemoryCase, ordinal: usize, level: OptLevel) {
    use crate::smir::lower::runtime::ExecMem;

    let function = optimize(lift_case(case), level);
    let memory = memory_bytes(case, ordinal);
    let mut context = VectorMemoryContext {
        value: memory,
        ok: 1,
        calls: 0,
        last_addr: 0,
        last_index: 0,
        last_size: 0,
        last_zero_upper: 0,
    };
    let mut registers = initial_guest_regs(case, ordinal);
    let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
    registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
    registers.vec_load_fn = vector_load_helper as usize as u64;

    let mut expected = interpret(&function, &registers, Some(memory), address);
    expected.vector_scratch = helper_payload(case, memory);
    let (code, entry, _) = lower(&function, case);
    assert!(
        code.windows(case.register_bytes().len())
            .any(|window| window == case.register_bytes()),
        "{level:?} {case:?}"
    );
    let exec = ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    exec.run(entry, &mut registers);
    expected.host_mxcsr = registers.host_mxcsr;

    assert_eq!(registers, expected, "{level:?} {case:?}");
    assert_helper_observation(&context, address, case, "native");
}

#[cfg(target_arch = "x86_64")]
const RAW_NATIVE_CHILD_RANGE_ENV: &str = "RAX_VEX_SM3_SM4_MEMORY_CHILD_RANGE";

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
fn execute_raw_native_case_range(cases: &[Sm3Sm4MemoryCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        for level in NATIVE_LEVELS {
            execute_raw_native_case(case, ordinal, level);
        }
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
        .expect("run isolated native SM3/SM4 memory-source differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_raw_native_differential(test_name: &str, cases: &[Sm3Sm4MemoryCase]) {
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
        "isolated native SM3/SM4 memory-source failure at case {start}/{}: \
         {case:?} {:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case.bytes(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn raw_native_sm3_memory_sources_match_interpreter_for_all_roles_and_immediates() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("sm3") {
        eprintln!("skipping native SM3 memory-source differential: host lacks AVX/SM3");
        return;
    }
    let cases = sm3_semantic_cases();
    assert_eq!(cases.len(), 3 * OPERANDS.len() + 256);
    run_isolated_raw_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_sm3_sm4_memory_source::semantics::\
         raw_native_sm3_memory_sources_match_interpreter_for_all_roles_and_immediates",
        &cases,
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn raw_native_sm4_memory_sources_match_interpreter_for_all_roles_and_widths() {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("sm4") {
        eprintln!("skipping native SM4 memory-source differential: host lacks AVX/SM4");
        return;
    }
    let cases = sm4_semantic_cases();
    assert_eq!(cases.len(), 4 * OPERANDS.len());
    run_isolated_raw_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_sm3_sm4_memory_source::semantics::\
         raw_native_sm4_memory_sources_match_interpreter_for_all_roles_and_widths",
        &cases,
    );
}
