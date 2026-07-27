//! Fault-precise helper-backed lowering for MXCSR memory operations.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, MemWidth, OpId, VReg, VecWidth, X86Reg};
use crate::smir::lower::X86_GUEST_APX_ENABLED_OFFSET;
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
use crate::smir::lower::runtime::GuestRegs;

const PC: u64 = 0x2345;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn store_op(addr: Address, hint: Option<X86OpHint>) -> SmirOp {
    store_op_with_apx(addr, hint, false)
}

fn store_op_with_apx(addr: Address, hint: Option<X86OpHint>, requires_apx: bool) -> SmirOp {
    let kind = OpKind::X86StoreMxcsr { addr, requires_apx };
    match hint {
        Some(hint) => SmirOp::with_hint(OpId(0), PC, kind, hint),
        None => SmirOp::new(OpId(0), PC, kind),
    }
}

fn load_op(addr: Address, hint: Option<X86OpHint>, requires_apx: bool, next_pc: u64) -> SmirOp {
    let kind = OpKind::X86LoadMxcsr {
        addr,
        requires_apx,
        next_pc,
    };
    match hint {
        Some(hint) => SmirOp::with_hint(OpId(0), PC, kind, hint),
        None => SmirOp::new(OpId(0), PC, kind),
    }
}

fn vex_hint(w: bool) -> X86OpHint {
    X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0xAE,
        width: VecWidth::V128,
        w,
    }
}

fn lower(
    op: SmirOp,
    mem_helpers: bool,
    preserve_vectors: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    lower_with_guards(op, mem_helpers, preserve_vectors, true)
}

fn lower_with_guards(
    op: SmirOp,
    mem_helpers: bool,
    preserve_vectors: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, op.kind.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0] = op;

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    lowerer.set_avx_ymm16_vector_state(preserve_vectors);
    let result = lowerer.lower_function(&function)?;
    assert!(result.relocations.is_empty());
    Ok((lowerer.finalize()?, result.entry_offset))
}

#[test]
fn lower_mxcsr_load_requires_guards_helpers_and_encodes_validation_and_frontiers() {
    let addr = Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 0x20,
        disp_size: DispSize::Disp8,
    };
    let legacy = load_op(addr.clone(), None, false, PC + 4);
    assert!(matches!(
        lower_with_guards(legacy.clone(), true, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(legacy, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for (hint, next_pc) in [
        (None, PC + 4),
        (Some(vex_hint(false)), PC + 5),
        (Some(vex_hint(true)), PC + 6),
    ] {
        let (code, _) = lower(load_op(addr.clone(), hint, false, next_pc), true, false)
            .unwrap_or_else(|error| panic!("{hint:?}: {error:?}"));
        for (name, value) in [
            ("CR0 guard displacement", X86_GUEST_CR0_OFFSET as u32),
            ("MXCSR state displacement", X86_GUEST_MXCSR_OFFSET as u32),
            ("load helper displacement", X86_GUEST_LOAD_FN_OFFSET as u32),
            ("load width", MemWidth::B4.bytes() as u32),
            ("fault PC", PC as u32),
            ("success PC", next_pc as u32),
            (
                "reserved-bit mask",
                !crate::isa::x86_64::MXCSR_SUPPORTED_MASK,
            ),
        ] {
            assert!(
                code.windows(4).any(|window| window == value.to_le_bytes()),
                "{hint:?}: missing {name} {value:#x}: {code:02X?}"
            );
        }
        assert!(
            !code
                .windows(5)
                .any(|window| window == [0x0F, 0xAE, 0x54, 0x24, 0x18]),
            "scalar MXCSR load must not modify live host MXCSR"
        );
    }
}

#[test]
fn lower_mxcsr_store_requires_helpers_accepts_vex_wig_and_embeds_exact_store_abi() {
    let addr = Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 0x20,
        disp_size: DispSize::Disp8,
    };
    assert!(matches!(
        lower(store_op(addr.clone(), None), false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower_with_guards(store_op(addr.clone(), None), true, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for hint in [None, Some(vex_hint(false)), Some(vex_hint(true))] {
        let (code, _) = lower(store_op(addr.clone(), hint), true, false)
            .unwrap_or_else(|error| panic!("{hint:?}: {error:?}"));
        for (name, value) in [
            ("CR0 guard displacement", X86_GUEST_CR0_OFFSET as u32),
            ("MXCSR state displacement", X86_GUEST_MXCSR_OFFSET as u32),
            (
                "store helper displacement",
                X86_GUEST_STORE_FN_OFFSET as u32,
            ),
            ("store width", MemWidth::B4.bytes() as u32),
            ("fault PC", PC as u32),
        ] {
            assert!(
                code.windows(4).any(|window| window == value.to_le_bytes()),
                "{hint:?}: missing {name} {value:#x}: {code:02X?}"
            );
        }
        assert!(
            !code
                .windows(4)
                .any(|window| window == [0x0F, 0xAE, 0x1C, 0x24]),
            "scalar MXCSR store must source GuestRegs rather than live host MXCSR"
        );
    }

    let (rex2, _) = lower(
        store_op_with_apx(Address::Direct(x86(X86Reg::R31)), None, true),
        true,
        false,
    )
    .expect("lower APX-guarded STMXCSR");
    for (name, value) in [
        (
            "APX guard displacement",
            X86_GUEST_APX_ENABLED_OFFSET as u32,
        ),
        ("CR0 guard displacement", X86_GUEST_CR0_OFFSET as u32),
        (
            "store helper displacement",
            X86_GUEST_STORE_FN_OFFSET as u32,
        ),
    ] {
        assert!(
            rex2.windows(4).any(|window| window == value.to_le_bytes()),
            "REX2 store missing {name} {value:#x}: {rex2:02X?}"
        );
    }
}

#[test]
fn lower_mxcsr_operations_reject_every_non_lifter_shape() {
    let addr = Address::Absolute(0x4000);
    let malformed = store_op(
        addr.clone(),
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::None,
            opcode: 0xAE,
            width: VecWidth::V128,
            w: false,
        }),
    );
    assert!(matches!(
        lower(malformed, true, false),
        Err(LowerError::InvalidOperand { .. })
    ));

    for malformed in [
        load_op(addr.clone(), None, false, PC + 2),
        load_op(addr.clone(), None, false, PC + 16),
        load_op(addr.clone(), None, true, PC + 3),
        load_op(addr.clone(), Some(vex_hint(false)), true, PC + 4),
        load_op(
            addr.clone(),
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::None,
                opcode: 0xAE,
                width: VecWidth::V128,
                w: false,
            }),
            false,
            PC + 5,
        ),
        load_op(Address::Direct(x86(X86Reg::R31)), None, false, PC + 4),
        load_op(Address::Direct(VReg::virt(0)), None, false, PC + 3),
    ] {
        assert!(matches!(
            lower(malformed, true, false),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    for malformed in [
        store_op(Address::Direct(x86(X86Reg::R31)), None),
        store_op_with_apx(
            Address::Direct(x86(X86Reg::R31)),
            Some(vex_hint(false)),
            true,
        ),
    ] {
        assert!(matches!(
            lower(malformed, true, false),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    lower(
        load_op(Address::Direct(x86(X86Reg::R31)), None, true, PC + 4),
        true,
        false,
    )
    .expect("REX2 LDMXCSR with a guarded EGPR address");
    lower(
        store_op_with_apx(Address::Direct(x86(X86Reg::R31)), None, true),
        true,
        false,
    )
    .expect("REX2 STMXCSR with a guarded EGPR address");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    loads: u64,
    stores: u64,
    addr: u64,
    value: u64,
    size: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load_helper(
    context: *mut MemoryContext,
    addr: u64,
    size: u32,
    signed: u32,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.addr = addr;
    context.size = u64::from(size);
    assert_eq!(signed, 0);
    LoadResult {
        value: u64::from(context.value as u32),
        ok: context.ok,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store_helper(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.addr = addr;
    context.value = value;
    context.size = size;
    context.ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn initialized_regs(context: &mut MemoryContext, mxcsr: u32) -> GuestRegs {
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.gpr[3] = 0x4000;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
    regs.ctx = (context as *mut MemoryContext) as u64;
    regs.load_fn = load_helper as usize as u64;
    regs.store_fn = store_helper as usize as u64;
    regs.mxcsr = mxcsr;
    regs.mxcsr_state_active = 1;
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mxcsr_load_commits_only_after_a_valid_helper_result() {
    use crate::smir::lower::runtime::ExecMem;

    const INITIAL_MXCSR: u32 = 0x1F80;
    const VALID_MXCSR: u32 = 0x3F80;
    const RESERVED_MXCSR: u32 = 0x0001_1F80;
    const NEXT_PC: u64 = PC + 4;
    let addr = Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 0x20,
        disp_size: DispSize::Disp8,
    };
    let (code, entry) = lower(load_op(addr, None, false, NEXT_PC), true, false)
        .expect("lower helper-backed LDMXCSR");
    let exec = ExecMem::new(&code).expect("map helper-backed LDMXCSR");

    for (value, ok, expected_mxcsr, expected_pc) in [
        (VALID_MXCSR, 1, VALID_MXCSR, NEXT_PC),
        (RESERVED_MXCSR, 1, INITIAL_MXCSR, PC),
        (VALID_MXCSR, 0, INITIAL_MXCSR, PC),
    ] {
        let mut context = MemoryContext {
            value: u64::from(value),
            ok,
            ..MemoryContext::default()
        };
        let mut regs = initialized_regs(&mut context, INITIAL_MXCSR);
        let before_gpr = regs.gpr;
        let before_rflags = regs.rflags;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 1);
        assert_eq!(context.addr, 0x4020);
        assert_eq!(context.size, 4);
        assert_eq!(context.stores, 0);
        assert_eq!(regs.gpr, before_gpr);
        assert_eq!(regs.rflags, before_rflags);
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.mxcsr, expected_mxcsr);
        assert_eq!(regs.mxcsr_state_active, 1);
        assert_eq!(regs.exit_pc, expected_pc);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mxcsr_ts_guard_precedes_load_and_store_helpers_without_committing() {
    use crate::smir::lower::runtime::ExecMem;

    const INITIAL_MXCSR: u32 = 0x1F80;
    const NEXT_PC: u64 = PC + 4;
    let addr = Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 0x20,
        disp_size: DispSize::Disp8,
    };

    for (name, op) in [
        ("load", load_op(addr.clone(), None, false, NEXT_PC)),
        ("store", store_op(addr, None)),
    ] {
        let (code, entry) =
            lower(op, true, false).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let exec = ExecMem::new(&code).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let mut context = MemoryContext {
            value: 0x5F80,
            ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = initialized_regs(&mut context, INITIAL_MXCSR);
        regs.cr0 = 1 << 3;
        let before_gpr = regs.gpr;
        let before_rflags = regs.rflags;

        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 0, "{name}: load helper");
        assert_eq!(context.stores, 0, "{name}: store helper");
        assert_eq!(regs.gpr, before_gpr, "{name}: GPRs");
        assert_eq!(regs.rflags, before_rflags, "{name}: RFLAGS");
        assert_eq!(regs.ac_flag, 1, "{name}: AC");
        assert_eq!(regs.mxcsr, INITIAL_MXCSR, "{name}: MXCSR");
        assert_eq!(regs.exit_pc, PC, "{name}: deoptimization PC");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_rex2_mxcsr_load_guards_apx_before_memory() {
    use crate::smir::lower::runtime::ExecMem;

    const INITIAL_MXCSR: u32 = 0x1F80;
    const LOADED_MXCSR: u32 = 0x5F80;
    const NEXT_PC: u64 = PC + 4;
    let (code, entry) = lower(
        load_op(Address::Direct(x86(X86Reg::R31)), None, true, NEXT_PC),
        true,
        false,
    )
    .expect("lower APX-guarded LDMXCSR");
    let exec = ExecMem::new(&code).expect("map APX-guarded LDMXCSR");

    for enabled in [false, true] {
        let mut context = MemoryContext {
            value: u64::from(LOADED_MXCSR),
            ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = initialized_regs(&mut context, INITIAL_MXCSR);
        regs.gpr[31] = 0x6000;
        regs.apx_enabled = u64::from(enabled);
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, usize::from(enabled) as u64);
        if enabled {
            assert_eq!(context.addr, 0x6000);
        }
        assert_eq!(
            (regs.mxcsr, regs.exit_pc),
            if enabled {
                (LOADED_MXCSR, NEXT_PC)
            } else {
                (INITIAL_MXCSR, PC)
            }
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_rex2_mxcsr_store_guards_apx_before_ts_and_memory() {
    use crate::smir::lower::runtime::ExecMem;

    const MXCSR: u32 = 0x5F80;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    let (code, entry) = lower(
        store_op_with_apx(Address::Direct(x86(X86Reg::R31)), None, true),
        true,
        false,
    )
    .expect("lower APX-guarded STMXCSR");
    let exec = ExecMem::new(&code).expect("map APX-guarded STMXCSR");

    for enabled in [false, true] {
        let mut context = MemoryContext {
            ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = initialized_regs(&mut context, MXCSR);
        regs.gpr[31] = 0x6000;
        regs.apx_enabled = u64::from(enabled);
        regs.cr0 = if enabled { 0 } else { 1 << 3 };
        let before_gpr = regs.gpr;
        let before_rflags = regs.rflags;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 0);
        assert_eq!(context.stores, usize::from(enabled) as u64);
        if enabled {
            assert_eq!(context.addr, 0x6000);
            assert_eq!(context.value, u64::from(MXCSR));
            assert_eq!(context.size, 4);
        }
        assert_eq!(regs.gpr, before_gpr);
        assert_eq!(regs.rflags, before_rflags);
        assert_eq!(regs.mxcsr, MXCSR);
        assert_eq!(regs.exit_pc, if enabled { SENTINEL_PC } else { PC });
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mxcsr_store_is_exact_flag_neutral_and_noncommitting_on_fault() {
    use crate::smir::lower::runtime::ExecMem;

    const MXCSR: u32 = 0xFFE5;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    let addr = Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 0x20,
        disp_size: DispSize::Disp8,
    };
    let (code, entry) = lower(store_op(addr, Some(vex_hint(true))), true, false)
        .expect("lower helper-backed VSTMXCSR");
    let exec = ExecMem::new(&code).expect("map helper-backed VSTMXCSR");

    for ok in [1, 0] {
        let mut context = MemoryContext {
            ok,
            ..MemoryContext::default()
        };
        let mut regs = initialized_regs(&mut context, MXCSR);
        let before_gpr = regs.gpr;
        let before_rflags = regs.rflags;
        exec.run(entry, &mut regs);

        assert_eq!(context.stores, 1);
        assert_eq!(context.addr, 0x4020);
        assert_eq!(context.value, u64::from(MXCSR));
        assert_eq!(context.size, 4);
        assert_eq!(regs.gpr, before_gpr);
        assert_eq!(regs.rflags, before_rflags);
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.mxcsr, MXCSR);
        assert_eq!(regs.mxcsr_state_active, 1);
        assert_eq!(regs.exit_pc, if ok != 0 { SENTINEL_PC } else { PC });
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_vector_mxcsr_store_snapshots_live_guest_state_and_restores_host() {
    use crate::smir::lower::runtime::{ExecMem, X86_VECTOR_STATE_YMM16};

    if !std::is_x86_feature_detected!("avx") {
        return;
    }

    fn read_mxcsr() -> u32 {
        let mut value = 0u32;
        unsafe {
            core::arch::asm!(
                "stmxcsr [{ptr}]",
                ptr = in(reg) &mut value,
                options(nostack, preserves_flags)
            );
        }
        value
    }

    const GUEST_MXCSR: u32 = 0xFFE5;
    let (code, entry) = lower(
        store_op(Address::Absolute(0x5000), Some(vex_hint(false))),
        true,
        true,
    )
    .expect("lower vector-active VSTMXCSR");
    assert!(
        code.windows(4)
            .any(|window| window == [0x0F, 0xAE, 0x1C, 0x24]),
        "vector-active store must snapshot live MXCSR before its helper"
    );
    let exec = ExecMem::new(&code).expect("map vector-active VSTMXCSR");
    let host_before = read_mxcsr();
    let mut context = MemoryContext {
        ok: 1,
        ..MemoryContext::default()
    };
    let mut regs = initialized_regs(&mut context, GUEST_MXCSR);
    regs.vector_active = X86_VECTOR_STATE_YMM16;
    exec.run(entry, &mut regs);

    assert_eq!(context.value, u64::from(GUEST_MXCSR));
    assert_eq!(context.size, 4);
    assert_eq!(regs.mxcsr, GUEST_MXCSR);
    assert_eq!(regs.host_mxcsr, host_before);
    assert_eq!(read_mxcsr(), host_before, "guest MXCSR leaked into Rust");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_vector_mxcsr_load_updates_live_guest_state_and_restores_host() {
    use crate::smir::lower::runtime::{ExecMem, X86_VECTOR_STATE_YMM16};

    if !std::is_x86_feature_detected!("avx") {
        return;
    }

    fn read_mxcsr() -> u32 {
        let mut value = 0u32;
        unsafe {
            core::arch::asm!(
                "stmxcsr [{ptr}]",
                ptr = in(reg) &mut value,
                options(nostack, preserves_flags)
            );
        }
        value
    }

    const INITIAL_MXCSR: u32 = 0x1F80;
    const LOADED_MXCSR: u32 = 0x3F80;
    let (code, entry) = lower(
        load_op(
            Address::Absolute(0x5000),
            Some(vex_hint(false)),
            false,
            PC + 5,
        ),
        true,
        true,
    )
    .expect("lower vector-active VLDMXCSR");
    assert!(
        code.windows(5)
            .any(|window| window == [0x0F, 0xAE, 0x54, 0x24, 0x18]),
        "vector-active load must update live guest MXCSR"
    );
    let exec = ExecMem::new(&code).expect("map vector-active VLDMXCSR");
    let host_before = read_mxcsr();
    let mut context = MemoryContext {
        value: u64::from(LOADED_MXCSR),
        ok: 1,
        ..MemoryContext::default()
    };
    let mut regs = initialized_regs(&mut context, INITIAL_MXCSR);
    regs.vector_active = X86_VECTOR_STATE_YMM16;
    exec.run(entry, &mut regs);

    assert_eq!(context.loads, 1);
    assert_eq!(regs.mxcsr, LOADED_MXCSR);
    assert_eq!(regs.host_mxcsr, host_before);
    assert_eq!(read_mxcsr(), host_before, "guest MXCSR leaked into Rust");
}
