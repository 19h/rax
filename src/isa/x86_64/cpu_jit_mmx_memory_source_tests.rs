//! Native execution tests for helper-backed MMX m64-source operations.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu_with_mem() -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    (X86_64Vcpu::new(0, memory.clone()), memory)
}

fn configure_long_mode_jit(vcpu: &mut X86_64Vcpu) {
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rflags = 0x246;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
}

#[test]
fn jit_compiles_and_executes_all_mmx_m64_source_encoding_maps() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    // paddb mm0,[rsp]
    // pabsb mm1,[rbp+8]
    // pshufw mm2,[rbx+16],0x1b
    // palignr mm3,[rbx+24],4
    // psraw mm4,[rbx+32]
    // jmp next; next: ret
    memory
        .write_slice(
            &[
                0x0F, 0xFC, 0x04, 0x24, 0x0F, 0x38, 0x1C, 0x4D, 0x08, 0x0F, 0x70, 0x53, 0x10, 0x1B,
                0x0F, 0x3A, 0x0F, 0x5B, 0x18, 0x04, 0x0F, 0xE1, 0x63, 0x20, 0xEB, 0x00, 0xC3,
            ],
            GuestAddress(0),
        )
        .unwrap();
    let sources = [
        u64::from_le_bytes([0x02, 0x01, 0x80, 0x02, 0xFF, 0xF0, 0x03, 0xAB]),
        u64::from_le_bytes([0x80, 0xFF, 0x00, 0x01, 0x7F, 0x81, 0xC0, 0x40]),
        0x4444_3333_2222_1111,
        u64::from_le_bytes([0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7]),
        3,
    ];
    for (index, source) in sources.into_iter().enumerate() {
        memory
            .write_slice(
                &source.to_le_bytes(),
                GuestAddress(0x2000 + index as u64 * 8),
            )
            .unwrap();
    }

    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rbx = 0x2000;
    vcpu.regs.rsp = 0x2000;
    vcpu.regs.rbp = 0x2000;
    vcpu.regs.mm = [
        u64::from_le_bytes([0x01, 0x7F, 0x80, 0xFF, 0x00, 0x10, 0xFE, 0x55]),
        0x1111_1111_1111_1111,
        0x2222_2222_2222_2222,
        u64::from_le_bytes([0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7]),
        u64::from_le_bytes([0x00, 0x80, 0xFF, 0xFF, 0xFF, 0x7F, 0x08, 0x00]),
        0x5555_5555_5555_5555,
        0x6666_6666_6666_6666,
        0x7777_7777_7777_7777,
    ];
    let original = vcpu.regs.mm;
    vcpu.fpu.tag_word = 0xFFFF;

    let region = vcpu
        .jit_compile_region()
        .expect("compile MMX m64-source region")
        .expect("exact MMX m64-source forms should be JIT eligible");
    assert!(region.uses_mmx);
    assert!(!region.uses_vector);

    vcpu.jit_run_region_native(&region);

    assert_eq!(
        vcpu.regs.mm[0],
        u64::from_le_bytes([0x03, 0x80, 0x00, 0x01, 0xFF, 0x00, 0x01, 0x00])
    );
    assert_eq!(
        vcpu.regs.mm[1],
        u64::from_le_bytes([0x80, 0x01, 0x00, 0x01, 0x7F, 0x7F, 0x40, 0x40])
    );
    assert_eq!(vcpu.regs.mm[2], 0x1111_2222_3333_4444);
    assert_eq!(
        vcpu.regs.mm[3],
        u64::from_le_bytes([0xB4, 0xB5, 0xB6, 0xB7, 0xA0, 0xA1, 0xA2, 0xA3])
    );
    assert_eq!(
        vcpu.regs.mm[4],
        u64::from_le_bytes([0x00, 0xF0, 0xFF, 0xFF, 0xFF, 0x0F, 0x01, 0x00])
    );
    assert_eq!(&vcpu.regs.mm[5..], &original[5..]);
    assert_eq!(vcpu.fpu.tag_word, 0);
    assert_eq!(vcpu.regs.rflags, 0x246);
    assert_eq!(vcpu.regs.rbx, 0x2000);
    assert_eq!(vcpu.regs.rsp, 0x2000);
    assert_eq!(vcpu.regs.rbp, 0x2000);
    assert_eq!(vcpu.regs.rip, 26);
}

#[test]
fn jit_faulting_mmx_m64_source_preserves_pre_instruction_state() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    // paddb mm3,[rbx]; jmp next; ret. The 8-byte source straddles the mapped
    // guest-RAM boundary: 4 bytes are readable and 4 bytes are unmapped.
    memory
        .write_slice(&[0x0F, 0xFC, 0x1B, 0xEB, 0x00, 0xC3], GuestAddress(0))
        .unwrap();
    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rbx = 0xFFFC;
    vcpu.regs.rflags = 0x8D7;
    vcpu.regs.mm =
        std::array::from_fn(|index| 0x0123_4567_89AB_CDEFu64.rotate_left(index as u32 * 7));
    let original = vcpu.regs.mm;
    vcpu.fpu.tag_word = 0xFFFF;

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting MMX m64-source region")
        .expect("fault-capable MMX m64 source should retain a native deopt path");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.mm, original);
    assert_eq!(vcpu.fpu.tag_word, 0xFFFF);
    assert_eq!(vcpu.regs.rflags, 0x8D7);
    assert_eq!(vcpu.regs.rip, 0, "fault must restart the MMX instruction");
}
