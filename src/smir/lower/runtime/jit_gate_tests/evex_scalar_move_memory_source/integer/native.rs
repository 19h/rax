use super::*;
use crate::smir::lower::runtime::ExecMem;

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[derive(Clone, Debug)]
struct MemoryContext {
    bytes: [u8; 8],
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
    last_store_value: u64,
}

impl MemoryContext {
    fn new(bytes: [u8; 8], ok: bool) -> Self {
        Self {
            bytes,
            ok: u64::from(ok),
            calls: 0,
            last_addr: 0,
            last_size: 0,
            last_signed: 0,
            last_store_value: 0,
        }
    }
}

extern "C" fn load_helper(
    context: *mut MemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = address;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: u64::from_le_bytes(context.bytes),
        ok: context.ok,
    }
}

extern "C" fn store_helper(
    context: *mut MemoryContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = address;
    context.last_size = size;
    context.last_store_value = value;
    if context.ok == 0 {
        return 0;
    }
    let size = usize::try_from(size).unwrap();
    context.bytes[..size].copy_from_slice(&value.to_le_bytes()[..size]);
    1
}

fn bind_helpers(registers: &mut GuestRegs, context: &mut MemoryContext) {
    registers.ctx = (context as *mut MemoryContext) as u64;
    registers.load_fn = load_helper as usize as u64;
    registers.store_fn = store_helper as usize as u64;
}

fn success_oracle(initial: &GuestRegs, before: [u8; 8], case: IntegerCase) -> (GuestRegs, [u8; 8]) {
    let mut registers = *initial;
    let mut memory = before;
    let width = case.selector.memory_width().bytes() as usize;
    match case.selector.kind() {
        X86EvexScalarMoveMemoryKind::Load => {
            let mut scalar = [0u8; 8];
            scalar[..width].copy_from_slice(&before[..width]);
            registers.zmm[usize::from(case.vector)] = [0; 8];
            registers.zmm[usize::from(case.vector)][0] = u64::from_le_bytes(scalar);
        }
        X86EvexScalarMoveMemoryKind::Store => {
            let scalar = initial.zmm[usize::from(case.vector)][0].to_le_bytes();
            memory[..width].copy_from_slice(&scalar[..width]);
        }
    }
    (registers, memory)
}

#[test]
fn native_integer_scalar_moves_match_independent_success_and_fault_oracles() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!(
            "skipping native EVEX integer scalar memory differential: host lacks AVX-512F/BW"
        );
        return;
    }

    let fp16 = std::is_x86_feature_detected!("avx512fp16");
    let vectors = [0u8, 8, 16, 31];
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (selector_ordinal, selector) in IntegerSelector::ALL.into_iter().enumerate() {
        if selector.needs_avx512fp16() && !fp16 {
            continue;
        }
        for vector in vectors {
            let case = IntegerCase {
                selector,
                vector,
                base: 2,
            };
            let seed = selector_ordinal * vectors.len() + usize::from(vector);
            for level in [OptLevel::O0, OptLevel::O2] {
                let function = optimize(lift_case(case), level);
                let (code, entry) = lower_case(&function, case);
                let exec = ExecMem::new(&code)
                    .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                let memory_before = (0xA1B2_C3D4_E5F6_0718u64
                    ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081))
                .to_le_bytes();

                let mut context = MemoryContext::new(memory_before, true);
                let mut registers = full_registers(case, seed);
                bind_helpers(&mut registers, &mut context);
                let (mut expected, expected_memory) =
                    success_oracle(&registers, memory_before, case);

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: success");
                assert_eq!(context.calls, 1, "{level:?} {case:?}");
                assert_eq!(context.last_addr, MEMORY_ADDRESS, "{level:?} {case:?}");
                assert_eq!(
                    context.last_size,
                    selector.memory_width().bytes() as u64,
                    "{level:?} {case:?}"
                );
                assert_eq!(context.bytes, expected_memory, "{level:?} {case:?}");
                match selector.kind() {
                    X86EvexScalarMoveMemoryKind::Load => {
                        assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
                    }
                    X86EvexScalarMoveMemoryKind::Store => {
                        let width = selector.memory_width().bytes() as usize;
                        let mask = if width == 8 {
                            u64::MAX
                        } else {
                            (1u64 << (width * 8)) - 1
                        };
                        assert_eq!(
                            context.last_store_value & mask,
                            expected.zmm[usize::from(vector)][0] & mask,
                            "{level:?} {case:?}"
                        );
                    }
                }
                successes += 1;

                let fault_memory = 0x55AA_33CC_0FF0_9696u64.to_le_bytes();
                let mut fault_context = MemoryContext::new(fault_memory, false);
                let mut fault_registers = full_registers(case, seed ^ 0x55);
                bind_helpers(&mut fault_registers, &mut fault_context);
                let mut fault_expected = fault_registers;
                fault_expected.exit_pc = PC;

                exec.run(entry, &mut fault_registers);
                fault_expected.host_mxcsr = fault_registers.host_mxcsr;
                assert_eq!(
                    fault_registers, fault_expected,
                    "{level:?} {case:?}: fault committed architectural state"
                );
                assert_eq!(fault_context.calls, 1, "{level:?} {case:?}");
                assert_eq!(fault_context.last_addr, MEMORY_ADDRESS);
                assert_eq!(
                    fault_context.last_size,
                    selector.memory_width().bytes() as u64
                );
                assert_eq!(fault_context.bytes, fault_memory, "{level:?} {case:?}");
                faults += 1;
            }
        }
    }

    let supported_selectors = 6 + if fp16 { 4 } else { 0 };
    assert_eq!(successes, supported_selectors * vectors.len() * 2);
    assert_eq!(faults, successes);
}
