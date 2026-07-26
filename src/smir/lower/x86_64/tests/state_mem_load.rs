//! Helper-backed scalar loads into state-backed guest RSP/RBP.
//!
//! The MMU-helper load path already delivers its result into the destination's
//! `GuestRegs` slot, so guest RSP/RBP are valid load destinations: the host
//! stack pointer and native frame pointer are never written. An RBP
//! destination additionally re-synchronizes the prologue-saved guest word.

use super::*;
use crate::smir::lower::SmirLowerer;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

const PC: u64 = 0x1000;

fn load_op(dst: X86Reg, width: MemWidth, sign: SignExtend) -> OpKind {
    OpKind::Load {
        dst: x86(dst),
        addr: Address::BaseOffset {
            base: x86(X86Reg::Rbx),
            offset: 8,
            disp_size: DispSize::Disp8,
        },
        width,
        sign,
    }
}

fn lower_mem_load(dst: X86Reg, width: MemWidth, sign: SignExtend) -> (Vec<u8>, usize) {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, load_op(dst, width, sign));
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let result = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed load");
    (lowerer.finalize().expect("finalize"), result.entry_offset)
}

#[test]
fn helper_backed_loads_commit_stack_destinations_through_the_guest_file() {
    // GuestRegs.gpr[4] (RSP) is at +20h, gpr[5] (RBP) at +28h; the commit is a
    // `mov [rcx+disp32], rax` against the state pointer, never a host write.
    let (rsp, _) = lower_mem_load(X86Reg::Rsp, MemWidth::B8, SignExtend::Zero);
    assert!(
        rsp.windows(7)
            .any(|b| b == [0x48, 0x89, 0x81, 0x20, 0x00, 0x00, 0x00]),
        "must commit the guest RSP slot: {rsp:02X?}"
    );
    assert!(
        !rsp.windows(4).any(|b| b == [0x48, 0x89, 0x45, 0x00]),
        "an RSP destination must not touch the saved guest RBP word: {rsp:02X?}"
    );

    let (rbp, _) = lower_mem_load(X86Reg::Rbp, MemWidth::B8, SignExtend::Zero);
    assert!(
        rbp.windows(7)
            .any(|b| b == [0x48, 0x89, 0x81, 0x28, 0x00, 0x00, 0x00]),
        "must commit the guest RBP slot: {rbp:02X?}"
    );
    assert!(
        rbp.windows(4).any(|b| b == [0x48, 0x8B, 0x41, 0x28]),
        "must reload the committed guest RBP value: {rbp:02X?}"
    );
    assert!(
        rbp.windows(4).any(|b| b == [0x48, 0x89, 0x45, 0x00]),
        "must synchronize the saved guest RBP word: {rbp:02X?}"
    );

    // Partial-width destinations keep x86 merge semantics inside the slot.
    let (byte, _) = lower_mem_load(X86Reg::Rsp, MemWidth::B1, SignExtend::Zero);
    assert!(
        byte.windows(6)
            .any(|b| b == [0x88, 0x81, 0x20, 0x00, 0x00, 0x00]),
        "byte load must write only SPL in the slot: {byte:02X?}"
    );
    let (word, _) = lower_mem_load(X86Reg::Rbp, MemWidth::B2, SignExtend::Zero);
    assert!(
        word.windows(7)
            .any(|b| b == [0x66, 0x89, 0x81, 0x28, 0x00, 0x00, 0x00]),
        "word load must write only BP in the slot: {word:02X?}"
    );
    assert!(
        word.windows(4).any(|b| b == [0x48, 0x89, 0x45, 0x00]),
        "a partial RBP load must still synchronize the saved word: {word:02X?}"
    );

    // A signed narrow load is already sign-extended by the helper and therefore
    // replaces the complete architectural register rather than merging.
    let (signed_byte, _) = lower_mem_load(X86Reg::Rsp, MemWidth::B1, SignExtend::Sign);
    assert!(
        signed_byte
            .windows(7)
            .any(|b| b == [0x48, 0x89, 0x81, 0x20, 0x00, 0x00, 0x00]),
        "signed byte load must replace the complete guest RSP slot: {signed_byte:02X?}"
    );
    assert!(
        !signed_byte
            .windows(6)
            .any(|b| b == [0x88, 0x81, 0x20, 0x00, 0x00, 0x00]),
        "signed byte load must not use partial SPL merge: {signed_byte:02X?}"
    );
}

#[test]
fn stack_destination_loads_require_the_memory_helper_path() {
    // Without MMU helpers the load would target the host register of the same
    // name, so it must still be rejected.
    for dst in [X86Reg::Rsp, X86Reg::Rbp] {
        let mut builder = FunctionBuilder::new(FunctionId(0), PC);
        builder.push_op(PC, load_op(dst, MemWidth::B8, SignExtend::Zero));
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        assert!(
            matches!(
                lowerer.lower_function(&builder.finish()),
                Err(LowerError::InvalidRegister(_))
            ),
            "{dst:?} destination must be rejected without MMU helpers"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    loads: u64,
    last_addr: u64,
    last_size: u32,
    last_signed: u32,
    load_value: u64,
    load_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load(context: *mut MemoryContext, addr: u64, size: u32, signed: u32) -> LoadResult {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    let raw = context.load_value;
    let value = match (size, signed != 0) {
        (1, false) => u64::from(raw as u8),
        (2, false) => u64::from(raw as u16),
        (4, false) => u64::from(raw as u32),
        (1, true) => (raw as u8 as i8 as i64) as u64,
        (2, true) => (raw as u16 as i16 as i64) as u64,
        (4, true) => (raw as u32 as i32 as i64) as u64,
        (8, _) => raw,
        _ => {
            return LoadResult { value: 0, ok: 0 };
        }
    };
    LoadResult {
        value,
        ok: context.load_ok,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_helper_backed_stack_loads_commit_only_the_guest_file() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const FLAGS: u64 = 0xCD7;
    const STATUS_MASK: u64 = 0x8D5;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    const LOAD_VALUE: u64 = 0x0123_4567_89AB_CDEF;

    let mut initial = [0u64; 32];
    for (index, value) in initial.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    initial[3] = 0x2000; // RBX base

    for (dst, slot, width, sign, expected) in [
        (
            X86Reg::Rsp,
            4usize,
            MemWidth::B8,
            SignExtend::Zero,
            LOAD_VALUE,
        ),
        (X86Reg::Rbp, 5, MemWidth::B8, SignExtend::Zero, LOAD_VALUE),
        (
            X86Reg::Rsp,
            4,
            MemWidth::B4,
            SignExtend::Zero,
            u64::from(LOAD_VALUE as u32),
        ),
        (
            X86Reg::Rbp,
            5,
            MemWidth::B1,
            SignExtend::Zero,
            (0xA500_0000_0000_0005u64 & !0xFF) | (LOAD_VALUE & 0xFF),
        ),
        (
            X86Reg::Rsp,
            4,
            MemWidth::B2,
            SignExtend::Zero,
            (0xA500_0000_0000_0004u64 & !0xFFFF) | (LOAD_VALUE & 0xFFFF),
        ),
        (
            X86Reg::Rbp,
            5,
            MemWidth::B4,
            SignExtend::Sign,
            (LOAD_VALUE as u32 as i32 as i64) as u64,
        ),
        (
            X86Reg::Rsp,
            4,
            MemWidth::B2,
            SignExtend::Sign,
            (LOAD_VALUE as u16 as i16 as i64) as u64,
        ),
        (
            X86Reg::Rbp,
            5,
            MemWidth::B1,
            SignExtend::Sign,
            (LOAD_VALUE as u8 as i8 as i64) as u64,
        ),
    ] {
        let (code, entry) = lower_mem_load(dst, width, sign);
        let exec = ExecMem::new(&code).expect("map helper-backed load");

        let mut context = MemoryContext {
            load_value: LOAD_VALUE,
            load_ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial;
        regs.rflags = FLAGS;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.load_fn = load as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 1, "{dst:?} {width:?}");
        assert_eq!(context.last_addr, 0x2008, "{dst:?} {width:?}");
        assert_eq!(context.last_size, width.bytes(), "{dst:?} {width:?}");
        assert_eq!(
            context.last_signed,
            u32::from(sign == SignExtend::Sign),
            "{dst:?} {width:?}"
        );
        let mut want = initial;
        want[slot] = expected;
        assert_eq!(regs.gpr, want, "{dst:?} {width:?} GPR file");
        assert_eq!(regs.rflags & STATUS_MASK, FLAGS & STATUS_MASK);
        assert_eq!(regs.exit_pc, SENTINEL_PC);
    }

    // A faulting load must leave the architectural stack registers untouched
    // and hand control back at the faulting guest PC.
    for dst in [X86Reg::Rsp, X86Reg::Rbp] {
        let (code, entry) = lower_mem_load(dst, MemWidth::B8, SignExtend::Zero);
        let exec = ExecMem::new(&code).expect("map faulting helper-backed load");
        let mut context = MemoryContext {
            load_value: LOAD_VALUE,
            load_ok: 0,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial;
        regs.rflags = FLAGS;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.load_fn = load as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(regs.gpr, initial, "{dst:?} faulting load must not commit");
        assert_eq!(regs.rflags & STATUS_MASK, FLAGS & STATUS_MASK);
        assert_eq!(regs.exit_pc, PC);
    }
}
