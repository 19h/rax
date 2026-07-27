//! Fused native lowering for memory-destination `CMPXCHG`.

use super::*;
use crate::smir::ir::types::Condition;
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
        base: x86(X86Reg::Rbx),
        offset: 16,
        disp_size: DispSize::Disp8,
    }
}

/// The full lifted shape: two snapshot MOVs, the load, the compare, the
/// predicated store and the accumulator write-back.
fn cmpxchg(mem_width: MemWidth, width: OpWidth, source: X86Reg) -> Vec<OpKind> {
    vec![
        OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Reg(x86(source)),
            width,
        },
        OpKind::Mov {
            dst: virt(1),
            src: SrcOperand::Reg(x86(X86Reg::Rax)),
            width,
        },
        OpKind::Load {
            dst: virt(2),
            addr: addr(),
            width: mem_width,
            sign: SignExtend::Zero,
        },
        OpKind::Cmp {
            src1: virt(1),
            src2: SrcOperand::Reg(virt(2)),
            width,
        },
        OpKind::SetCC {
            dst: virt(3),
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
        OpKind::Select {
            dst: virt(4),
            cond: virt(3),
            src_true: virt(0),
            src_false: virt(2),
            width,
        },
        OpKind::PredStore {
            src: SrcOperand::Reg(virt(4)),
            cond: virt(3),
            addr: addr(),
            width: mem_width,
        },
        OpKind::CMove {
            dst: x86(X86Reg::Rax),
            src: virt(2),
            cond: Condition::Ne,
            width,
        },
    ]
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
        .expect("lower fused CMPXCHG");
    (lowerer.finalize().expect("finalize"), result.entry_offset)
}

#[test]
fn cmpxchg_publishes_one_compare_and_branches_around_the_store() {
    let (bytes, _) = lower(cmpxchg(MemWidth::B8, OpWidth::W64, X86Reg::Rcx));
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0xE0]),
        "must reserve the flag-neutral caller frame: {bytes:02X?}"
    );
    // `cmp rax, [rsp]` is the single architectural comparison.
    assert_eq!(
        bytes
            .windows(4)
            .filter(|b| *b == [0x48, 0x3B, 0x04, 0x24])
            .count(),
        1,
        "must publish exactly one architectural comparison: {bytes:02X?}"
    );
    // The predicated store is reached through a mismatch branch.
    assert!(
        bytes.windows(2).any(|b| b == [0x0F, 0x85]),
        "must branch around the store on a mismatch: {bytes:02X?}"
    );
    // The accumulator write-back is a plain MOV on the mismatch path, never a
    // host CMOVcc (which would zero-extend unconditionally at 32 bits).
    assert!(
        !bytes.windows(2).any(|b| b == [0x0F, 0x45]),
        "must not use host CMOVNE for the accumulator: {bytes:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    loads: u64,
    stores: u64,
    load_addr: u64,
    store_addr: u64,
    stored: u64,
    stored_size: u64,
    value: u64,
    load_ok: u64,
    store_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load_helper(context: *mut MemoryContext, addr: u64, size: u64) -> (u64, u64) {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.load_addr = addr;
    let mask = if size >= 8 {
        u64::MAX
    } else {
        (1u64 << (size * 8)) - 1
    };
    (context.value & mask, context.load_ok)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store_helper(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.store_addr = addr;
    let mask = if size >= 8 {
        u64::MAX
    } else {
        (1u64 << (size * 8)) - 1
    };
    context.stored = value & mask;
    context.stored_size = size;
    context.store_ok
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_cmpxchg_matches_the_architectural_match_and_mismatch_paths() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const ZF: u64 = 1 << 6;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    let mut initial = [0u64; 32];
    initial[0] = 0xAAAA_BBBB_1234_5678; // RAX accumulator
    initial[1] = 0x1111_2222_3333_4444; // RCX replacement
    initial[3] = 0x7000; // RBX base

    let run = |ops: Vec<OpKind>, memory: u64, load_ok: u64, store_ok: u64| {
        let (code, entry) = lower(ops);
        let exec = ExecMem::new(&code).expect("map fused CMPXCHG");
        let mut context = MemoryContext {
            value: memory,
            load_ok,
            store_ok,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial;
        regs.rflags = 0x2;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.load_fn = load_helper as usize as u64;
        regs.store_fn = store_helper as usize as u64;
        exec.run(entry, &mut regs);
        (context, regs)
    };

    for (mem_width, width, bytes) in [
        (MemWidth::B8, OpWidth::W64, 8u64),
        (MemWidth::B4, OpWidth::W32, 4),
        (MemWidth::B2, OpWidth::W16, 2),
        (MemWidth::B1, OpWidth::W8, 1),
    ] {
        let mask = if bytes >= 8 {
            u64::MAX
        } else {
            (1u64 << (bytes * 8)) - 1
        };
        let ops = || cmpxchg(mem_width, width, X86Reg::Rcx);

        // Match: the replacement is written, RAX and ZF report success.
        let (context, regs) = run(ops(), initial[0] & mask, 1, 1);
        assert_eq!(context.loads, 1);
        assert_eq!(context.load_addr, 0x7010);
        assert_eq!(context.stores, 1, "{mem_width:?} match must store");
        assert_eq!(context.store_addr, 0x7010);
        assert_eq!(
            context.stored,
            initial[1] & mask,
            "{mem_width:?} replacement"
        );
        assert_eq!(context.stored_size, bytes);
        assert_eq!(regs.gpr, initial, "{mem_width:?} match preserves every GPR");
        assert!(regs.rflags & ZF != 0, "{mem_width:?} match sets ZF");

        // Mismatch: no store, and the accumulator takes the memory operand with
        // ordinary partial-register semantics.
        let other = (initial[0] ^ 1) & mask;
        let (context, regs) = run(ops(), other, 1, 1);
        assert_eq!(context.stores, 0, "{mem_width:?} mismatch must not store");
        let mut expected = initial;
        expected[0] = match width {
            OpWidth::W8 | OpWidth::W16 => (initial[0] & !mask) | other,
            _ => other,
        };
        assert_eq!(regs.gpr, expected, "{mem_width:?} mismatch accumulator");
        assert!(regs.rflags & ZF == 0, "{mem_width:?} mismatch clears ZF");
        assert_eq!(regs.exit_pc, SENTINEL_PC);
    }

    // A faulting load commits nothing and resumes at the guest PC.
    let (context, regs) = run(
        cmpxchg(MemWidth::B8, OpWidth::W64, X86Reg::Rcx),
        initial[0],
        0,
        1,
    );
    assert_eq!(context.stores, 0);
    assert_eq!(regs.gpr, initial);
    assert_eq!(regs.exit_pc, PC);

    // A faulting store on the matching path leaves every register unchanged and
    // resumes at the guest PC; the comparison's flags are already architectural.
    let (context, regs) = run(
        cmpxchg(MemWidth::B8, OpWidth::W64, X86Reg::Rcx),
        initial[0],
        1,
        0,
    );
    assert_eq!(context.stores, 1);
    assert_eq!(regs.gpr, initial);
    assert!(regs.rflags & ZF != 0);
    assert_eq!(regs.exit_pc, PC);
}
