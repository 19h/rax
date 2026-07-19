//! Helper-backed lowering tests for explicit x86 long-mode addr32 addresses.

use super::*;
use crate::smir::ir::{SmirBlock, SmirFunction};

const PC: u64 = 0x1000;
const ADDR32_LOAD: [u8; 6] = [0x67, 0x48, 0x8B, 0x44, 0x8B, 0x20];
const ADDR32_FS_LOAD: [u8; 7] = [0x67, 0x64, 0x48, 0x8B, 0x44, 0x8B, 0x20];
const ADDR32_EIP_LOAD: [u8; 8] = [0x67, 0x48, 0x8B, 0x05, 0xFC, 0xFF, 0xFF, 0xFF];
const ADDR32_STORE: [u8; 6] = [0x67, 0x48, 0x89, 0x44, 0x8B, 0x20];

fn lift_function(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .expect("lift addr32 memory instruction");
    let mut block = SmirBlock::new(crate::smir::ir::types::BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn lower_function(bytes: &[u8], mem_helpers: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let function = lift_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    let result = lowerer.lower_function(&function)?;
    assert!(
        result.relocations.is_empty(),
        "addr32 memory helper must not relocate"
    );
    Ok((lowerer.finalize()?, result.entry_offset))
}

#[test]
fn addr32_scalar_memory_lowering_requires_helpers_and_emits_w32_address_math() {
    assert!(matches!(
        lower_function(&ADDR32_LOAD, false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let (code, _) = lower_function(&ADDR32_LOAD, true).expect("lower helper-backed addr32 load");
    let wrapped_sib = [
        0x48, 0x8B, 0xB0, 0x18, 0x00, 0x00, 0x00, // RSI = guest RBX
        0x89, 0xF6, // zero-extend EBX through ESI
        0x48, 0x8B, 0xB8, 0x08, 0x00, 0x00, 0x00, // RDI = guest RCX
        0x89, 0xFF, // zero-extend ECX through EDI
        0xC1, 0xE7, 0x02, // EDI <<= 2
        0x01, 0xFE, // ESI += EDI modulo 2^32
        0x81, 0xC6, 0x20, 0x00, 0x00, 0x00, // ESI += 20h modulo 2^32
    ];
    assert!(
        code.windows(wrapped_sib.len())
            .any(|window| window == wrapped_sib),
        "missing explicit W32 base/index/scale/displacement construction"
    );

    lower_function(&ADDR32_STORE, true).expect("lower helper-backed addr32 store");
    lower_function(&ADDR32_FS_LOAD, true).expect("lower helper-backed FS addr32 load");
    lower_function(&ADDR32_EIP_LOAD, true).expect("lower helper-backed EIP addr32 load");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    load_value: u64,
    load_ok: u64,
    store_ok: u64,
    last_addr: u64,
    last_size: u64,
    last_value: u64,
    loads: u64,
    stores: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load(context: *mut MemoryContext, addr: u64, size: u64, _signed: u64) -> LoadResult {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    context.last_size = size;
    LoadResult {
        value: context.load_value,
        ok: context.load_ok,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_value = value;
    context.store_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_addr32_scalar_memory_wraps_before_segment_add_and_preserves_fault_state() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const FLAGS: u64 = 0xCD7;
    const STATUS_MASK: u64 = 0x8D5;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    const LOAD_VALUE: u64 = 0x0123_4567_89AB_CDEF;
    const STORE_VALUE: u64 = 0xFEDC_BA98_7654_3210;

    let mut initial_gprs = [0u64; 32];
    for (index, value) in initial_gprs.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    initial_gprs[0] = STORE_VALUE;
    initial_gprs[3] = 0xAAAA_BBBB_FFFF_FFF0; // EBX = FFFF_FFF0h
    initial_gprs[1] = 0xCCCC_DDDD_0000_0008; // ECX = 8h
    let wrapped = u64::from(
        (initial_gprs[3] as u32)
            .wrapping_add((initial_gprs[1] as u32).wrapping_mul(4))
            .wrapping_add(0x20),
    );
    assert_eq!(wrapped, 0x30);

    let run_load = |bytes: &[u8], fs_base: u64, expected_addr: u64| {
        let (code, entry) = lower_function(bytes, true).expect("lower native addr32 load");
        let exec = ExecMem::new(&code).expect("map native addr32 load");
        let mut context = MemoryContext {
            load_value: LOAD_VALUE,
            load_ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.rflags = FLAGS;
        regs.exit_pc = SENTINEL_PC;
        regs.fs_base = fs_base;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.load_fn = load as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 1);
        assert_eq!(context.last_addr, expected_addr);
        assert_eq!(context.last_size, 8);
        let mut expected_gprs = initial_gprs;
        expected_gprs[0] = LOAD_VALUE;
        assert_eq!(regs.gpr, expected_gprs);
        assert_eq!(regs.rflags & STATUS_MASK, FLAGS & STATUS_MASK);
        assert_eq!(regs.exit_pc, SENTINEL_PC);
    };

    run_load(&ADDR32_LOAD, 0, wrapped);
    let fs_base = 0x1234_5678_0000_0000;
    run_load(&ADDR32_FS_LOAD, fs_base, fs_base.wrapping_add(wrapped));
    // next EIP = 1008h; 1008h + FFFF_FFFCh = 1004h modulo 2^32.
    run_load(&ADDR32_EIP_LOAD, 0, 0x1004);

    let (load_code, load_entry) =
        lower_function(&ADDR32_LOAD, true).expect("lower faulting addr32 load");
    let load_exec = ExecMem::new(&load_code).expect("map faulting addr32 load");
    let mut load_fault_context = MemoryContext {
        load_value: LOAD_VALUE,
        load_ok: 0,
        ..MemoryContext::default()
    };
    let mut load_fault = GuestRegs::default();
    load_fault.gpr = initial_gprs;
    load_fault.rflags = FLAGS;
    load_fault.exit_pc = SENTINEL_PC;
    load_fault.ctx = (&mut load_fault_context as *mut MemoryContext) as u64;
    load_fault.load_fn = load as usize as u64;
    load_exec.run(load_entry, &mut load_fault);
    assert_eq!(load_fault_context.last_addr, wrapped);
    assert_eq!(
        load_fault.gpr, initial_gprs,
        "faulting load must not commit"
    );
    assert_eq!(load_fault.rflags & STATUS_MASK, FLAGS & STATUS_MASK);
    assert_eq!(load_fault.exit_pc, PC);

    let (store_code, store_entry) =
        lower_function(&ADDR32_STORE, true).expect("lower native addr32 store");
    let store_exec = ExecMem::new(&store_code).expect("map native addr32 store");
    for (store_ok, expected_exit) in [(1, SENTINEL_PC), (0, PC)] {
        let mut context = MemoryContext {
            store_ok,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial_gprs;
        regs.rflags = FLAGS;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.store_fn = store as usize as u64;
        store_exec.run(store_entry, &mut regs);

        assert_eq!(context.stores, 1);
        assert_eq!(context.last_addr, wrapped);
        assert_eq!(context.last_size, 8);
        assert_eq!(context.last_value, STORE_VALUE);
        assert_eq!(regs.gpr, initial_gprs, "store must not modify GPR state");
        assert_eq!(regs.rflags & STATUS_MASK, FLAGS & STATUS_MASK);
        assert_eq!(regs.exit_pc, expected_exit);
    }
}
