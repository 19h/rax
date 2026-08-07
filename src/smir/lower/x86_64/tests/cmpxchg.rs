//! Native lowering for register- and memory-destination `CMPXCHG`.

use super::*;
use crate::smir::ir::ops::{X86CmpxchgOp, X86GprOperand, X86OpHint};
use crate::smir::ir::types::Condition;
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::x86_cmpxchg_shape_valid;

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

fn register_cmpxchg(
    dst: X86GprOperand,
    src: X86GprOperand,
    width: OpWidth,
    flags: FlagUpdate,
) -> OpKind {
    OpKind::X86Cmpxchg(X86CmpxchgOp {
        dst,
        src,
        width,
        flags,
    })
}

#[test]
fn register_cmpxchg_emits_direct_high_byte_and_state_backed_sequences() {
    let rdx_rcx = lower_single_op(register_cmpxchg(
        X86GprOperand::low(X86Reg::Rdx),
        X86GprOperand::low(X86Reg::Rcx),
        OpWidth::W64,
        FlagUpdate::All,
    ));
    assert!(
        rdx_rcx
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x0F, 0xB1, 0xCA]),
        "CMPXCHG RDX,RCX: {rdx_rcx:02X?}"
    );

    let ch_dh = lower_single_op(register_cmpxchg(
        X86GprOperand::high(X86Reg::Rcx),
        X86GprOperand::high(X86Reg::Rdx),
        OpWidth::W8,
        FlagUpdate::All,
    ));
    assert!(
        ch_dh
            .windows(7)
            .any(|bytes| bytes == [0x3A, 0xC5, 0x9C, 0x0F, 0xB0, 0xF5, 0x9D]),
        "high-byte flag wrapper: {ch_dh:02X?}"
    );

    let spl_bpl = lower_single_op(register_cmpxchg(
        X86GprOperand::low(X86Reg::Rsp),
        X86GprOperand::low(X86Reg::Rbp),
        OpWidth::W8,
        FlagUpdate::All,
    ));
    assert!(
        spl_bpl
            .windows(4)
            .any(|bytes| bytes == [0x40, 0x0F, 0xB0, 0xFA]),
        "state-backed scratch CMPXCHG: {spl_bpl:02X?}"
    );
    assert!(
        spl_bpl.windows(2).any(|bytes| bytes == [0x0F, 0x85]),
        "state-backed mismatch branch: {spl_bpl:02X?}"
    );
    assert!(
        spl_bpl.windows(3).any(|bytes| bytes == [0x88, 0x51, 0x20]),
        "SPL match-path commit: {spl_bpl:02X?}"
    );

    let egpr = lower_single_op(register_cmpxchg(
        X86GprOperand::low(X86Reg::R16),
        X86GprOperand::low(X86Reg::R31),
        OpWidth::W32,
        FlagUpdate::None,
    ));
    assert!(
        egpr.windows(4)
            .any(|bytes| bytes == [0x0F, 0xB1, 0xFA, 0x0F]),
        "flagless scratch CMPXCHG followed by JNE: {egpr:02X?}"
    );
    assert!(
        egpr.iter().filter(|byte| **byte == 0x9C).count() >= 1
            && egpr.iter().filter(|byte| **byte == 0x9D).count() >= 1,
        "flagless state path must save and restore RFLAGS: {egpr:02X?}"
    );
}

#[test]
fn register_cmpxchg_lowering_rejects_unencodable_shapes_and_hints() {
    for malformed in [
        register_cmpxchg(
            X86GprOperand::low(X86Reg::Xmm(0)),
            X86GprOperand::low(X86Reg::Rax),
            OpWidth::W64,
            FlagUpdate::All,
        ),
        register_cmpxchg(
            X86GprOperand::high(X86Reg::Rsi),
            X86GprOperand::low(X86Reg::Rax),
            OpWidth::W8,
            FlagUpdate::All,
        ),
        register_cmpxchg(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::low(X86Reg::R8),
            OpWidth::W8,
            FlagUpdate::All,
        ),
        register_cmpxchg(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::high(X86Reg::Rbx),
            OpWidth::W16,
            FlagUpdate::All,
        ),
        register_cmpxchg(
            X86GprOperand::low(X86Reg::Rdx),
            X86GprOperand::low(X86Reg::Rbx),
            OpWidth::W64,
            FlagUpdate::Specific(FlagSet::ZF),
        ),
    ] {
        assert!(matches!(
            lower_single_op_err(malformed),
            LowerError::InvalidOperand { .. }
        ));
    }

    let exact = register_cmpxchg(
        X86GprOperand::low(X86Reg::Rdx),
        X86GprOperand::low(X86Reg::Rbx),
        OpWidth::W64,
        FlagUpdate::All,
    );
    assert!(matches!(
        lower_single_hinted_op_err(exact, X86OpHint::RexByteReg),
        LowerError::InvalidOperand { .. }
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_register_cmpxchg_matches_subtraction_oracle_for_aliases_and_state_slots() {
    use crate::isa::x86_64::flags;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const ARITHMETIC_FLAGS: u64 = 0x8D5;
    #[derive(Clone, Copy)]
    struct Case {
        name: &'static str,
        dst: X86GprOperand,
        src: X86GprOperand,
        width: OpWidth,
        matched: bool,
        update_flags: bool,
    }
    let low = X86GprOperand::low;
    let high = X86GprOperand::high;
    let cases = [
        Case {
            name: "CMPXCHG RDX,RCX direct match",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rcx),
            width: OpWidth::W64,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG RDX,RCX direct mismatch",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rcx),
            width: OpWidth::W64,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG DX,CX direct partial mismatch",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rcx),
            width: OpWidth::W16,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG R8D,R9D direct match and zero extension",
            dst: low(X86Reg::R8),
            src: low(X86Reg::R9),
            width: OpWidth::W32,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG DL,CL direct byte mismatch",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rcx),
            width: OpWidth::W8,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG RAX,RCX accumulator destination",
            dst: low(X86Reg::Rax),
            src: low(X86Reg::Rcx),
            width: OpWidth::W64,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG R8D,R8D explicit self alias",
            dst: low(X86Reg::R8),
            src: low(X86Reg::R8),
            width: OpWidth::W32,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG R8D,R8D self match and zero extension",
            dst: low(X86Reg::R8),
            src: low(X86Reg::R8),
            width: OpWidth::W32,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG RDX,RAX source accumulator match",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rax),
            width: OpWidth::W64,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG RDX,RAX source accumulator mismatch",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rax),
            width: OpWidth::W64,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG AH,BH high-byte match",
            dst: high(X86Reg::Rax),
            src: high(X86Reg::Rbx),
            width: OpWidth::W8,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG AH,BH high-byte mismatch",
            dst: high(X86Reg::Rax),
            src: high(X86Reg::Rbx),
            width: OpWidth::W8,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG AL,AH parent alias",
            dst: low(X86Reg::Rax),
            src: high(X86Reg::Rax),
            width: OpWidth::W8,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG AH,AL source-accumulator mismatch",
            dst: high(X86Reg::Rax),
            src: low(X86Reg::Rax),
            width: OpWidth::W8,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG AH,AH high-byte self match",
            dst: high(X86Reg::Rax),
            src: high(X86Reg::Rax),
            width: OpWidth::W8,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG AH,AH high-byte self mismatch",
            dst: high(X86Reg::Rax),
            src: high(X86Reg::Rax),
            width: OpWidth::W8,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG SPL,BPL state match",
            dst: low(X86Reg::Rsp),
            src: low(X86Reg::Rbp),
            width: OpWidth::W8,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG BP,R16W state match and saved-RBP synchronization",
            dst: low(X86Reg::Rbp),
            src: low(X86Reg::R16),
            width: OpWidth::W16,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG R16D,R31D EGPR match",
            dst: low(X86Reg::R16),
            src: low(X86Reg::R31),
            width: OpWidth::W32,
            matched: true,
            update_flags: true,
        },
        Case {
            name: "CMPXCHG RSP,RSP state self mismatch",
            dst: low(X86Reg::Rsp),
            src: low(X86Reg::Rsp),
            width: OpWidth::W64,
            matched: false,
            update_flags: true,
        },
        Case {
            name: "flagless CMPXCHG R16,RAX mismatch",
            dst: low(X86Reg::R16),
            src: low(X86Reg::Rax),
            width: OpWidth::W64,
            matched: false,
            update_flags: false,
        },
        Case {
            name: "flagless CMPXCHG RDX,RCX direct match",
            dst: low(X86Reg::Rdx),
            src: low(X86Reg::Rcx),
            width: OpWidth::W64,
            matched: true,
            update_flags: false,
        },
    ];

    let read = |gpr: &[u64; 32], operand: X86GprOperand, width: OpWidth| {
        let value = gpr[operand.gpr_index().unwrap() as usize];
        if operand.high_byte {
            (value >> 8) & 0xFF
        } else {
            value & width.mask()
        }
    };
    let write = |gpr: &mut [u64; 32], operand: X86GprOperand, width: OpWidth, value: u64| {
        let slot = &mut gpr[operand.gpr_index().unwrap() as usize];
        if operand.high_byte {
            *slot = (*slot & !0xFF00) | ((value & 0xFF) << 8);
        } else {
            *slot = match width {
                OpWidth::W8 => (*slot & !0xFF) | (value & 0xFF),
                OpWidth::W16 => (*slot & !0xFFFF) | (value & 0xFFFF),
                OpWidth::W32 => value & 0xFFFF_FFFF,
                OpWidth::W64 => value,
                _ => unreachable!(),
            };
        }
    };
    let accumulator = low(X86Reg::Rax);

    for case in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), PC);
        builder.push_op(
            PC,
            register_cmpxchg(
                case.dst,
                case.src,
                case.width,
                if case.update_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                },
            ),
        );
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        let function = builder.finish();
        assert!(x86_cmpxchg_shape_valid(&function.blocks[0].ops[0]));
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&function)
            .unwrap_or_else(|error| panic!("{} lower: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x8123_4567_89AB_00F1u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0137));
        }
        let old_dst = read(&regs.gpr, case.dst, case.width);
        write(
            &mut regs.gpr,
            accumulator,
            case.width,
            if case.matched { old_dst } else { old_dst ^ 1 },
        );
        assert_eq!(
            read(&regs.gpr, accumulator, case.width) == read(&regs.gpr, case.dst, case.width),
            case.matched,
            "{} path setup",
            case.name
        );
        regs.rflags = ARITHMETIC_FLAGS;

        let mut expected = regs;
        let old_accumulator = read(&expected.gpr, accumulator, case.width);
        let old_dst = read(&expected.gpr, case.dst, case.width);
        let old_src = read(&expected.gpr, case.src, case.width);
        let difference = old_accumulator.wrapping_sub(old_dst) & case.width.mask();
        if old_accumulator == old_dst {
            write(&mut expected.gpr, case.dst, case.width, old_src);
        } else {
            write(&mut expected.gpr, accumulator, case.width, old_dst);
        }
        if case.update_flags {
            flags::update_flags_sub(
                &mut expected.rflags,
                old_accumulator,
                old_dst,
                difference,
                (case.width.bits() / 8) as u8,
            );
        }

        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(
            regs.rflags & ARITHMETIC_FLAGS,
            expected.rflags & ARITHMETIC_FLAGS,
            "{} arithmetic flags",
            case.name
        );
    }
}
