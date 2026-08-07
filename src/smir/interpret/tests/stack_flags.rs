//! Canonical PUSHF/POPF interpretation, privilege, and fault precision.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x1000;
const CR0_PE: u64 = 1;
const CR0_AM: u64 = 1 << 18;
const CR4_VME: u64 = 1;

fn function(bytes: &[u8]) -> SmirFunction {
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in result.ops {
        builder.push_op(PC, op.kind);
    }
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    builder.finish()
}

fn configure(context: &mut SmirContext, rsp: u64, rflags: u64, cr0: u64, cr4: u64, cpl: u8) {
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gpr[4] = rsp;
    x86.rflags = rflags;
    x86.cr0 = cr0;
    x86.cr4 = cr4;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.cpl = cpl;
    context.flags.materialized = MaterializedFlags::from_rflags(rflags);
    context.flags.lazy = None;
}

fn execute(
    bytes: &[u8],
    level: OptLevel,
    context: &mut SmirContext,
    memory: &mut dyn SmirMemory,
) -> BlockResult {
    let mut function = function(bytes);
    optimize_function(&mut function, level);
    assert!(
        function.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86StackFlags(..)))
    );
    SmirInterpreter::new().execute_block(context, memory, &function.blocks[0])
}

fn x86(context: &SmirContext) -> &crate::smir::ir::context::X86RegState {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    x86
}

#[test]
fn push_images_and_rex_w_precedence_match_intel_masks_at_o0_o1_o2() {
    let initial = 0xF000_0000_0000_0000
        | 0x2
        | flags::bits::CF
        | flags::bits::PF
        | flags::bits::AF
        | flags::bits::ZF
        | flags::bits::SF
        | flags::bits::TF
        | flags::bits::IF
        | flags::bits::DF
        | flags::bits::OF
        | flags::bits::IOPL_MASK
        | flags::bits::NT
        | flags::bits::RF
        | flags::bits::AC
        | flags::bits::VIF
        | flags::bits::VIP
        | flags::bits::ID;

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (bytes, size) in [
            (&[0x9C][..], 8_usize),
            (&[0x66, 0x9C][..], 2),
            (&[0x66, 0x48, 0x9C][..], 8),
        ] {
            let mut context = SmirContext::new_x86_64();
            configure(&mut context, 0x2000, initial, CR0_PE, 0, 0);
            let mut memory = FlatMemory::new(0x3000);
            let result = execute(bytes, level, &mut context, &mut memory);
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            assert_eq!(x86(&context).gpr[4], 0x2000 - size as u64);
            let mut image = [0_u8; 8];
            memory
                .read(0x2000 - size as u64, &mut image[..size])
                .unwrap();
            let observed = u64::from_le_bytes(image);
            let expected = if size == 2 {
                initial & 0xFFFF
            } else {
                initial & 0x00FC_FFFF
            };
            assert_eq!(observed, expected, "{bytes:02X?} {level:?}");
            assert_eq!(x86(&context).rflags, initial, "PUSHF state");
        }
    }
}

#[test]
fn pushed_image_can_be_modified_and_popped_as_one_complete_transaction() {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x200, 0xCD7, CR0_PE, 0, 0);
    let mut memory = FlatMemory::new(0x400);

    let result = execute(&[0x9C], OptLevel::O2, &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(x86(&context).gpr[4], 0x1F8);
    let mut pushed = [0_u8; 8];
    memory.read(0x1F8, &mut pushed).unwrap();
    assert_eq!(u64::from_le_bytes(pushed), 0xCD7);

    memory.write(0x1F8, &0x8D7_u64.to_le_bytes()).unwrap();
    let result = execute(&[0x9D], OptLevel::O2, &mut context, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(x86(&context).gpr[4], 0x200);
    context.flags.materialize_all();
    assert_eq!(context.flags.materialized.to_rflags(), 0x8D7);
}

#[test]
fn pop_privilege_filter_updates_only_permitted_fields() {
    let popped = flags::bits::CF
        | flags::bits::PF
        | flags::bits::AF
        | flags::bits::ZF
        | flags::bits::SF
        | flags::bits::TF
        | flags::bits::IF
        | flags::bits::DF
        | flags::bits::OF
        | flags::bits::IOPL_MASK
        | flags::bits::NT
        | flags::bits::AC
        | flags::bits::ID;

    for (name, cpl, old_iopl, expected_if, expected_iopl) in [
        ("cpl0", 0, 0, flags::bits::IF, flags::bits::IOPL_MASK),
        ("cpl3-iopl0", 3, 0, 0, 0),
        (
            "cpl3-iopl3",
            3,
            flags::bits::IOPL_MASK,
            flags::bits::IF,
            flags::bits::IOPL_MASK,
        ),
    ] {
        let initial = 0x2 | flags::bits::RF | flags::bits::VIF | flags::bits::VIP | old_iopl;
        let mut context = SmirContext::new_x86_64();
        configure(&mut context, 0x1800, initial, CR0_PE, 0, cpl);
        let mut memory = FlatMemory::new(0x3000);
        memory.write(0x1800, &popped.to_le_bytes()).unwrap();
        let result = execute(&[0x9D], OptLevel::O2, &mut context, &mut memory);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        let actual = x86(&context).rflags;
        assert_eq!(x86(&context).gpr[4], 0x1808, "{name}");
        assert_eq!(actual & flags::bits::IF, expected_if, "{name}");
        assert_eq!(actual & flags::bits::IOPL_MASK, expected_iopl, "{name}");
        assert_eq!(actual & flags::bits::RF, 0, "{name}");
        assert_eq!(
            actual & (flags::bits::VIF | flags::bits::VIP),
            initial & (flags::bits::VIF | flags::bits::VIP),
            "{name}"
        );
        assert_eq!(
            actual
                & (flags::bits::CF
                    | flags::bits::PF
                    | flags::bits::AF
                    | flags::bits::ZF
                    | flags::bits::SF
                    | flags::bits::TF
                    | flags::bits::DF
                    | flags::bits::OF
                    | flags::bits::NT
                    | flags::bits::AC
                    | flags::bits::ID),
            popped
                & (flags::bits::CF
                    | flags::bits::PF
                    | flags::bits::AF
                    | flags::bits::ZF
                    | flags::bits::SF
                    | flags::bits::TF
                    | flags::bits::DF
                    | flags::bits::OF
                    | flags::bits::NT
                    | flags::bits::AC
                    | flags::bits::ID),
            "{name}"
        );
    }
}

#[test]
fn vme_pop_post_read_gp_and_push_virtualization_are_precise() {
    let initial = 0x2
        | flags::bits::VM
        | flags::bits::VIF
        | flags::bits::VIP
        | flags::bits::CF
        | flags::bits::DF;

    let mut memory = FlatMemory::new(0x3000);
    memory
        .write(0x1800, &(flags::bits::IF as u16).to_le_bytes())
        .unwrap();
    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x1800, initial, CR0_PE, CR4_VME, 3);
    let result = execute(&[0x66, 0x9D], OptLevel::O2, &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: PC,
            error_code: 0
        })
    ));
    assert_eq!(x86(&context).gpr[4], 0x1800);
    assert_eq!(x86(&context).rflags, initial);
    context.flags.materialize_all();
    assert_eq!(context.flags.materialized.to_rflags(), initial & 0x4_0CD7);

    let mut push_context = SmirContext::new_x86_64();
    configure(&mut push_context, 0x1800, initial, CR0_PE, CR4_VME, 3);
    let mut push_memory = FlatMemory::new(0x3000);
    let result = execute(
        &[0x66, 0x9C],
        OptLevel::O2,
        &mut push_context,
        &mut push_memory,
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let mut image = [0_u8; 2];
    push_memory.read(0x17FE, &mut image).unwrap();
    let image = u16::from_le_bytes(image) as u64;
    assert_ne!(image & flags::bits::IF, 0, "VIF substitutes for IF");
    assert_eq!(image & flags::bits::IOPL_MASK, flags::bits::IOPL_MASK);
}

#[test]
fn stack_address_and_memory_faults_leave_rsp_and_flags_uncommitted() {
    let initial = 0x2 | flags::bits::CF | flags::bits::DF;
    for (name, bytes, rsp, cr0, cpl, expected) in [
        (
            "noncanonical push",
            &[0x9C][..],
            0x0000_8000_0000_0008,
            CR0_PE,
            0,
            "ss",
        ),
        (
            "unaligned pop",
            &[0x9D][..],
            0x1801,
            CR0_PE | CR0_AM,
            3,
            "ac",
        ),
        ("unmapped pop", &[0x9D][..], 0x3000, CR0_PE, 0, "memory"),
    ] {
        let rflags = initial | if expected == "ac" { flags::bits::AC } else { 0 };
        let mut context = SmirContext::new_x86_64();
        configure(&mut context, rsp, rflags, cr0, 0, cpl);
        let mut memory = FlatMemory::new(0x2000);
        let result = execute(bytes, OptLevel::O2, &mut context, &mut memory);
        match expected {
            "ss" => assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::StackSegment { addr: PC, .. })
            )),
            "ac" => assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::AlignmentCheck { addr: PC })
            )),
            "memory" => assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            )),
            _ => unreachable!(),
        }
        assert_eq!(x86(&context).gpr[4], rsp, "{name}");
        assert_eq!(x86(&context).rflags, rflags, "{name}");
    }

    let mut context = SmirContext::new_x86_64();
    configure(&mut context, 0x80, initial, CR0_PE, 0, 0);
    let mut memory = StoreFaultMemory {
        inner: FlatMemory::new(0x100),
        stores_before_fault: 0,
    };
    let result = execute(&[0x9C], OptLevel::O2, &mut context, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    assert_eq!(x86(&context).gpr[4], 0x80);
    assert_eq!(x86(&context).rflags, initial);
}
