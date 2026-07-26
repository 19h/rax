//! Fused native lowering for LOCK-prefixed x86 memory read-modify-write.

use super::*;
use crate::smir::ir::types::{AtomicOp, MemoryOrder};
use crate::smir::lower::SmirLowerer;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn addr() -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::Rdi),
        offset: 8,
        disp_size: DispSize::Disp8,
    }
}

fn lower(ops: Vec<OpKind>) -> (Vec<u8>, usize) {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in ops {
        builder.push_op(PC, op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let result = lowerer
        .lower_function(&builder.finish())
        .expect("lower fused locked RMW");
    (lowerer.finalize().expect("finalize"), result.entry_offset)
}

fn count(bytes: &[u8], needle: &[u8]) -> usize {
    bytes.windows(needle.len()).filter(|w| *w == needle).count()
}

#[test]
fn locked_or_fuses_into_the_helper_backed_frame() {
    // `lock or byte [rdi+8],1` with dead flags: one compute, no replay.
    let (flag_dead, _) = lower(vec![
        OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Imm(1),
            width: OpWidth::W8,
        },
        OpKind::AtomicRmw {
            dst: virt(1),
            addr: addr(),
            src: virt(0),
            op: AtomicOp::Or,
            width: MemWidth::B1,
            order: MemoryOrder::SeqCst,
        },
    ]);
    assert!(
        flag_dead
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0xE0]),
        "must reserve the 32-byte caller frame: {flag_dead:02X?}"
    );
    // `or al, 1` is 80 /1 ib against the scratch accumulator.
    assert_eq!(
        count(&flag_dead, &[0x80, 0xC8, 0x01]),
        1,
        "flag-dead form computes exactly once: {flag_dead:02X?}"
    );

    // With the flags live, the same operation is replayed after the store.
    let (flag_live, _) = lower(vec![
        OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Imm(1),
            width: OpWidth::W8,
        },
        OpKind::AtomicRmw {
            dst: virt(1),
            addr: addr(),
            src: virt(0),
            op: AtomicOp::Or,
            width: MemWidth::B1,
            order: MemoryOrder::SeqCst,
        },
        OpKind::Or {
            dst: virt(2),
            src1: virt(1),
            src2: SrcOperand::Reg(virt(0)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
    ]);
    assert_eq!(
        count(&flag_live, &[0x80, 0xC8, 0x01]),
        2,
        "flag-publishing form computes and replays: {flag_live:02X?}"
    );
}

#[test]
fn locked_xadd_writes_the_pre_operation_value_back_after_the_store() {
    let (bytes, _) = lower(vec![
        OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Imm(1),
            width: OpWidth::W32,
        },
        OpKind::AtomicRmw {
            dst: virt(1),
            addr: addr(),
            src: virt(0),
            op: AtomicOp::Add,
            width: MemWidth::B4,
            order: MemoryOrder::SeqCst,
        },
        OpKind::Mov {
            dst: x86(X86Reg::Rcx),
            src: SrcOperand::Reg(virt(1)),
            width: OpWidth::W32,
        },
    ]);
    // `add eax, 1` computes the stored value.
    assert!(
        bytes.windows(3).any(|b| b == [0x83, 0xC0, 0x01]),
        "must add the materialized immediate: {bytes:02X?}"
    );
    // `mov ecx, [rsp]` delivers the pre-operation memory value, zero-extended.
    assert!(
        bytes.windows(4).any(|b| b == [0x8B, 0x0C, 0x24, 0x48]),
        "must write the loaded value back into ECX before releasing the frame: {bytes:02X?}"
    );
    // The write-back is flag neutral: no PUSHFQ/POPFQ pair is added for it.
    assert_eq!(
        count(&bytes, &[0x83, 0xC0, 0x01]),
        1,
        "a flag-dead XADD must not replay: {bytes:02X?}"
    );
}

#[test]
fn locked_dec_replays_the_unary_flag_contract() {
    let (bytes, _) = lower(vec![
        OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Imm(1),
            width: OpWidth::W32,
        },
        OpKind::AtomicRmw {
            dst: virt(1),
            addr: addr(),
            src: virt(0),
            op: AtomicOp::Sub,
            width: MemWidth::B4,
            order: MemoryOrder::SeqCst,
        },
        OpKind::Dec {
            dst: virt(2),
            src: virt(1),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
    ]);
    // The memory value is produced with `sub eax, 1` (83 /5 ib) ...
    assert!(
        bytes.windows(3).any(|b| b == [0x83, 0xE8, 0x01]),
        "must subtract one from the loaded value: {bytes:02X?}"
    );
    // ... while the published flags come from `dec eax` (FF /1), which leaves
    // CF unchanged exactly as the architecture requires.
    assert!(
        bytes.windows(2).any(|b| b == [0xFF, 0xC8]),
        "must replay the unary DEC flag contract: {bytes:02X?}"
    );
    assert_eq!(
        count(&bytes, &[0x83, 0xE8, 0x01]),
        1,
        "the Group-1 form must not also replay: {bytes:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    loads: u64,
    stores: u64,
    last_addr: u64,
    value: u64,
    load_ok: u64,
    store_ok: u64,
    stored: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load(context: *mut MemoryContext, addr: u64, _size: u64) -> (u64, u64) {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    (context.value, context.load_ok)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store(context: *mut MemoryContext, addr: u64, value: u64, _size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.last_addr = addr;
    context.stored = value;
    context.store_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_locked_rmw_updates_memory_publishes_flags_and_writes_back() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const FLAGS: u64 = 0x2;
    const ZF: u64 = 1 << 6;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    let mut initial = [0u64; 32];
    initial[7] = 0x3000; // RDI base

    // `lock add dword [rdi+8], 1` with a live flag result and an XADD-style
    // write-back into ECX.
    let (code, entry) = lower(vec![
        OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Imm(1),
            width: OpWidth::W32,
        },
        OpKind::AtomicRmw {
            dst: virt(1),
            addr: addr(),
            src: virt(0),
            op: AtomicOp::Add,
            width: MemWidth::B4,
            order: MemoryOrder::SeqCst,
        },
        OpKind::Add {
            dst: virt(2),
            src1: virt(1),
            src2: SrcOperand::Reg(virt(0)),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
        OpKind::Mov {
            dst: x86(X86Reg::Rcx),
            src: SrcOperand::Reg(virt(1)),
            width: OpWidth::W32,
        },
    ]);
    let exec = ExecMem::new(&code).expect("map fused locked RMW");

    for (value, expect_zf) in [(0x1000_0000u64, false), (0xFFFF_FFFFu64, true)] {
        let mut context = MemoryContext {
            value,
            load_ok: 1,
            store_ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial;
        regs.rflags = FLAGS;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.load_fn = load as usize as u64;
        regs.store_fn = store as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 1);
        assert_eq!(context.stores, 1);
        assert_eq!(context.last_addr, 0x3008);
        assert_eq!(context.stored, u64::from((value as u32).wrapping_add(1)));
        assert_eq!(regs.gpr[1], value, "pre-operation value written back");
        assert_eq!(regs.rflags & ZF != 0, expect_zf, "architectural ZF");
        assert_eq!(regs.exit_pc, SENTINEL_PC);
    }

    // A faulting store must leave every architectural register untouched and
    // resume at the locked instruction.
    let mut context = MemoryContext {
        value: 7,
        load_ok: 1,
        store_ok: 0,
        ..MemoryContext::default()
    };
    let mut regs = GuestRegs::default();
    regs.gpr = initial;
    regs.rflags = FLAGS;
    regs.exit_pc = SENTINEL_PC;
    regs.ctx = (&mut context as *mut MemoryContext) as u64;
    regs.load_fn = load as usize as u64;
    regs.store_fn = store as usize as u64;
    exec.run(entry, &mut regs);
    assert_eq!(regs.gpr, initial, "faulting store must not commit");
    assert_eq!(regs.exit_pc, PC);
}
