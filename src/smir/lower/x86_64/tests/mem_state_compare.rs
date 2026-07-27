//! Fused memory-source compare against a state-backed GPR.

use super::*;
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

fn load(dst: VReg, width: MemWidth) -> OpKind {
    OpKind::Load {
        dst,
        addr: addr(),
        width,
        sign: SignExtend::Zero,
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
        .expect("lower fused memory/state compare");
    (lowerer.finalize().expect("finalize"), result.entry_offset)
}

#[test]
fn memory_state_compare_stages_the_helper_result_and_reloads_the_slot() {
    // `cmp dword [rbx+16], ebp`
    let (bytes, _) = lower(vec![
        load(virt(0), MemWidth::B4),
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W32,
        },
    ]);
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0xE0]),
        "must reserve the flag-neutral caller frame: {bytes:02X?}"
    );
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x89, 0x44, 0x24, 0x18]),
        "must save the architectural RAX: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8B, 0x40, 0x28]),
        "must reload guest RBP from its GuestRegs slot: {bytes:02X?}"
    );
    assert!(
        bytes.windows(3).any(|b| b == [0x39, 0x04, 0x24]),
        "must compare the staged memory operand against the slot value: {bytes:02X?}"
    );
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x8B, 0x44, 0x24, 0x18]),
        "must restore the architectural RAX: {bytes:02X?}"
    );
    assert!(
        !bytes.contains(&0x9C) || !bytes[bytes.len() - 16..].contains(&0x9D),
        "the compare itself must publish flags without a wrapper: {bytes:02X?}"
    );

    // `cmp rsp, qword [rbx+16]` reverses the architectural operand order.
    let (reversed, _) = lower(vec![
        load(virt(0), MemWidth::B8),
        OpKind::Cmp {
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Reg(virt(0)),
            width: OpWidth::W64,
        },
    ]);
    assert!(
        reversed.windows(4).any(|b| b == [0x48, 0x8B, 0x40, 0x20]),
        "must reload guest RSP from its GuestRegs slot: {reversed:02X?}"
    );
    assert!(
        reversed.windows(4).any(|b| b == [0x48, 0x3B, 0x04, 0x24]),
        "must compute register minus memory: {reversed:02X?}"
    );

    // `test byte [rbx+16], bpl` is commutative and uses the memory form.
    let (test_bytes, _) = lower(vec![
        load(virt(0), MemWidth::B1),
        OpKind::Test {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W8,
        },
    ]);
    assert!(
        test_bytes.windows(3).any(|b| b == [0x84, 0x04, 0x24]),
        "must test the staged memory operand against the slot value: {test_bytes:02X?}"
    );

    // An APX EGPR operand uses the same state-backed path.
    let (egpr, _) = lower(vec![
        load(virt(0), MemWidth::B4),
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W32,
        },
    ]);
    assert!(
        egpr.windows(7)
            .any(|b| b == [0x48, 0x8B, 0x80, 0x80, 0x00, 0x00, 0x00]),
        "must reload R16 from its GuestRegs slot: {egpr:02X?}"
    );
}

#[test]
fn identity_and_immediate_operands_keep_the_generic_memory_source_fusion() {
    // An identity-mapped operand is handled by the existing fusion, which uses
    // that register as the transfer register instead of a caller frame.
    let (identity, _) = lower(vec![
        load(virt(0), MemWidth::B4),
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W32,
        },
    ]);
    assert!(
        !identity
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0xE0]),
        "identity operands must not reserve the state-compare frame: {identity:02X?}"
    );

    let (immediate, _) = lower(vec![
        load(virt(0), MemWidth::B4),
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Imm(7),
            width: OpWidth::W32,
        },
    ]);
    assert!(
        !immediate.windows(4).any(|b| b == [0x48, 0x8B, 0x40, 0x28]),
        "an immediate operand needs no GuestRegs slot: {immediate:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    loads: u64,
    last_addr: u64,
    value: u64,
    load_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn load_helper(context: *mut MemoryContext, addr: u64, _size: u64) -> (u64, u64) {
    let context = unsafe { &mut *context };
    context.loads += 1;
    context.last_addr = addr;
    (context.value, context.load_ok)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_memory_state_compare_publishes_architectural_flags() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const CF: u64 = 1 << 0;
    const ZF: u64 = 1 << 6;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    let mut initial = [0u64; 32];
    initial[3] = 0x4000; // RBX base
    initial[5] = 0x1234_5678; // guest RBP

    // `cmp dword [rbx+16], ebp`
    let (code, entry) = lower(vec![
        load(virt(0), MemWidth::B4),
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W32,
        },
    ]);
    let exec = ExecMem::new(&code).expect("map fused memory/state compare");

    for value in [0x1234_5678u64, 0x1234_5677, 0x1234_5679] {
        let mut context = MemoryContext {
            value,
            load_ok: 1,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial;
        regs.rflags = 0x2;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.load_fn = load_helper as usize as u64;
        exec.run(entry, &mut regs);

        assert_eq!(context.loads, 1);
        assert_eq!(context.last_addr, 0x4010);
        assert_eq!(regs.gpr, initial, "no architectural register may change");
        let memory = value as u32;
        let register = initial[5] as u32;
        assert_eq!(
            regs.rflags & ZF != 0,
            memory == register,
            "ZF for {memory:#x} - {register:#x}"
        );
        assert_eq!(
            regs.rflags & CF != 0,
            memory < register,
            "CF for {memory:#x} - {register:#x}"
        );
        assert_eq!(regs.exit_pc, SENTINEL_PC);
    }

    // A faulting load exits at the guest PC without publishing a comparison.
    let mut context = MemoryContext {
        value: 0,
        load_ok: 0,
        ..MemoryContext::default()
    };
    let mut regs = GuestRegs::default();
    regs.gpr = initial;
    regs.rflags = 0x2;
    regs.exit_pc = SENTINEL_PC;
    regs.ctx = (&mut context as *mut MemoryContext) as u64;
    regs.load_fn = load_helper as usize as u64;
    exec.run(entry, &mut regs);
    assert_eq!(regs.gpr, initial);
    assert_eq!(regs.exit_pc, PC);
}
