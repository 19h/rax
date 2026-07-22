//! Direct-execution regressions for APX-promoted MOVBE.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DATA_ADDRESS: u64 = 0x2000;
const STATUS_MASK: u64 = 0x08D5;
const INITIAL_RFLAGS: u64 = 0x2 | STATUS_MASK | flags::bits::DF;
const INITIAL_R8: u64 = 0xA1B2_C3D4_E5F6_7788;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rflags = INITIAL_RFLAGS;
    vcpu.set_apx_enabled(true);
    vcpu
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

fn width_mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1_u64 << (u32::from(bytes) * 8)) - 1
    }
}

fn merge_gpr(old: u64, value: u64, bytes: u8) -> u64 {
    match bytes {
        2 => (old & !width_mask(bytes)) | (value & width_mask(bytes)),
        4 => value as u32 as u64,
        8 => value,
        _ => unreachable!(),
    }
}

fn swap(value: u64, bytes: u8) -> u64 {
    match bytes {
        2 => u64::from((value as u16).swap_bytes()),
        4 => u64::from((value as u32).swap_bytes()),
        8 => value.swap_bytes(),
        _ => unreachable!(),
    }
}

fn p1(bytes: u8) -> u8 {
    match bytes {
        2 => 0x7D,
        4 => 0x7C,
        8 => 0xFC,
        _ => unreachable!(),
    }
}

#[test]
fn direct_apx_movbe_covers_both_directions_widths_and_source_classes() {
    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;

    for bytes in [2, 4, 8] {
        let expected = swap(SOURCE, bytes);

        // Opcode 60: MOVBE r8,rbx.
        let code = [0x62, 0x74, p1(bytes), 0x08, 0x60, 0xC3];
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.regs.r8 = INITIAL_R8;
        vcpu.regs.rbx = SOURCE;
        assert!(vcpu.step().expect("MOVBE register load form").is_none());
        assert_eq!(vcpu.regs.r8, merge_gpr(INITIAL_R8, expected, bytes));
        assert_eq!(vcpu.regs.rbx, SOURCE);
        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);
        assert_eq!(vcpu.regs.rip, code.len() as u64);

        // Opcode 60: MOVBE r8,[rbx].
        let code = [0x62, 0x74, p1(bytes), 0x08, 0x60, 0x03];
        let memory = memory_with_code(&code);
        memory
            .write_slice(
                &SOURCE.to_le_bytes()[..usize::from(bytes)],
                GuestAddress(DATA_ADDRESS),
            )
            .unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.r8 = INITIAL_R8;
        vcpu.regs.rbx = DATA_ADDRESS;
        assert!(vcpu.step().expect("MOVBE memory load form").is_none());
        assert_eq!(vcpu.regs.r8, merge_gpr(INITIAL_R8, expected, bytes));
        assert_eq!(vcpu.regs.rbx, DATA_ADDRESS);
        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);

        // Opcode 61: MOVBE r8,rax.
        let code = [0x62, 0xD4, p1(bytes), 0x08, 0x61, 0xC0];
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.regs.rax = SOURCE;
        vcpu.regs.r8 = INITIAL_R8;
        assert!(vcpu.step().expect("MOVBE register store form").is_none());
        assert_eq!(vcpu.regs.r8, merge_gpr(INITIAL_R8, expected, bytes));
        assert_eq!(vcpu.regs.rax, SOURCE);
        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);

        // Opcode 61: MOVBE [rbx],r8.
        let code = [0x62, 0x74, p1(bytes), 0x08, 0x61, 0x03];
        let memory = memory_with_code(&code);
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.regs.r8 = SOURCE;
        vcpu.regs.rbx = DATA_ADDRESS;
        assert!(vcpu.step().expect("MOVBE memory store form").is_none());
        let mut observed = [0_u8; 8];
        memory
            .read_slice(
                &mut observed[..usize::from(bytes)],
                GuestAddress(DATA_ADDRESS),
            )
            .unwrap();
        assert_eq!(
            &observed[..usize::from(bytes)],
            &expected.to_le_bytes()[..usize::from(bytes)]
        );
        assert_eq!(vcpu.regs.r8, SOURCE);
        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);
    }
}

#[test]
fn direct_apx_movbe_uses_egpr_and_memory_index_extensions() {
    // MOVBE r24,r16 and the reverse direction both exercise R'/B4.
    for (opcode, source, expected_r16, expected_r24) in [
        (
            0x60,
            0x0123_4567_89AB_CDEF,
            0x0123_4567_89AB_CDEF,
            0xEFCD_AB89_6745_2301,
        ),
        (
            0x61,
            0x0123_4567_89AB_CDEF,
            0xEFCD_AB89_6745_2301,
            0x0123_4567_89AB_CDEF,
        ),
    ] {
        let code = [0x62, 0x6C, 0xFC, 0x08, opcode, 0xC0];
        let mut vcpu = test_vcpu(memory_with_code(&code));
        if opcode == 0x60 {
            vcpu.regs.r16 = source;
            vcpu.regs.r24 = 0;
        } else {
            vcpu.regs.r16 = 0;
            vcpu.regs.r24 = source;
        }
        assert!(vcpu.step().expect("EGPR MOVBE").is_none());
        assert_eq!(vcpu.regs.r16, expected_r16);
        assert_eq!(vcpu.regs.r24, expected_r24);
    }

    // P1.X4=0 extends the SIB index to R16 for memory forms.
    let address = DATA_ADDRESS + 0x18;
    let code = [0x62, 0x74, 0xF8, 0x08, 0x60, 0x04, 0x03];
    let memory = memory_with_code(&code);
    memory
        .write_slice(
            &0x0123_4567_89AB_CDEF_u64.to_le_bytes(),
            GuestAddress(address),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rbx = DATA_ADDRESS;
    vcpu.regs.r16 = 0x18;
    assert!(vcpu.step().expect("indexed MOVBE load").is_none());
    assert_eq!(vcpu.regs.r8, 0xEFCD_AB89_6745_2301);

    let code = [0x62, 0x74, 0xF8, 0x08, 0x61, 0x04, 0x03];
    let memory = memory_with_code(&code);
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.regs.r8 = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rbx = DATA_ADDRESS;
    vcpu.regs.r16 = 0x18;
    assert!(vcpu.step().expect("indexed MOVBE store").is_none());
    let mut observed = [0_u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(address))
        .unwrap();
    assert_eq!(u64::from_le_bytes(observed), 0xEFCD_AB89_6745_2301);
}

fn assert_ud(code: &[u8], name: &str) {
    let mut vcpu = test_vcpu(memory_with_code(code));
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rbx = 0x1111_2222_3333_4444;
    vcpu.regs.r8 = INITIAL_R8;
    let before = register_image(&vcpu);
    let error = format!("{:#}", vcpu.step().expect_err(name));
    assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
    assert_eq!(register_image(&vcpu), before, "{name}");
    assert_eq!(vcpu.regs.rip, 0, "{name} RIP");
}

#[test]
fn direct_apx_movbe_reserved_fields_fault_without_commit() {
    for (code, name) in [
        (&[0x62, 0x74, 0x7E, 0x08, 0x60, 0xC3][..], "F3 pp"),
        (&[0x62, 0x74, 0x7F, 0x08, 0x61, 0xC3][..], "F2 pp"),
        (&[0x62, 0x74, 0x7C, 0x18, 0x60, 0xC3][..], "ND"),
        (&[0x62, 0x74, 0x7C, 0x0C, 0x61, 0xC3][..], "NF"),
        (&[0x62, 0x74, 0x7C, 0x88, 0x60, 0xC3][..], "z"),
        (&[0x62, 0x74, 0x7C, 0x28, 0x61, 0xC3][..], "LL"),
        (&[0x62, 0x74, 0x7C, 0x09, 0x60, 0xC3][..], "payload bit 0"),
        (&[0x62, 0x74, 0x74, 0x08, 0x61, 0xC3][..], "V3:0"),
        (&[0x62, 0x74, 0x7C, 0x00, 0x60, 0xC3][..], "V4"),
        (&[0x62, 0x74, 0x78, 0x08, 0x61, 0xC3][..], "register U"),
        (
            &[0x66, 0x62, 0x74, 0x7C, 0x08, 0x60, 0xC3][..],
            "leading 66",
        ),
    ] {
        assert_ud(code, name);
    }
}

#[test]
fn direct_apx_movbe_apx_and_memory_faults_are_precise_and_noncommitting() {
    for opcode in [0x60, 0x61] {
        let code = [0x62, 0x74, 0xFC, 0x08, opcode, 0x03];
        let memory = memory_with_code(&code);
        let mut disabled = test_vcpu(memory.clone());
        disabled.set_apx_enabled(false);
        disabled.regs.rbx = 0x2_0000;
        disabled.regs.r8 = INITIAL_R8;
        let before = register_image(&disabled);
        let error = format!("{:#}", disabled.step().expect_err("APX disabled"));
        assert!(error.contains("IDT entry 6 not present"), "{error}");
        assert_eq!(register_image(&disabled), before);

        let mut enabled = test_vcpu(memory);
        enabled.regs.rbx = 0x2_0000;
        enabled.regs.r8 = INITIAL_R8;
        let before = register_image(&enabled);
        assert!(enabled.step().is_err());
        assert_eq!(register_image(&enabled), before, "opcode={opcode:#04x}");
        assert_eq!(enabled.regs.rip, 0);
    }
}
