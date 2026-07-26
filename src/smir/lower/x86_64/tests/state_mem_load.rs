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

fn load_op(dst: X86Reg, width: MemWidth) -> OpKind {
    OpKind::Load {
        dst: x86(dst),
        addr: Address::BaseOffset {
            base: x86(X86Reg::Rbx),
            offset: 8,
            disp_size: DispSize::Disp8,
        },
        width,
        sign: SignExtend::Zero,
    }
}

fn lower_mem_load(dst: X86Reg, width: MemWidth) -> (Vec<u8>, usize) {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, load_op(dst, width));
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
    let (rsp, _) = lower_mem_load(X86Reg::Rsp, MemWidth::B8);
    assert!(
        rsp.windows(7)
            .any(|b| b == [0x48, 0x89, 0x81, 0x20, 0x00, 0x00, 0x00]),
        "must commit the guest RSP slot: {rsp:02X?}"
    );
    assert!(
        !rsp.windows(4).any(|b| b == [0x48, 0x89, 0x45, 0x00]),
        "an RSP destination must not touch the saved guest RBP word: {rsp:02X?}"
    );

    let (rbp, _) = lower_mem_load(X86Reg::Rbp, MemWidth::B8);
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
    let (byte, _) = lower_mem_load(X86Reg::Rsp, MemWidth::B1);
    assert!(
        byte.windows(6)
            .any(|b| b == [0x88, 0x81, 0x20, 0x00, 0x00, 0x00]),
        "byte load must write only SPL in the slot: {byte:02X?}"
    );
    let (word, _) = lower_mem_load(X86Reg::Rbp, MemWidth::B2);
    assert!(
        word.windows(7)
            .any(|b| b == [0x66, 0x89, 0x81, 0x28, 0x00, 0x00, 0x00]),
        "word load must write only BP in the slot: {word:02X?}"
    );
    assert!(
        word.windows(4).any(|b| b == [0x48, 0x89, 0x45, 0x00]),
        "a partial RBP load must still synchronize the saved word: {word:02X?}"
    );
}

#[test]
fn stack_destination_loads_require_the_memory_helper_path() {
    // Without MMU helpers the load would target the host register of the same
    // name, so it must still be rejected.
    for dst in [X86Reg::Rsp, X86Reg::Rbp] {
        let mut builder = FunctionBuilder::new(FunctionId(0), PC);
        builder.push_op(PC, load_op(dst, MemWidth::B8));
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
    load_value: u64,
    load_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load(context: *mut MemoryContext, addr: u64, _size: u64) -> (u64, u64) {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    (context.load_value, context.load_ok)
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

    for (dst, slot, width, expected) in [
        (X86Reg::Rsp, 4usize, MemWidth::B8, LOAD_VALUE),
        (X86Reg::Rbp, 5, MemWidth::B8, LOAD_VALUE),
        (X86Reg::Rsp, 4, MemWidth::B4, u64::from(LOAD_VALUE as u32)),
        (
            X86Reg::Rbp,
            5,
            MemWidth::B1,
            (0xA500_0000_0000_0005u64 & !0xFF) | (LOAD_VALUE & 0xFF),
        ),
    ] {
        let (code, entry) = lower_mem_load(dst, width);
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
        let mut want = initial;
        want[slot] = expected;
        assert_eq!(regs.gpr, want, "{dst:?} {width:?} GPR file");
        assert_eq!(regs.rflags & STATUS_MASK, FLAGS & STATUS_MASK);
        assert_eq!(regs.exit_pc, SENTINEL_PC);
    }

    // A faulting load must leave the architectural stack registers untouched
    // and hand control back at the faulting guest PC.
    for dst in [X86Reg::Rsp, X86Reg::Rbp] {
        let (code, entry) = lower_mem_load(dst, MemWidth::B8);
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
