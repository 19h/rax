//! Fused native lowering for `PUSHF`/`PUSHFQ`.

use super::*;
use crate::smir::lower::SmirLowerer;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn push_flags(delta: i64, push_width: MemWidth) -> Vec<OpKind> {
    vec![
        OpKind::ReadFlags { dst: virt(0) },
        OpKind::Sub {
            dst: x86(X86Reg::Rsp),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Imm(delta),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Store {
            src: virt(0),
            addr: Address::Direct(x86(X86Reg::Rsp)),
            width: push_width,
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
        .expect("lower fused flag push");
    (lowerer.finalize().expect("finalize"), result.entry_offset)
}

#[test]
fn flag_push_materializes_the_modeled_image_and_reuses_the_helper_backed_push() {
    let (bytes, _) = lower(push_flags(8, MemWidth::B8));
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0xE0]),
        "must reserve the flag-neutral staging frame: {bytes:02X?}"
    );
    // The architectural image is built by the shared ReadFlags helper, which
    // merges the state-backed guest AC bit.
    assert!(
        bytes.windows(2).any(|b| b == [0x0F, 0xBA]) || bytes.contains(&0x9C),
        "must snapshot the host flag image: {bytes:02X?}"
    );
    // `and rax, 40CD7h` drops every bit SMIR does not model, bracketed by
    // PUSHFQ/POPFQ so PUSHF itself publishes no flag change.
    assert!(
        bytes
            .windows(9)
            .any(|b| b == [0x9C, 0x48, 0x81, 0xE0, 0xD7, 0x0C, 0x04, 0x00, 0x9D]),
        "must mask the image to the modeled RFLAGS bits: {bytes:02X?}"
    );
    // The stack pointer is committed through its GuestRegs slot (+20h).
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x89, 0x50, 0x20]),
        "must commit the guest RSP slot: {bytes:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct MemoryContext {
    stores: u64,
    store_addr: u64,
    stored: u64,
    stored_size: u64,
    store_ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn store_helper(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
    let context = unsafe { &mut *context };
    context.stores += 1;
    context.store_addr = addr;
    // Mirror `rax_jit_mem_store`, which commits exactly `size` bytes.
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
fn native_flag_push_stores_exactly_the_modeled_rflags_image() {
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    const AC: u64 = 1 << 18;

    let mut initial = [0u64; 32];
    initial[4] = 0x9000; // guest RSP
    initial[0] = 0xDEAD_BEEF_CAFE_F00D; // guest RAX must survive

    let run = |ops: Vec<OpKind>, rflags: u64, ac: u64, store_ok: u64| {
        let (code, entry) = lower(ops);
        let exec = ExecMem::new(&code).expect("map fused flag push");
        let mut context = MemoryContext {
            store_ok,
            ..MemoryContext::default()
        };
        let mut regs = GuestRegs::default();
        regs.gpr = initial;
        regs.rflags = rflags;
        regs.ac_flag = ac;
        regs.exit_pc = SENTINEL_PC;
        regs.ctx = (&mut context as *mut MemoryContext) as u64;
        regs.store_fn = store_helper as usize as u64;
        exec.run(entry, &mut regs);
        (context, regs)
    };

    // Every combination of the modeled status/direction bits plus guest AC must
    // reproduce `MaterializedFlags::to_rflags` exactly.
    for image in [
        0x2u64,
        0x2 | 0x1,   // CF
        0x2 | 0x4,   // PF
        0x2 | 0x10,  // AF
        0x2 | 0x40,  // ZF
        0x2 | 0x80,  // SF
        0x2 | 0x400, // DF
        0x2 | 0x800, // OF
        0x2 | 0x8D5, // every status bit plus DF
    ] {
        for ac in [0u64, 1] {
            let (context, regs) = run(push_flags(8, MemWidth::B8), image, ac, 1);
            let expected =
                MaterializedFlags::from_rflags(image | if ac != 0 { AC } else { 0 }).to_rflags();
            assert_eq!(
                context.stored, expected,
                "pushed image for rflags {image:#x} ac {ac}"
            );
            assert_eq!(context.stored_size, 8);
            assert_eq!(context.store_addr, 0x9000 - 8);
            assert_eq!(regs.gpr[0], initial[0], "guest RAX must be preserved");
            assert_eq!(regs.gpr[4], 0x9000 - 8, "guest RSP must be committed");
            assert_eq!(
                regs.rflags & 0x8D5,
                image & 0x8D5,
                "PUSHF must not change the architectural flags"
            );
            assert_eq!(regs.ac_flag, ac, "PUSHF must not change guest AC");
            assert_eq!(regs.exit_pc, SENTINEL_PC);
        }
    }

    // A 16-bit push stores only the low word at the new stack top.
    let (context, regs) = run(push_flags(2, MemWidth::B2), 0x2 | 0x8D5, 1, 1);
    assert_eq!(context.stored, u64::from((0x2u64 | 0x8D5) as u16));
    assert_eq!(context.stored_size, 2);
    assert_eq!(regs.gpr[4], 0x9000 - 2);

    // A faulting stack write leaves RSP unchanged and resumes at the guest PC.
    let (context, regs) = run(push_flags(8, MemWidth::B8), 0x2, 0, 0);
    assert_eq!(context.stores, 1);
    assert_eq!(regs.gpr, initial, "guest RSP must not move");
    assert_eq!(regs.exit_pc, PC);
}
