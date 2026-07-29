//! FMA4 memory/register equivalence and native helper-boundary differentials.

use super::*;

const INTERPRETER_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

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

fn memory_bytes(case: Fma4MemoryCase, ordinal: usize) -> [u8; 64] {
    const VALUES: [f64; 16] = [
        0.25, -0.25, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 4.0, -4.0, 8.0, -8.0, 16.0, -16.0, 32.0,
        -32.0,
    ];
    let (elem, _, _) = case.spec();
    let mut bytes = [0xA5; 64];
    let element_bytes = elem.bytes() as usize;
    for lane in 0..64 / element_bytes {
        let value = VALUES[(ordinal + lane * 5) % VALUES.len()];
        let bits = match elem {
            VecElementType::F32 => u64::from((value as f32).to_bits()),
            VecElementType::F64 => value.to_bits(),
            _ => unreachable!("FMA4 floating element"),
        }
        .to_le_bytes();
        bytes[lane * element_bytes..(lane + 1) * element_bytes]
            .copy_from_slice(&bits[..element_bytes]);
    }
    bytes
}

fn initial_guest_regs(case: Fma4MemoryCase, ordinal: usize) -> GuestRegs {
    let (elem, _, _) = case.spec();
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1003u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        // All SIMD exceptions masked; vary accrued status and rounding mode.
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (register, vector) in registers.zmm.iter_mut().enumerate() {
        let mut bytes = [0u8; 64];
        let element_bytes = elem.bytes() as usize;
        for lane in 0..64 / element_bytes {
            let selector = ordinal + register * 3 + lane * 7;
            let value: f64 = match selector & 7 {
                0 => 0.25,
                1 => -0.5,
                2 => 1.0,
                3 => -2.0,
                4 => 4.0,
                5 => -8.0,
                6 => 16.0,
                _ => -32.0,
            };
            let bits = match elem {
                VecElementType::F32 => u64::from((value as f32).to_bits()),
                VecElementType::F64 => value.to_bits(),
                _ => unreachable!("FMA4 floating element"),
            }
            .to_le_bytes();
            bytes[lane * element_bytes..(lane + 1) * element_bytes]
                .copy_from_slice(&bits[..element_bytes]);
        }
        *vector = bytes_to_words(bytes);
    }
    let base = case.base().expect("semantic cases use base+disp8");
    registers.gpr[usize::from(base)] = 0x2003 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

fn helper_payload(case: Fma4MemoryCase, memory: [u8; 64]) -> [u64; 8] {
    let mut payload = [0u8; 64];
    payload[..case.memory_size() as usize].copy_from_slice(&memory[..case.memory_size() as usize]);
    bytes_to_words(payload)
}

fn interpret_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: [u8; 64],
    address: u64,
    case: Fma4MemoryCase,
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
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    memory.load(
        address as usize,
        &memory_value[..case.memory_size() as usize],
    );
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
    expected.vector_scratch = helper_payload(case, memory_value);
    expected
}

fn register_interpret_destination(
    case: Fma4MemoryCase,
    level: OptLevel,
    initial: &GuestRegs,
    memory_value: [u8; 64],
) -> ([u64; 8], u32, u64) {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut function = optimize(lift_bytes(&case.register_bytes()), level);
    function.blocks[0].set_terminator(Terminator::Return { values: Vec::new() });
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.xmm[usize::from(case.scratch())][..8]
            .copy_from_slice(&helper_payload(case, memory_value));
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut destination = [0u64; 8];
    destination.copy_from_slice(&x86.xmm[usize::from(case.destination())][..8]);
    (destination, x86.mxcsr, x86.rflags)
}

#[test]
fn memory_and_byte_rewritten_register_interpretation_match_960_family_role_optimization_cells() {
    let cases = all_cases();
    assert_eq!(cases.len(), 640);
    let mut compared = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        if case.base().is_none() || case.form == MemoryForm::FsAddr32Sib {
            continue;
        }
        for level in INTERPRETER_LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_guest_regs(case, ordinal);
            let memory = memory_bytes(case, ordinal);
            let base = case.base().unwrap();
            let address = initial.gpr[usize::from(base)].wrapping_add(DISP as u64);
            let memory_result = interpret_success(&function, &initial, memory, address, case);
            let (register_destination, register_mxcsr, register_rflags) =
                register_interpret_destination(case, level, &initial, memory);
            assert_eq!(
                memory_result.zmm[usize::from(case.destination())],
                register_destination,
                "{level:?} {case:?}"
            );
            assert_eq!(memory_result.mxcsr, register_mxcsr, "{level:?} {case:?}");
            assert_eq!(memory_result.rflags, register_rflags, "{level:?} {case:?}");
            compared += 1;
        }
    }
    assert_eq!(compared, (640 - 20 * 2 * 2 * 2) * 2);
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
        || !matches!(size, 4 | 8 | 16 | 32)
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
fn vxorps_bytes(case: Fma4MemoryCase) -> [u8; 5] {
    let destination = case.destination();
    let source1 = case.source1();
    let scratch = case.scratch();
    [
        0xC4,
        (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if scratch < 8 { 0x20 } else { 0 }) | 1,
        (((!source1) & 0x0F) << 3) | (u8::from(case.width() == VecWidth::V256) << 2),
        0x57,
        0xC0 | ((destination & 7) << 3) | (scratch & 7),
    ]
}

#[cfg(target_arch = "x86_64")]
fn patch_fma4_to_vxorps(code: &mut [u8], case: Fma4MemoryCase) {
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
fn vxorps_expected(mut registers: GuestRegs, case: Fma4MemoryCase, memory: [u8; 64]) -> GuestRegs {
    let source1 = words_to_bytes(registers.zmm[usize::from(case.source1())]);
    let payload = words_to_bytes(helper_payload(case, memory));
    let mut destination = [0u8; 64];
    for byte in 0..case.width().bytes() as usize {
        destination[byte] = source1[byte] ^ payload[byte];
    }
    registers.zmm[usize::from(case.destination())] = bytes_to_words(destination);
    registers.vector_scratch = bytes_to_words(payload);
    registers
}

#[cfg(target_arch = "x86_64")]
#[test]
fn patched_native_boundary_executes_640_successes_and_faults_with_exact_state() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping patched FMA4 helper boundary: host lacks AVX");
        return;
    }

    let cases = native_cases();
    assert_eq!(cases.len(), 20 * 2 * 2 * 4);
    let expected_executions = cases.len() * NATIVE_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in NATIVE_LEVELS {
            let function = optimize(lift_case(case), level);
            let (mut code, entry, _) = lower(&function, case);
            patch_fma4_to_vxorps(&mut code, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory = memory_bytes(case, ordinal);
            let base = case.base().unwrap();

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
            let address = registers.gpr[usize::from(base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = vxorps_expected(registers, case, memory);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
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
            let address = registers.gpr[usize::from(base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
            assert_eq!(context.last_addr, address, "fault {level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.memory_size(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!("executed {successes} successful and {faults} faulting patched FMA4 cases");
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_FMA4_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn real_native_cases() -> Vec<(OptLevel, Fma4MemoryCase)> {
    let cases = native_cases();
    let mut flattened = Vec::with_capacity(cases.len() * NATIVE_LEVELS.len());
    for case in cases {
        for level in NATIVE_LEVELS {
            flattened.push((level, case));
        }
    }
    flattened
}

#[cfg(target_arch = "x86_64")]
fn execute_real_native_range(cases: &[(OptLevel, Fma4MemoryCase)], range: std::ops::Range<usize>) {
    use crate::smir::lower::runtime::ExecMem;

    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &(level, case)) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let function = optimize(lift_case(case), level);
        let (code, entry, _) = lower(&function, case);
        let exec =
            ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
        let memory = memory_bytes(case, ordinal);
        let base = case.base().unwrap();
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
        let address = registers.gpr[usize::from(base)].wrapping_add(DISP as u64);
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let mut expected = interpret_success(&function, &registers, memory, address, case);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(registers, expected, "{level:?} {case:?}");
        assert_eq!(context.calls, 1, "{level:?} {case:?}");
        assert_eq!(context.last_addr, address, "{level:?} {case:?}");
        assert_eq!(context.last_size, case.memory_size(), "{level:?} {case:?}");
        assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
    }
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native FMA4 memory differential")
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_fma4_memory_matches_o0_o2_interpretation_when_host_supports_fma4() {
    if !std::is_x86_feature_detected!("avx") || !x86_host_has_fma4() {
        eprintln!("skipping native FMA4 memory differential: host lacks AVX/FMA4");
        return;
    }

    let cases = real_native_cases();
    assert_eq!(cases.len(), 20 * 2 * 2 * 4 * 2);
    if let Some(range) = child_range() {
        execute_real_native_range(&cases, range);
        return;
    }

    let test_name = "smir::lower::runtime::jit_gate_tests::vex_fma4_memory_source::semantics::\
        native_fma4_memory_matches_o0_o2_interpretation_when_host_supports_fma4";
    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }

    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let (level, case) = cases[start];
    panic!(
        "isolated native FMA4 memory failure at case {start}/{}: \
         {level:?} {case:?} {:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        case.bytes(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}
