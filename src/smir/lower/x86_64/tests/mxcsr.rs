//! Fault-precise helper-backed lowering for STMXCSR/VSTMXCSR.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, MemWidth, OpId, VReg, VecWidth, X86Reg};
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
use crate::smir::lower::runtime::GuestRegs;

const PC: u64 = 0x2345;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn store_op(addr: Address, hint: Option<X86OpHint>) -> SmirOp {
    let kind = OpKind::X86StoreMxcsr { addr };
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
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, op.kind.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0] = op;

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    lowerer.set_avx_ymm16_vector_state(preserve_vectors);
    let result = lowerer.lower_function(&function)?;
    assert!(result.relocations.is_empty());
    Ok((lowerer.finalize()?, result.entry_offset))
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

    for hint in [None, Some(vex_hint(false)), Some(vex_hint(true))] {
        let (code, _) = lower(store_op(addr.clone(), hint), true, false)
            .unwrap_or_else(|error| panic!("{hint:?}: {error:?}"));
        for (name, value) in [
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
}

#[test]
fn lower_mxcsr_store_rejects_malformed_hints_and_loads() {
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

    assert!(matches!(
        lower(
            SmirOp::new(OpId(0), PC, OpKind::X86LoadMxcsr { addr }),
            true,
            false,
        ),
        Err(LowerError::UnsupportedOp { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    stores: u64,
    addr: u64,
    value: u64,
    size: u64,
    ok: u64,
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
    regs.store_fn = store_helper as usize as u64;
    regs.mxcsr = mxcsr;
    regs.mxcsr_state_active = 1;
    regs
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
