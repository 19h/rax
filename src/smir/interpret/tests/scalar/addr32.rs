//! x86-64 address-size-override interpretation tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn x86_lea_interpretation_applies_destination_width() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);

    ctx.write_vreg(rax, 0xffff_ffff);
    ctx.write_vreg(rdx, u64::MAX);
    execute_lifted_x86(&[0x8d, 0x50, 0x01], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rdx), 0, "r32 LEA must zero-extend");

    ctx.write_vreg(rax, 0xffff);
    ctx.write_vreg(rdx, 0x1234_5678_9abc_ffff);
    execute_lifted_x86(&[0x66, 0x8d, 0x50, 0x01], &mut ctx, &mut memory);
    assert_eq!(
        ctx.read_vreg(rdx),
        0x1234_5678_9abc_0000,
        "r16 LEA must preserve bits 63:16"
    );
}

#[test]
fn addr32_memory_call_targets_wrap_before_segment_addition() {
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let r12 = VReg::Arch(ArchReg::X86(X86Reg::R12));
    let fs = VReg::Arch(ArchReg::X86(X86Reg::FsBase));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.write_vreg(r8, 0xDEAD_BEEF_FFFF_FFF0);
    ctx.write_vreg(r12, 0xCAFE_BABE_0000_0008);
    ctx.write_vreg(fs, 0x100);

    // (FFFF_FFF0h + 8*4 + 10h) mod 2^32 = 20h; FS is then added,
    // selecting the 8-byte target at linear address 120h.
    memory.write(0x120, &0x2345u64.to_le_bytes()).unwrap();
    let mut call = SmirBlock::new(BlockId(0), 0x1000);
    call.set_terminator(Terminator::Call {
        target: CallTarget::X86IndirectMemAddr32(Address::SegmentRel {
            segment: fs,
            base: Some(r8),
            index: Some(r12),
            scale: 4,
            disp: 0x10,
        }),
        args: Vec::new(),
        continuation: BlockId(1),
    });
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &call),
        BlockResult::Continue(0x2345)
    ));

    // Synthetic IR with high absolute bits still follows the variant's modulo
    // contract. TailCall shares the same canonical target-resolution path.
    memory.write(0x180, &0x3456u64.to_le_bytes()).unwrap();
    let mut tail = SmirBlock::new(BlockId(0), 0x1000);
    tail.set_terminator(Terminator::TailCall {
        target: CallTarget::X86IndirectMemAddr32(Address::Absolute(0x1_0000_0180)),
        args: Vec::new(),
    });
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &tail),
        BlockResult::Continue(0x3456)
    ));
}

#[test]
fn lifted_modrm_addr32_zero_extends_wraps_and_adds_segment_after_offset() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let r12 = VReg::Arch(ArchReg::X86(X86Reg::R12));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let fs = VReg::Arch(ArchReg::X86(X86Reg::FsBase));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    // The high half of RBX is discarded and FFFF_FFF8h + 28h wraps to
    // 0000_0020h before being zero-extended.
    let wrap_value = 0x0123_4567_89AB_CDEFu64;
    memory.write(0x20, &wrap_value.to_le_bytes()).unwrap();
    ctx.write_vreg(rbx, 0xDEAD_BEEF_FFFF_FFF8);
    execute_lifted_x86(&[0x67, 0x48, 0x8B, 0x43, 0x28], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), wrap_value);

    // SIB base=101 with no index remains absolute disp32 in addr32 mode.
    let absolute_value = 0xA5A5_5A5A_F00D_CAFEu64;
    memory.write(0x80, &absolute_value.to_le_bytes()).unwrap();
    execute_lifted_x86(
        &[0x67, 0x48, 0x8B, 0x04, 0x25, 0x80, 0x00, 0x00, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert_eq!(ctx.read_vreg(rax), absolute_value);

    // ModR/M mod=00,r/m=101 remains RIP-relative. The lifted instruction is
    // based at 1000h and is 8 bytes, so EIP + FFFFF080h wraps to 00000088h.
    let eip_relative_value = 0x1020_3040_5060_7080u64;
    memory
        .write(0x88, &eip_relative_value.to_le_bytes())
        .unwrap();
    execute_lifted_x86(
        &[0x67, 0x48, 0x8B, 0x05, 0x80, 0xF0, 0xFF, 0xFF],
        &mut ctx,
        &mut memory,
    );
    assert_eq!(ctx.read_vreg(rax), eip_relative_value);

    // REX.B/X select the low 32-bit halves of r8/r12. The address is
    // 10h + 3*4 + 8 = 24h despite nonzero upper halves.
    let extended_value = 0x8877_6655_4433_2211u64;
    memory.write(0x24, &extended_value.to_le_bytes()).unwrap();
    ctx.write_vreg(r8, 0x1111_2222_0000_0010);
    ctx.write_vreg(r12, 0x3333_4444_0000_0003);
    execute_lifted_x86(&[0x67, 0x4B, 0x8B, 0x44, 0xA0, 0x08], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), extended_value);

    // FS is added only after the wrapped, zero-extended 32-bit offset.
    let segmented_value = 0x0BAD_F00D_C001_D00Du64;
    memory.write(0x120, &segmented_value.to_le_bytes()).unwrap();
    ctx.write_vreg(fs, 0x100);
    ctx.write_vreg(rbx, 0xABCD_EF01_FFFF_FFF8);
    execute_lifted_x86(&[0x67, 0x64, 0x48, 0x8B, 0x43, 0x28], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), segmented_value);

    // LEA consumes the same zero-extended offset but ignores FS/GS bases.
    ctx.write_vreg(rax, u64::MAX);
    execute_lifted_x86(&[0x67, 0x64, 0x48, 0x8D, 0x43, 0x28], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x20);

    // POP r/m evaluates an ESP-based addr32 destination after incrementing
    // RSP: old stack at 180h, new RSP 188h, destination [ESP+8] = 190h.
    let popped = 0x1357_9BDF_2468_ACE0u64;
    memory.write(0x180, &popped.to_le_bytes()).unwrap();
    ctx.write_vreg(rsp, 0x180);
    execute_lifted_x86(&[0x67, 0x8F, 0x44, 0x24, 0x08], &mut ctx, &mut memory);
    let mut stored = [0u8; 8];
    memory.read(0x190, &mut stored).unwrap();
    assert_eq!(u64::from_le_bytes(stored), popped);
    assert_eq!(ctx.read_vreg(rsp), 0x188);

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
