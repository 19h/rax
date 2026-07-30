//! Native per-lane success, suppression, and precise-fault differentials.

#[cfg(target_arch = "x86_64")]
use super::*;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct ScalarMemoryContext {
    base: u64,
    bytes: [u8; 64],
    fail_address: Option<u64>,
    calls: Vec<(u64, u64, Option<u64>)>,
    commits: Vec<(u64, u64, u64)>,
}

#[cfg(target_arch = "x86_64")]
impl ScalarMemoryContext {
    fn new(seed: usize) -> Self {
        Self {
            base: 0x4000,
            bytes: std::array::from_fn(|index| {
                (index as u8)
                    .wrapping_mul(0x3D)
                    .wrapping_add((seed as u8).wrapping_mul(0x17))
                    .wrapping_add(0x29)
            }),
            fail_address: None,
            calls: Vec::new(),
            commits: Vec::new(),
        }
    }
}

#[cfg(target_arch = "x86_64")]
extern "C" fn load_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls.push((address, size, None));
    if signed != 0 || context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    let width = usize::try_from(size).unwrap();
    assert!(matches!(width, 4 | 8));
    let mut raw = [0u8; 8];
    raw[..width].copy_from_slice(&context.bytes[offset..offset + width]);
    LoadResult {
        value: u64::from_le_bytes(raw),
        ok: 1,
    }
}

#[cfg(target_arch = "x86_64")]
extern "C" fn store_helper(
    context: *mut ScalarMemoryContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    context.calls.push((address, size, Some(value)));
    if context.fail_address == Some(address) {
        return 0;
    }
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    let width = usize::try_from(size).unwrap();
    assert!(matches!(width, 4 | 8));
    context.bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
    context.commits.push((address, size, value));
    1
}

#[cfg(target_arch = "x86_64")]
fn vector_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(target_arch = "x86_64")]
fn vector_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn patterned_registers(case: MaskedMemoryCase, seed: usize, active: &[bool]) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1020_4081_0204_0810u64.rotate_left(((seed + index * 9) & 63) as u32)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x195)) & 0x8D5),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: std::array::from_fn(|index| 0xA55A_6996_F00F_3CC3u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((seed as u32) & 0x3F) | (((seed as u32) & 3) << 13),
        vector_scratch: std::array::from_fn(|index| {
            0xCCDD_EEFF_0011_2233u64 ^ (index as u64).wrapping_mul(0x1111_1111_1111_1111)
        }),
        cr0: 1,
        cr4: 1 << 18,
        xcr0: 0b110,
        cs_l: 1,
        ..GuestRegs::default()
    };
    for (register, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64
                .rotate_left(((seed * 5 + register * 11 + word * 17) & 63) as u32)
                ^ (register as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
        });
    }
    let mut mask = vector_bytes(registers.zmm[usize::from(case.mask)]);
    let element_bytes = case.elem().bytes() as usize;
    for (lane, enabled) in active.iter().copied().enumerate() {
        let msb = lane * element_bytes + element_bytes - 1;
        mask[msb] = (mask[msb] & 0x7F) | if enabled { 0x80 } else { 0 };
    }
    registers.zmm[usize::from(case.mask)] = vector_words(mask);
    registers.gpr[usize::from(case.base)] = 0x4000 - u64::from(DISP);
    registers
}

#[cfg(target_arch = "x86_64")]
fn active_pattern(lanes: usize) -> Vec<bool> {
    (0..lanes).map(|lane| lane % 3 != 2).collect()
}

#[cfg(target_arch = "x86_64")]
fn active_addresses(case: MaskedMemoryCase, active: &[bool], base: u64) -> Vec<u64> {
    let stride = u64::from(case.elem().bytes());
    active
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(lane, enabled)| enabled.then_some(base + lane as u64 * stride))
        .collect()
}

#[cfg(target_arch = "x86_64")]
fn assert_calls(context: &ScalarMemoryContext, case: MaskedMemoryCase, expected_addresses: &[u64]) {
    assert_eq!(context.calls.len(), expected_addresses.len(), "{case:?}");
    for ((address, size, _), expected_address) in
        context.calls.iter().zip(expected_addresses.iter())
    {
        assert_eq!(address, expected_address, "{case:?}");
        assert_eq!(*size, u64::from(case.elem().bytes()), "{case:?}");
    }
}

#[cfg(target_arch = "x86_64")]
fn expected_load(
    initial: GuestRegs,
    memory: &[u8; 64],
    case: MaskedMemoryCase,
    active: &[bool],
) -> GuestRegs {
    let mut expected = initial;
    let mut destination = [0u8; 64];
    let element_bytes = case.elem().bytes() as usize;
    for (lane, enabled) in active.iter().copied().enumerate() {
        if enabled {
            let range = lane * element_bytes..(lane + 1) * element_bytes;
            destination[range.clone()].copy_from_slice(&memory[range]);
        }
    }
    expected.zmm[usize::from(case.vector)] = vector_words(destination);
    expected
}

#[cfg(target_arch = "x86_64")]
fn expected_store_bytes(
    initial: &GuestRegs,
    original: [u8; 64],
    case: MaskedMemoryCase,
    active: &[bool],
    completed: usize,
) -> [u8; 64] {
    let mut expected = original;
    let data = vector_bytes(initial.zmm[usize::from(case.vector)]);
    let element_bytes = case.elem().bytes() as usize;
    let mut remaining = completed;
    for (lane, enabled) in active.iter().copied().enumerate() {
        if enabled && remaining != 0 {
            let range = lane * element_bytes..(lane + 1) * element_bytes;
            expected[range.clone()].copy_from_slice(&data[range]);
            remaining -= 1;
        }
    }
    assert_eq!(remaining, 0);
    expected
}

#[cfg(target_arch = "x86_64")]
fn execute(
    case: MaskedMemoryCase,
    level: OptLevel,
    seed: usize,
    active: Vec<bool>,
    fault_active_ordinal: Option<usize>,
) {
    let function = optimize(lift_case(case), level);
    let (code, entry, _) = lower(&function, case);
    let executable = ExecMem::new(&code)
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: executable memory: {error:?}"));

    let mut context = ScalarMemoryContext::new(seed);
    let original_memory = context.bytes;
    let addresses = active_addresses(case, &active, context.base);
    if let Some(ordinal) = fault_active_ordinal {
        context.fail_address = Some(addresses[ordinal]);
    }
    let mut registers = patterned_registers(case, seed, &active);
    registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
    registers.load_fn = load_helper as usize as u64;
    registers.store_fn = store_helper as usize as u64;
    let initial = registers;

    executable.run(entry, &mut registers);
    let observed_host_mxcsr = registers.host_mxcsr;
    let expected_call_count = fault_active_ordinal.map_or(addresses.len(), |ordinal| ordinal + 1);
    assert_calls(&context, case, &addresses[..expected_call_count]);

    if let Some(fault_ordinal) = fault_active_ordinal {
        let mut expected = initial;
        expected.exit_pc = PC;
        expected.host_mxcsr = observed_host_mxcsr;
        assert_eq!(registers, expected, "{level:?} {case:?}: fault state");
        if case.load() {
            assert_eq!(
                context.bytes, original_memory,
                "{level:?} {case:?}: load fault memory"
            );
        } else {
            assert_eq!(
                context.bytes,
                expected_store_bytes(&initial, original_memory, case, &active, fault_ordinal,),
                "{level:?} {case:?}: partial store"
            );
            assert_eq!(context.commits.len(), fault_ordinal, "{level:?} {case:?}");
        }
    } else if case.load() {
        let mut expected = expected_load(initial, &original_memory, case, &active);
        expected.host_mxcsr = observed_host_mxcsr;
        assert_eq!(registers, expected, "{level:?} {case:?}: load success");
        assert_eq!(context.bytes, original_memory, "{level:?} {case:?}");
    } else {
        let mut expected = initial;
        expected.host_mxcsr = observed_host_mxcsr;
        assert_eq!(registers, expected, "{level:?} {case:?}: store success");
        assert_eq!(
            context.bytes,
            expected_store_bytes(&initial, original_memory, case, &active, addresses.len()),
            "{level:?} {case:?}: store bytes"
        );
        assert_eq!(context.commits.len(), addresses.len(), "{level:?} {case:?}");
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    instruction: MaskedMemoryCase,
    level: OptLevel,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for (opcode, w) in families() {
        for width in [VecWidth::V128, VecWidth::V256] {
            for level in [OptLevel::O0, OptLevel::O2] {
                let vector = [0, 4, 9, 15][ordinal & 3];
                cases.push(NativeCase {
                    instruction: MaskedMemoryCase {
                        opcode,
                        w,
                        width,
                        mask: if ordinal % 3 == 0 {
                            vector
                        } else {
                            [1, 13, 15][ordinal % 3]
                        },
                        vector,
                        base: [4, 5, 12, 14][ordinal & 3],
                    },
                    level,
                    seed: ordinal,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case(case: NativeCase) {
    let lanes = case.instruction.lanes();
    let active = active_pattern(lanes);
    execute(
        case.instruction,
        case.level,
        case.seed,
        active.clone(),
        None,
    );
    execute(
        case.instruction,
        case.level,
        case.seed ^ 0x55,
        active,
        Some(1),
    );
    execute(
        case.instruction,
        case.level,
        case.seed ^ 0xAA,
        vec![false; lanes],
        None,
    );
}

#[cfg(target_arch = "x86_64")]
const RAW_NATIVE_CHILD_RANGE_ENV: &str = "RAX_VEX_MASKED_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn raw_native_child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(RAW_NATIVE_CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {RAW_NATIVE_CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid range start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid range end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn run_child(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(
            RAW_NATIVE_CHILD_RANGE_ENV,
            format!("{}:{}", range.start, range.end),
        )
        .output()
        .expect("run isolated native VEX masked-memory differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated(test_name: &str, cases: &[NativeCase]) {
    if let Some(range) = raw_native_child_range() {
        for case in &cases[range] {
            execute_native_case(*case);
        }
        return;
    }
    let whole = run_child(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child(test_name, start..end);
    panic!(
        "isolated VEX masked-memory failure at {start}/{}: {:?}; whole {}; \
         singleton {}; stdout: {}; stderr: {}",
        cases.len(),
        cases[start],
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn raw_native_all_32_family_width_optimization_cells_preserve_lane_fault_semantics() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX masked-memory differential: host lacks AVX");
        return;
    }
    let cases = native_cases();
    assert_eq!(cases.len(), 8 * 2 * 2);
    run_isolated(
        "smir::lower::runtime::jit_gate_tests::vex_masked_memory::semantics::\
         raw_native_all_32_family_width_optimization_cells_preserve_lane_fault_semantics",
        &cases,
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn raw_native_addr32_fs_wraps_effective_offset_before_linear_lane_offsets() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX masked-memory addr32 differential: host lacks AVX");
        return;
    }
    let case = MaskedMemoryCase {
        opcode: 0x2C,
        w: false,
        width: VecWidth::V128,
        mask: 1,
        vector: 2,
        base: 14,
    };
    let bytes = [0x64, 0x67, 0xC4, 0xC2, 0x71, 0x2C, 0x56, 0x20];
    let function = optimize(lift_bytes(&bytes), OptLevel::O2);
    assert_exact_graph(&function, case);
    let (code, entry, _) = lower(&function, case);
    let executable = ExecMem::new(&code).expect("addr32 masked-memory executable memory");

    let active = vec![true; case.lanes()];
    let mut context = ScalarMemoryContext::new(0x32);
    context.base = 0x5010;
    let original_memory = context.bytes;
    let mut registers = patterned_registers(case, 0x32, &active);
    registers.gpr[14] = 0xFFFF_FFF0;
    registers.fs_base = 0x5000;
    registers.ctx = (&mut context as *mut ScalarMemoryContext) as u64;
    registers.load_fn = load_helper as usize as u64;
    registers.store_fn = store_helper as usize as u64;
    let initial = registers;

    executable.run(entry, &mut registers);
    assert_calls(&context, case, &[0x5010, 0x5014, 0x5018, 0x501C]);
    let mut expected = expected_load(initial, &original_memory, case, &active);
    expected.host_mxcsr = registers.host_mxcsr;
    assert_eq!(registers, expected);
}
