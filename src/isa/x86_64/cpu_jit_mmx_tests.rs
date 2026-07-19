//! Native x86-64 JIT tests for the architectural MM0-MM7 state bridge.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu_with_mem() -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    (X86_64Vcpu::new(0, mem.clone()), mem)
}

fn test_vcpu() -> X86_64Vcpu {
    test_vcpu_with_mem().0
}

#[test]
fn jit_native_region_synchronizes_mmx_values_and_precise_guest_tags() {
    // push rbp; mov rbp,rsp; push rax; mov rax,[rbp+24];
    // mov qword ptr [rax+x87_tag],0; pop rax; paddb mm0,mm1; leave; ret.
    // The explicit state-slot store models the lowerer's precise EnterMmx
    // commit independently of the trampoline's host-only EMMS cleanup.
    let mut code = vec![
        0x55, 0x48, 0x89, 0xE5, 0x50, 0x48, 0x8B, 0x45, 0x18, 0x48, 0xC7, 0x80,
    ];
    code.extend_from_slice(
        &(crate::smir::lower::X86_GUEST_X87_TAG_WORD_OFFSET as u32).to_le_bytes(),
    );
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x58, 0x0F, 0xFC, 0xC1, 0xC9, 0xC3]);
    let region = JitRegion {
        exec: crate::smir::lower::runtime::ExecMem::new(&code).expect("map MMX region"),
        entry_offset: 0,
        uses_vector: false,
        narrow_vector_opmasks: false,
        uses_mmx: true,
    };
    let mut vcpu = test_vcpu();
    vcpu.regs.rip = 0x1000;
    vcpu.regs.rflags = 0x2;
    vcpu.regs.mm = [
        0x00ff_7f80_0102_0304,
        0x0102_0304_0506_0708,
        2,
        3,
        4,
        5,
        6,
        7,
    ];
    vcpu.fpu.tag_word = 0xFFFF;

    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.mm[0], 0x0101_8284_0608_0A0C);
    assert_eq!(
        &vcpu.regs.mm[1..],
        &[0x0102_0304_0506_0708, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rip, 0x1000);
}

#[test]
fn jit_compiles_and_executes_lifted_register_mmx_logic() {
    let (mut vcpu, mem) = test_vcpu_with_mem();
    // pand mm0,mm1; jmp next; ret. The RET block is an interpreter
    // frontier, leaving the MMX instruction in the executable native block.
    mem.write_slice(&[0x0F, 0xDB, 0xC1, 0xEB, 0x00, 0xC3], GuestAddress(0))
        .unwrap();
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rflags = 0x2;
    vcpu.regs.mm[0] = 0xF0F0_0FF0_AA55_1234;
    vcpu.regs.mm[1] = 0x0FF0_FFFF_0F0F_FFFF;
    vcpu.fpu.tag_word = 0xFFFF;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);

    let region = vcpu
        .jit_compile_region()
        .expect("compile MMX region")
        .expect("register MMX logic should be JIT eligible");
    assert!(region.uses_mmx);
    assert!(!region.uses_vector);

    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.mm[0], 0x00F0_0FF0_0A05_1234);
    assert_eq!(vcpu.regs.mm[1], 0x0FF0_FFFF_0F0F_FFFF);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rip, 5);
}

#[test]
fn jit_compiles_and_executes_mmx_movq_memory_helpers() {
    let (mut vcpu, mem) = test_vcpu_with_mem();
    // movq mm7,[rbx]; movq [rbx+8],mm7; jmp next; ret.
    mem.write_slice(
        &[0x0F, 0x6F, 0x3B, 0x0F, 0x7F, 0x7B, 0x08, 0xEB, 0x00, 0xC3],
        GuestAddress(0),
    )
    .unwrap();
    let source = 0xFEDC_BA98_7654_3210u64;
    mem.write_slice(&source.to_le_bytes(), GuestAddress(0x2000))
        .unwrap();
    mem.write_slice(
        &0xCCCC_CCCC_CCCC_CCCCu64.to_le_bytes(),
        GuestAddress(0x2008),
    )
    .unwrap();
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rbx = 0x2000;
    vcpu.regs.rflags = 0x246;
    vcpu.regs.mm = std::array::from_fn(|index| 0x1111_1111_1111_1111u64 * index as u64);
    let original = vcpu.regs.mm;
    vcpu.fpu.tag_word = 0xFFFF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);

    let region = vcpu
        .jit_compile_region()
        .expect("compile MMX memory region")
        .expect("MMX MOVQ memory forms should be JIT eligible");
    assert!(region.uses_mmx);
    assert!(!region.uses_vector);

    vcpu.jit_run_region_native(&region);

    let mut stored = [0u8; 8];
    mem.read_slice(&mut stored, GuestAddress(0x2008)).unwrap();
    assert_eq!(u64::from_le_bytes(stored), source);
    assert_eq!(vcpu.regs.mm[7], source);
    assert_eq!(&vcpu.regs.mm[..7], &original[..7]);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rflags, 0x246);
    assert_eq!(vcpu.regs.rip, 9);
}

#[test]
fn jit_scalar_memory_helper_preserves_live_register_mmx_state() {
    let (mut vcpu, mem) = test_vcpu_with_mem();
    // pand mm0,mm1; mov rax,[rbx]; jmp next; ret.
    mem.write_slice(
        &[0x0F, 0xDB, 0xC1, 0x48, 0x8B, 0x03, 0xEB, 0x00, 0xC3],
        GuestAddress(0),
    )
    .unwrap();
    let scalar = 0x0123_4567_89AB_CDEFu64;
    mem.write_slice(&scalar.to_le_bytes(), GuestAddress(0x2000))
        .unwrap();
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rbx = 0x2000;
    vcpu.regs.rflags = 0x246;
    vcpu.regs.mm[0] = 0xF0F0_0FF0_AA55_1234;
    vcpu.regs.mm[1] = 0x0FF0_FFFF_0F0F_FFFF;
    vcpu.fpu.tag_word = 0xFFFF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);

    let region = vcpu
        .jit_compile_region()
        .expect("compile mixed MMX/scalar-memory region")
        .expect("scalar MMU helpers should preserve live MMX state");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rax, scalar);
    assert_eq!(vcpu.regs.mm[0], 0x00F0_0FF0_0A05_1234);
    assert_eq!(vcpu.regs.mm[1], 0x0FF0_FFFF_0F0F_FFFF);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rflags, 0x246);
    assert_eq!(vcpu.regs.rip, 8);
}

#[test]
fn jit_faulting_mmx_movq_memory_helper_preserves_pre_instruction_state() {
    let (mut vcpu, mem) = test_vcpu_with_mem();
    // movq mm3,[rbx]; jmp next; ret. RBX points one byte beyond guest RAM.
    mem.write_slice(&[0x0F, 0x6F, 0x1B, 0xEB, 0x00, 0xC3], GuestAddress(0))
        .unwrap();
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rbx = 0x10000;
    vcpu.regs.rflags = 0x8D7;
    vcpu.regs.mm =
        std::array::from_fn(|index| 0x0123_4567_89AB_CDEFu64.rotate_left(index as u32 * 7));
    let original = vcpu.regs.mm;
    vcpu.fpu.tag_word = 0xFFFF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting MMX memory region")
        .expect("fault-capable MMX MOVQ should retain a native deopt path");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.mm, original);
    assert_eq!(vcpu.fpu.tag_word, 0xFFFF);
    assert_eq!(vcpu.regs.rflags, 0x8D7);
    assert_eq!(vcpu.regs.rip, 0, "fault must restart the MOVQ instruction");
}

#[test]
fn jit_call_helper_round_trips_interpreter_modified_mmx_state() {
    let (mut vcpu, mem) = test_vcpu_with_mem();
    // pand mm0,mm1; call callee; pand mm0,mm2; jmp next;
    // callee: por mm0,mm3; ret; next: ret.
    mem.write_slice(
        &[
            0x0F, 0xDB, 0xC1, 0xE8, 0x05, 0x00, 0x00, 0x00, 0x0F, 0xDB, 0xC2, 0xEB, 0x04, 0x0F,
            0xEB, 0xC3, 0xC3, 0xC3,
        ],
        GuestAddress(0),
    )
    .unwrap();
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rflags = 0x202;
    vcpu.regs.mm[0] = 0xF0F0_0FF0_AA55_1234;
    vcpu.regs.mm[1] = 0x0FF0_FFFF_0F0F_FFFF;
    vcpu.regs.mm[2] = 0xFFFF_00FF_FFFF_0FF0;
    vcpu.regs.mm[3] = 0x8000_0000_5000_0001;
    let expected = ((vcpu.regs.mm[0] & vcpu.regs.mm[1]) | vcpu.regs.mm[3]) & vcpu.regs.mm[2];
    vcpu.fpu.tag_word = 0xFFFF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(true);

    let region = vcpu
        .jit_compile_region()
        .expect("compile MMX call-through region")
        .expect("MMX call-through should be JIT eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.mm[0], expected);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rflags, 0x202);
    assert_eq!(vcpu.regs.rip, 17);
}
