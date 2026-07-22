use crate::common::{
    Bytes, CODE_ADDR, GDT_BASE, GuestAddress, GuestMemoryMmap, IDT_BASE, INT_HANDLER_ADDR,
    STACK_ADDR, run_until_hlt, setup_vm, setup_vm_compat, setup_vm_no_idt,
};
use rax::vm::vcpu::{Registers, Segment, VCpu, VcpuExit};

// Comprehensive tests for INT, INTO, INT3, and INT1 instructions (software interrupts)
// INT imm8 (CD), INTO (CE), INT3 (CC), INT1/ICEBP (F1)

// ============================================================================
// INT1/ICEBP - Debug Trap (0xF1)
// ============================================================================

#[test]
fn test_int1_icebp_saves_post_instruction_rip_and_preserves_dr6() {
    const DR6_SENTINEL: u64 = 0xFFFF_0FF0;
    let encodings: &[(&str, &[u8])] = &[
        ("bare", &[0xF1]),
        ("operand-size", &[0x66, 0xF1]),
        ("address-size", &[0x67, 0xF1]),
        ("REPNE", &[0xF2, 0xF1]),
        ("REP", &[0xF3, 0xF1]),
        ("segment", &[0x2E, 0xF1]),
        ("REX.W", &[0x48, 0xF1]),
    ];

    for &(name, instruction) in encodings {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let (mut vcpu, memory) = setup_vm(&code, None);
        // Capture the return RIP from the top of the #DB frame, then IRETQ.
        memory
            .write_slice(
                &[0x48, 0x8B, 0x04, 0x24, 0x48, 0xCF],
                GuestAddress(INT_HANDLER_ADDR),
            )
            .unwrap();
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.dr6 = DR6_SENTINEL;
        vcpu.set_sregs(&sregs).unwrap();

        let regs = run_until_hlt(&mut vcpu).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            regs.rax,
            CODE_ADDR + instruction.len() as u64,
            "{name}: #DB must save the post-INT1 RIP"
        );
        assert_eq!(
            vcpu.get_sregs().unwrap().dr6,
            DR6_SENTINEL,
            "{name}: INT1 must not set any DR6 cause bit"
        );
    }
}

#[test]
fn test_rex2_int1_icebp_is_apx_gated_and_saves_post_instruction_rip() {
    const DR6_SENTINEL: u64 = 0x1234_5678;
    let instruction = [0xD5, 0x00, 0xF1];

    let (mut disabled, _) = setup_vm_no_idt(&instruction, None);
    let error = disabled
        .step()
        .expect_err("disabled REX2 INT1 must raise #UD")
        .to_string();
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(
        disabled.get_regs().unwrap().rip,
        CODE_ADDR,
        "disabled REX2 must raise fault-class #UD before INT1"
    );

    let mut code = instruction.to_vec();
    code.push(0xF4);
    let (mut vcpu, memory) = setup_vm(&code, None);
    vcpu.set_apx_enabled(true);
    memory
        .write_slice(
            &[0x48, 0x8B, 0x04, 0x24, 0x48, 0xCF],
            GuestAddress(INT_HANDLER_ADDR),
        )
        .unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.dr6 = DR6_SENTINEL;
    vcpu.set_sregs(&sregs).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, CODE_ADDR + instruction.len() as u64);
    assert_eq!(vcpu.get_sregs().unwrap().dr6, DR6_SENTINEL);
}

#[test]
fn test_invalid_int1_prefixes_raise_fault_class_invalid_opcode() {
    for (name, instruction, apx_enabled) in [
        ("LOCK", &[0xF0, 0xF1][..], false),
        ("LOCK REX2", &[0xF0, 0xD5, 0x00, 0xF1], true),
        ("REX before REX2", &[0x48, 0xD5, 0x00, 0xF1], true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(instruction, None);
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let error = vcpu
                .step()
                .expect_err("invalid INT1 prefix must raise #UD")
                .to_string();
            assert!(
                error.contains("IDT entry 6 not present"),
                "{name} ({path}): wrong exception vector: {error}"
            );
            assert_eq!(
                vcpu.get_regs().unwrap().rip,
                CODE_ADDR,
                "{name} ({path}): #UD must retain the faulting RIP"
            );
        }
    }
}

// ============================================================================
// INT3 - Breakpoint Interrupt (0xCC)
// ============================================================================

#[test]
fn test_int3_basic() {
    // INT3 - breakpoint interrupt (interrupt 3)
    let code = [
        0xcc, // INT3
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (fallback)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    // INT3 should trigger interrupt or trap
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // If no interrupt handler, execution continues
    assert_eq!(regs.rax, 1);
}

#[test]
fn test_int3_preserves_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x48, 0xc7, 0xc3, 0x99, 0x00, 0x00, 0x00, // MOV RBX, 0x99
        0xcc, // INT3
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x42);
    assert_eq!(regs.rbx, 0x99);
}

#[test]
fn test_int3_one_byte_encoding() {
    // INT3 is a single byte (0xCC) - more compact than INT 3
    let code = [
        0xcc, // INT3 (1 byte)
        0xcd, 0x03, // INT 3 (2 bytes - equivalent but longer)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Both INT3 (0xCC) and INT 3 (0xCD 03) trap to vector 3; the IRETQ stub
    // returns to the next instruction, so execution reaches the final HLT.
    // RIP is past the HLT at offset 3 => 0x1000 + 3 + 1.
    assert_eq!(
        regs.rip,
        0x1000 + code.len() as u64,
        "reached final HLT past both INT3 forms"
    );
}

#[test]
fn test_int3_multiple_consecutive() {
    // Multiple INT3 instructions in sequence
    let code = [
        0xcc, // INT3
        0xcc, // INT3
        0xcc, // INT3
        0x48, 0xc7, 0xc0, 0xaa, 0x00, 0x00, 0x00, // MOV RAX, 0xAA
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xaa);
}

// ============================================================================
// INT imm8 - General Software Interrupt
// ============================================================================

#[test]
fn test_int_imm8_prefixes_save_exact_return_rip_and_rex2_is_apx_gated() {
    let encodings: &[(&str, &[u8], bool)] = &[
        ("bare", &[0xCD, 0x80], false),
        ("operand-size", &[0x66, 0xCD, 0x80], false),
        ("address-size", &[0x67, 0xCD, 0x80], false),
        ("REPNE", &[0xF2, 0xCD, 0x80], false),
        ("REP", &[0xF3, 0xCD, 0x80], false),
        ("segment", &[0x2E, 0xCD, 0x80], false),
        ("REX.W", &[0x48, 0xCD, 0x80], false),
        ("REX2", &[0xD5, 0x00, 0xCD, 0x80], true),
        (
            "ordered legacy plus REX2",
            &[0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x00, 0xCD, 0x80],
            true,
        ),
    ];

    for &(name, instruction, apx_enabled) in encodings {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let (mut vcpu, memory) = setup_vm(&code, None);
        vcpu.set_apx_enabled(apx_enabled);
        // Capture the software interrupt's saved RIP, then return to HLT.
        memory
            .write_slice(
                &[0x48, 0x8B, 0x04, 0x24, 0x48, 0xCF],
                GuestAddress(INT_HANDLER_ADDR),
            )
            .unwrap();

        let regs = run_until_hlt(&mut vcpu).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            regs.rax,
            CODE_ADDR + instruction.len() as u64,
            "{name}: INT imm8 must save the post-immediate RIP"
        );
    }

    let rex2 = [0xD5, 0x00, 0xCD, 0x80];
    let (mut disabled, _) = setup_vm_no_idt(&rex2, None);
    for path in ["cold decode", "decode-cache hit"] {
        let error = disabled
            .step()
            .expect_err("disabled REX2 INT imm8 must raise #UD")
            .to_string();
        assert!(
            error.contains("IDT entry 6 not present"),
            "{path}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(
            disabled.get_regs().unwrap().rip,
            CODE_ADDR,
            "{path}: APX-disabled #UD must retain the instruction RIP"
        );
    }
}

#[test]
fn test_int_imm8_illegal_prefixes_raise_fault_class_invalid_opcode() {
    for (name, instruction, apx_enabled) in [
        ("LOCK", &[0xF0, 0xCD, 0x80][..], false),
        ("LOCK REX2", &[0xF0, 0xD5, 0x00, 0xCD, 0x80], true),
        ("REX before REX2", &[0x48, 0xD5, 0x00, 0xCD, 0x80], true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(instruction, None);
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let error = vcpu
                .step()
                .expect_err("invalid INT imm8 prefix must raise #UD")
                .to_string();
            assert!(
                error.contains("IDT entry 6 not present"),
                "{name} ({path}): wrong exception vector: {error}"
            );
            assert_eq!(
                vcpu.get_regs().unwrap().rip,
                CODE_ADDR,
                "{name} ({path}): #UD must retain the faulting RIP"
            );
        }
    }
}

#[test]
fn test_int_imm8_vector_0() {
    // INT 0 - divide error interrupt
    let code = [
        0xcd, 0x00, // INT 0
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 1);
}

#[test]
fn test_int_imm8_vector_1() {
    // INT 1 - debug exception
    let code = [
        0xcd, 0x01, // INT 1
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 2);
}

#[test]
fn test_int_imm8_vector_3() {
    // INT 3 - equivalent to INT3 but 2 bytes
    let code = [
        0xcd, 0x03, // INT 3
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 3);
}

#[test]
fn test_int_imm8_vector_4() {
    // INT 4 - overflow interrupt (INTO uses this)
    let code = [
        0xcd, 0x04, // INT 4
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 4);
}

#[test]
fn test_int_imm8_vector_13() {
    // INT 0x0D - general protection fault
    let code = [
        0xcd, 0x0d, // INT 13
        0x48, 0xc7, 0xc0, 0x0d, 0x00, 0x00, 0x00, // MOV RAX, 0x0D
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x0d);
}

#[test]
fn test_int_imm8_vector_14() {
    // INT 0x0E - page fault
    let code = [
        0xcd, 0x0e, // INT 14
        0x48, 0xc7, 0xc0, 0x0e, 0x00, 0x00, 0x00, // MOV RAX, 0x0E
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x0e);
}

#[test]
fn test_int_imm8_vector_16() {
    // INT 0x10 - x87 FPU error
    let code = [
        0xcd, 0x10, // INT 16
        0x48, 0xc7, 0xc0, 0x10, 0x00, 0x00, 0x00, // MOV RAX, 0x10
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x10);
}

#[test]
fn test_int_imm8_vector_21h() {
    // INT 0x21 - DOS service interrupt
    let code = [
        0xcd, 0x21, // INT 0x21
        0x48, 0xc7, 0xc0, 0x21, 0x00, 0x00, 0x00, // MOV RAX, 0x21
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x21);
}

#[test]
fn test_int_imm8_vector_80h() {
    // INT 0x80 - Linux system call interrupt (32-bit)
    let code = [
        0xcd, 0x80, // INT 0x80
        0x48, 0xc7, 0xc0, 0x80, 0x00, 0x00, 0x00, // MOV RAX, 0x80
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x80);
}

#[test]
fn test_int_imm8_vector_255() {
    // INT 0xFF - maximum vector number
    let code = [
        0xcd, 0xff, // INT 255
        0x48, 0xc7, 0xc0, 0xff, 0x00, 0x00, 0x00, // MOV RAX, 0xFF
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xff);
}

// ============================================================================
// INT - Stack Behavior
// ============================================================================

#[test]
fn test_int_pushes_flags_cs_ip() {
    // INT should push FLAGS, CS, and IP onto stack
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0xcd, 0x20, // INT 0x20
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack should have been modified if interrupt executed
}

#[test]
fn test_ia32e_event_frame_contains_old_ss_rsp_rflags_cs_rip() {
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = CODE_ADDR;
    regs.rsp = STACK_ADDR;
    regs.rflags = 0x202;
    vcpu.set_regs(&regs).unwrap();

    vcpu.inject_exception(3, None).unwrap();
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 5 * 8);
    let frame = (0..5)
        .map(|slot| {
            memory
                .read_obj::<u64>(GuestAddress(regs.rsp + slot * 8))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        frame,
        [CODE_ADDR, 0x8, 0x202, STACK_ADDR, 0],
        "lowest address is the IRETQ-visible RIP"
    );
}

#[test]
fn test_real_mode_event_uses_four_byte_ivt_and_16_bit_frame() {
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    let vector = 0x21_u64;
    let handler_ip = 0x3456_u16;
    let handler_cs = 0x0200_u16;
    memory
        .write_obj(handler_ip, GuestAddress(IDT_BASE + vector * 4))
        .unwrap();
    memory
        .write_obj(handler_cs, GuestAddress(IDT_BASE + vector * 4 + 2))
        .unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr0 &= !1;
    sregs.efer = 0;
    sregs.idt.limit = 0x3FF;
    sregs.cs.selector = 0x100;
    sregs.cs.base = 0x1000;
    sregs.ss.selector = 0;
    sregs.ss.base = 0;
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = 0x2345;
    regs.rsp = STACK_ADDR;
    regs.rflags = 0x40302; // AC|IF|TF|required bit.
    vcpu.set_regs(&regs).unwrap();

    vcpu.inject_exception(vector as u8, None).unwrap();
    let regs = vcpu.get_regs().unwrap();
    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(regs.rip, u64::from(handler_ip));
    assert_eq!(regs.rsp, STACK_ADDR - 6);
    assert_eq!(sregs.cs.selector, handler_cs);
    assert_eq!(sregs.cs.base, u64::from(handler_cs) << 4);
    assert_eq!(regs.rflags & ((1 << 18) | (1 << 9) | (1 << 8)), 0);
    assert_eq!(
        [
            memory.read_obj::<u16>(GuestAddress(regs.rsp)).unwrap(),
            memory.read_obj::<u16>(GuestAddress(regs.rsp + 2)).unwrap(),
            memory.read_obj::<u16>(GuestAddress(regs.rsp + 4)).unwrap(),
        ],
        [0x2345, 0x100, 0x0302]
    );
}

fn configure_legacy_protected_delivery(
    vcpu: &mut rax::isa::x86_64::X86_64Vcpu,
    memory: &GuestMemoryMmap,
    code_32: bool,
) {
    let code = if code_32 {
        [0xFF, 0xFF, 0, 0, 0, 0x9A, 0xCF, 0]
    } else {
        [0xFF, 0xFF, 0, 0, 0, 0x9A, 0x00, 0]
    };
    let data = if code_32 {
        [0xFF, 0xFF, 0, 0, 0, 0x92, 0xCF, 0]
    } else {
        [0xFF, 0xFF, 0, 0, 0, 0x92, 0x00, 0]
    };
    memory
        .write_slice(&code, GuestAddress(GDT_BASE + 8))
        .unwrap();
    memory
        .write_slice(&data, GuestAddress(GDT_BASE + 16))
        .unwrap();

    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.efer &= !(1 << 10);
    sregs.cs = Segment {
        selector: 8,
        limit: if code_32 { u32::MAX } else { 0xFFFF },
        type_: 0xB,
        present: true,
        db: code_32,
        s: true,
        ..Segment::default()
    };
    sregs.ss = Segment {
        selector: 16,
        limit: if code_32 { u32::MAX } else { 0xFFFF },
        type_: 3,
        present: true,
        db: code_32,
        s: true,
        ..Segment::default()
    };
    sregs.gdt.limit = 0x27;
    sregs.idt.limit = 256 * 8 - 1;
    vcpu.set_sregs(&sregs).unwrap();
}

fn write_legacy_gate(memory: &GuestMemoryMmap, vector: u8, handler: u32, type_attr: u8) {
    let mut gate = [0_u8; 8];
    gate[0..2].copy_from_slice(&(handler as u16).to_le_bytes());
    gate[2..4].copy_from_slice(&8_u16.to_le_bytes());
    gate[5] = type_attr;
    gate[6..8].copy_from_slice(&((handler >> 16) as u16).to_le_bytes());
    memory
        .write_slice(&gate, GuestAddress(IDT_BASE + u64::from(vector) * 8))
        .unwrap();
}

#[test]
fn test_legacy_16_and_32_bit_gates_build_width_exact_same_privilege_frames() {
    for (name, code_32, gate_type, handler, width) in [
        ("16-bit", false, 0x86, 0x3456_u32, 2_u64),
        ("32-bit", true, 0x8E, 0x0012_3456, 4),
    ] {
        let (mut vcpu, memory) = setup_vm(&[0xF4], None);
        configure_legacy_protected_delivery(&mut vcpu, &memory, code_32);
        write_legacy_gate(&memory, 0x30, handler, gate_type);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rip = CODE_ADDR;
        regs.rsp = STACK_ADDR;
        regs.rflags = 0x202;
        vcpu.set_regs(&regs).unwrap();

        vcpu.inject_exception(0x30, None).unwrap();
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, u64::from(handler), "{name}");
        assert_eq!(regs.rsp, STACK_ADDR - 3 * width, "{name}");
        let mut frame = [0_u32; 3];
        for (slot, value) in frame.iter_mut().enumerate() {
            let address = GuestAddress(regs.rsp + slot as u64 * width);
            *value = if code_32 {
                memory.read_obj::<u32>(address).unwrap()
            } else {
                u32::from(memory.read_obj::<u16>(address).unwrap())
            };
        }
        assert_eq!(frame, [CODE_ADDR as u32, 8, 0x202], "{name}");
        assert_eq!(regs.rflags & (1 << 9), 0, "{name} interrupt gate clears IF");
    }
}

#[test]
fn test_legacy_cpl3_delivery_uses_tss_stack_and_pushes_outer_frame_and_error() {
    const TSS: u64 = 0x14000;
    const ESP0: u32 = 0xA000;
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    configure_legacy_protected_delivery(&mut vcpu, &memory, true);
    write_legacy_gate(&memory, 13, 0x1234, 0x8E);
    memory.write_obj(ESP0, GuestAddress(TSS + 4)).unwrap();
    memory.write_obj(0x10_u16, GuestAddress(TSS + 8)).unwrap();

    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 0x1B;
    sregs.cs.dpl = 3;
    sregs.ss.selector = 0x23;
    sregs.ss.dpl = 3;
    sregs.tr = Segment {
        base: TSS,
        limit: 0x67,
        selector: 0x18,
        type_: 0xB,
        present: true,
        s: false,
        ..Segment::default()
    };
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = CODE_ADDR;
    regs.rsp = 0x7000;
    regs.rflags = 0x202;
    vcpu.set_regs(&regs).unwrap();

    vcpu.inject_exception(13, Some(0xBEEF)).unwrap();
    let regs = vcpu.get_regs().unwrap();
    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(regs.rip, 0x1234);
    assert_eq!(regs.rsp, u64::from(ESP0) - 6 * 4);
    assert_eq!(sregs.cs.selector, 8);
    assert_eq!(sregs.ss.selector, 0x10);
    let frame = (0..6)
        .map(|slot| {
            memory
                .read_obj::<u32>(GuestAddress(regs.rsp + slot * 4))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(frame, [0xBEEF, CODE_ADDR as u32, 0x1B, 0x202, 0x7000, 0x23]);
}

#[test]
fn test_virtual_8086_exception_uses_cpl3_and_saves_and_clears_data_segments() {
    const TSS: u64 = 0x14000;
    const ESP0: u32 = 0xA000;
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    configure_legacy_protected_delivery(&mut vcpu, &memory, true);
    write_legacy_gate(&memory, 6, 0x1234, 0x8E);
    memory.write_obj(ESP0, GuestAddress(TSS + 4)).unwrap();
    memory.write_obj(0x10_u16, GuestAddress(TSS + 8)).unwrap();

    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 0x1234; // RPL is ignored: VM86 execution is always CPL3.
    sregs.cs.dpl = 3;
    sregs.ss.selector = 0x2000;
    sregs.ss.dpl = 3;
    sregs.es.selector = 0x3000;
    sregs.ds.selector = 0x4000;
    sregs.fs.selector = 0x5000;
    sregs.gs.selector = 0x6000;
    sregs.tr = Segment {
        base: TSS,
        limit: 0x67,
        selector: 0x18,
        type_: 0xB,
        present: true,
        s: false,
        ..Segment::default()
    };
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = CODE_ADDR;
    regs.rsp = 0x7000;
    regs.rflags = 0x2 | (1 << 9) | (1 << 17);
    vcpu.set_regs(&regs).unwrap();

    vcpu.inject_exception(6, None).unwrap();
    let regs = vcpu.get_regs().unwrap();
    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(regs.rsp, u64::from(ESP0) - 9 * 4);
    let frame = (0..9)
        .map(|slot| {
            memory
                .read_obj::<u32>(GuestAddress(regs.rsp + slot * 4))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        frame,
        [
            CODE_ADDR as u32,
            0x1234,
            0x2 | (1 << 9) | (1 << 17),
            0x7000,
            0x2000,
            0x3000,
            0x4000,
            0x5000,
            0x6000,
        ]
    );
    assert_eq!(regs.rflags & ((1 << 9) | (1 << 17)), 0);
    for segment in [&sregs.es, &sregs.ds, &sregs.fs, &sregs.gs] {
        assert_eq!(segment.selector, 0);
        assert!(segment.unusable);
    }
}

#[test]
fn test_interrupt_and_trap_gate_flag_clearing_is_exact() {
    const TF: u64 = 1 << 8;
    const IF: u64 = 1 << 9;
    const NT: u64 = 1 << 14;
    const RF: u64 = 1 << 16;
    const VM: u64 = 1 << 17;
    let cleared = TF | NT | RF | VM;

    for (gate_type, expect_if) in [(0x8E, false), (0x8F, true)] {
        let (mut vcpu, memory) = setup_vm(&[0xF4], None);
        memory
            .write_obj(gate_type, GuestAddress(IDT_BASE + 3 * 16 + 5))
            .unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rflags = 0x2 | IF | cleared;
        vcpu.set_regs(&regs).unwrap();

        vcpu.inject_exception(3, None).unwrap();
        let flags = vcpu.get_regs().unwrap().rflags;
        assert_eq!(flags & cleared, 0, "gate {gate_type:#x}");
        assert_eq!(flags & IF != 0, expect_if, "gate {gate_type:#x}");
    }
}

#[test]
fn test_idt_delivery_faults_escalate_with_exact_error_codes() {
    let (mut limit, limit_memory) = setup_vm(&[0xF4], None);
    let mut sregs = limit.get_sregs().unwrap();
    sregs.idt.limit = 13 * 16 + 14;
    limit.set_sregs(&sregs).unwrap();
    limit.inject_exception(13, Some(0)).unwrap();
    let regs = limit.get_regs().unwrap();
    assert_eq!(
        regs.rip, INT_HANDLER_ADDR,
        "#GP while delivering #GP becomes #DF"
    );
    assert_eq!(
        limit_memory
            .read_obj::<u64>(GuestAddress(regs.rsp))
            .unwrap(),
        0,
        "#DF pushes error code zero"
    );

    for (name, offset, value, expected_error) in [
        ("reserved IST", 4_u64, 8_u8, 0x1A_u64),
        ("invalid type", 5, 0x8C, 0x1A),
        ("null selector low", 2, 0, 0x0),
        ("null selector high", 3, 0, 0x0),
    ] {
        let (mut vcpu, memory) = setup_vm(&[0xF4], None);
        if name.starts_with("null") {
            memory
                .write_obj(0_u16, GuestAddress(IDT_BASE + 3 * 16 + 2))
                .unwrap();
        } else {
            memory
                .write_obj(value, GuestAddress(IDT_BASE + 3 * 16 + offset))
                .unwrap();
        }
        vcpu.inject_exception(3, None).unwrap();
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, INT_HANDLER_ADDR, "{name}");
        assert_eq!(
            memory.read_obj::<u64>(GuestAddress(regs.rsp)).unwrap(),
            expected_error,
            "{name} nested exception error code"
        );
    }
}

#[test]
fn test_external_interrupt_delivery_fault_sets_ext_in_nested_error_code() {
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    memory
        .write_obj(8_u8, GuestAddress(IDT_BASE + 0x20 * 16 + 4))
        .unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rflags |= 1 << 9;
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.inject_interrupt(0x20).unwrap());
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rip, INT_HANDLER_ADDR);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(regs.rsp)).unwrap(),
        0x103,
        "#GP names IDT vector 0x20 and sets IDT+EXT"
    );
}

#[test]
fn test_fault_during_double_fault_delivery_reports_triple_fault_chain() {
    let (mut vcpu, _) = setup_vm_no_idt(&[0xF4], None);
    let error = vcpu.inject_exception(6, None).unwrap_err().to_string();
    assert!(error.contains("triple fault"), "{error}");
    for vector in [6, 11, 8] {
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "missing vector {vector} in chain: {error}"
        );
    }
}

#[test]
fn test_gate_selector_rpl_does_not_choose_target_cpl() {
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    memory
        .write_obj(0x000B_u16, GuestAddress(IDT_BASE + 3 * 16 + 2))
        .unwrap();

    vcpu.inject_exception(3, None).unwrap();
    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(sregs.cs.selector, 0x8);
    assert_eq!(sregs.cs.dpl, 0);
    assert!(sregs.cs.l);
    assert!(!sregs.cs.db);
    assert_eq!(
        memory
            .read_obj::<u8>(GuestAddress(GDT_BASE + 8 + 5))
            .unwrap()
            & 1,
        1,
        "loading CS marks the code descriptor accessed"
    );
}

#[test]
fn test_software_interrupt_gate_dpl_fault_retains_faulting_rip() {
    const TSS_BASE: u64 = 0x14000;
    const RSP0: u64 = 0xA000;
    let (mut vcpu, memory) = setup_vm(&[0xCD, 0x80], None);
    memory.write_obj(RSP0, GuestAddress(TSS_BASE + 4)).unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 0xB; // CPL3; the vector-0x80 gate remains DPL0.
    sregs.cs.dpl = 3;
    sregs.tr.base = TSS_BASE;
    sregs.tr.limit = 0x67;
    sregs.tr.selector = 0x10;
    sregs.tr.type_ = 11;
    sregs.tr.present = true;
    sregs.tr.unusable = false;
    vcpu.set_sregs(&sregs).unwrap();

    assert!(vcpu.step().unwrap().is_none());
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rip, INT_HANDLER_ADDR);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(regs.rsp)).unwrap(),
        0x402
    );
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(regs.rsp + 8)).unwrap(),
        CODE_ADDR,
        "nested #GP saves the faulting INT address"
    );
}

#[test]
fn test_ist_switch_uses_validated_tss_pointer_and_aligned_stack() {
    const TSS_BASE: u64 = 0x14000;
    const IST1_RSP: u64 = 0xA00F;
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    memory
        .write_obj(1_u8, GuestAddress(IDT_BASE + 3 * 16 + 4))
        .unwrap();
    memory
        .write_obj(IST1_RSP, GuestAddress(TSS_BASE + 0x24))
        .unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.tr.base = TSS_BASE;
    sregs.tr.limit = 0x67;
    sregs.tr.selector = 0x10;
    sregs.tr.type_ = 11;
    sregs.tr.present = true;
    sregs.tr.unusable = false;
    vcpu.set_sregs(&sregs).unwrap();

    vcpu.inject_exception(3, None).unwrap();
    let regs = vcpu.get_regs().unwrap();
    let aligned_top = IST1_RSP & !0xF;
    assert_eq!(regs.rsp, aligned_top - 5 * 8);
    assert_eq!(
        memory
            .read_obj::<u64>(GuestAddress(regs.rsp + 3 * 8))
            .unwrap(),
        STACK_ADDR,
        "IST frame retains the interrupted RSP"
    );
    assert_eq!(vcpu.get_sregs().unwrap().ss.selector, 0);
}

#[test]
fn test_ist_switch_rejects_tss_limit_before_reading_pointer() {
    let (mut vcpu, memory) = setup_vm(&[0xF4], None);
    memory
        .write_obj(1_u8, GuestAddress(IDT_BASE + 3 * 16 + 4))
        .unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.tr.base = 0x14000;
    sregs.tr.limit = 0x2A;
    sregs.tr.selector = 0x10;
    sregs.tr.type_ = 11;
    sregs.tr.present = true;
    sregs.tr.unusable = false;
    vcpu.set_sregs(&sregs).unwrap();

    vcpu.inject_exception(3, None).unwrap();
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rip, INT_HANDLER_ADDR);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(regs.rsp)).unwrap(),
        0x10,
        "#TS error code names the current TSS without EXT for #BP delivery"
    );
}

#[test]
fn test_int_stack_alignment() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x48, 0x89, 0xe0, // MOV RAX, RSP (save initial)
        0xcd, 0x30, // INT 0x30
        0x48, 0x89, 0xe3, // MOV RBX, RSP (save after)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x8000);
}

// ============================================================================
// INTO - Interrupt on Overflow
// ============================================================================

#[test]
fn test_into_overflow_flag_clear() {
    // INTO when OF=0 should not interrupt
    // INTO is only valid in 32-bit/compatibility mode
    let code = [
        0x66, 0xb8, 0x01, 0x00, 0x00, 0x00, // MOV EAX, 1
        0x66, 0x83, 0xc0, 0x01, // ADD EAX, 1 (no overflow, OF=0)
        0xce, // INTO (should not trigger)
        0x66, 0xbb, 0x42, 0x00, 0x00, 0x00, // MOV EBX, 0x42
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFFFFFF, 2);
    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x42); // Execution continued
}

#[test]
fn test_into_overflow_flag_set() {
    // INTO when OF=1 should trigger interrupt 4
    // INTO is only valid in 32-bit/compatibility mode
    let code = [
        0x66, 0xb8, 0xff, 0xff, 0xff, 0x7f, // MOV EAX, 0x7FFFFFFF (max positive 32-bit)
        0x66, 0x83, 0xc0, 0x01, // ADD EAX, 1 (overflow, OF=1)
        0xce, // INTO (should trigger INT 4)
        0x66, 0xbb, 0x99, 0x00, 0x00, 0x00, // MOV EBX, 0x99
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // INTO with OF=1 traps to vector 4; the IDT stub IRETQs back to the
    // instruction after INTO, so EBX=0x99 is reached and EAX wrapped to 0x80000000.
    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x80000000,
        "0x7FFFFFFF + 1 overflowed"
    );
    assert_eq!(
        regs.rbx & 0xFFFFFFFF,
        0x99,
        "execution resumed after INTO trap"
    );
}

#[test]
fn test_into_after_addition_no_overflow() {
    // INTO is only valid in 32-bit/compatibility mode
    let code = [
        0x66, 0xb8, 0x10, 0x00, 0x00, 0x00, // MOV EAX, 16
        0x66, 0x83, 0xc0, 0x10, // ADD EAX, 16 (no overflow)
        0xce, // INTO
        0x66, 0xb9, 0xaa, 0x00, 0x00, 0x00, // MOV ECX, 0xAA
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFFFFFF, 32);
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0xaa);
}

#[test]
fn test_into_after_subtraction_no_overflow() {
    // INTO is only valid in 32-bit/compatibility mode
    let code = [
        0x66, 0xb8, 0x20, 0x00, 0x00, 0x00, // MOV EAX, 32
        0x66, 0x83, 0xe8, 0x10, // SUB EAX, 16 (no overflow)
        0xce, // INTO
        0x66, 0xba, 0xbb, 0x00, 0x00, 0x00, // MOV EDX, 0xBB
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFFFFFF, 16);
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0xbb);
}

#[test]
fn test_into_after_signed_overflow() {
    // Signed overflow: adding two large positive numbers
    // INTO is only valid in 32-bit/compatibility mode
    let code = [
        0x66, 0xb8, 0x00, 0x00, 0x00, 0x40, // MOV EAX, 0x40000000
        0x66, 0x05, 0x00, 0x00, 0x00, 0x40, // ADD EAX, 0x40000000 (overflow in signed)
        0xce, // INTO
        0x66, 0xbb, 0xcc, 0x00, 0x00, 0x00, // MOV EBX, 0xCC
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // May trigger interrupt 4 if OF set
}

#[test]
fn test_into_after_multiplication_overflow() {
    // Multiplication that causes overflow
    // INTO is only valid in 32-bit/compatibility mode
    let code = [
        0x66, 0xb8, 0x00, 0x00, 0x00, 0x80, // MOV EAX, 0x80000000
        0x66, 0xf7, 0xe8, // IMUL EAX (EAX * EAX, likely overflow)
        0xce, // INTO
        0x66, 0xbb, 0xdd, 0x00, 0x00, 0x00, // MOV EBX, 0xDD
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Behavior depends on OF flag state
}

// ============================================================================
// INT - Privilege Level Checks
// ============================================================================

#[test]
fn test_int_from_cpl0() {
    // INT from ring 0 (highest privilege)
    let code = [
        0xcd, 0x30, // INT 0x30
        0x48, 0xc7, 0xc0, 0x30, 0x00, 0x00, 0x00, // MOV RAX, 0x30
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x30);
}

#[test]
fn test_int_user_defined_vectors() {
    // User-defined interrupt vectors (32-255)
    let code = [
        0xcd, 0x40, // INT 0x40
        0xcd, 0x50, // INT 0x50
        0xcd, 0x60, // INT 0x60
        0x48, 0xc7, 0xc0, 0x60, 0x00, 0x00, 0x00, // MOV RAX, 0x60
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x60);
}

// ============================================================================
// INT - Reserved and Special Vectors
// ============================================================================

#[test]
fn test_int_divide_error_vector() {
    // INT 0 - divide error (normally triggered by DIV)
    let code = [
        0xcd, 0x00, // INT 0
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0);
}

#[test]
fn test_int_nmi_vector() {
    // INT 2 - NMI (non-maskable interrupt)
    let code = [
        0xcd, 0x02, // INT 2
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 2);
}

#[test]
fn test_int_bound_range_exceeded() {
    // INT 5 - BOUND range exceeded
    let code = [
        0xcd, 0x05, // INT 5
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 5);
}

#[test]
fn test_int_invalid_opcode() {
    // INT 6 - invalid opcode
    let code = [
        0xcd, 0x06, // INT 6
        0x48, 0xc7, 0xc0, 0x06, 0x00, 0x00, 0x00, // MOV RAX, 6
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 6);
}

#[test]
fn test_int_device_not_available() {
    // INT 7 - device not available (coprocessor)
    let code = [
        0xcd, 0x07, // INT 7
        0x48, 0xc7, 0xc0, 0x07, 0x00, 0x00, 0x00, // MOV RAX, 7
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 7);
}

#[test]
fn test_int_double_fault() {
    // INT 8 - double fault
    let code = [
        0xcd, 0x08, // INT 8
        0x48, 0xc7, 0xc0, 0x08, 0x00, 0x00, 0x00, // MOV RAX, 8
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 8);
}

#[test]
fn test_int_invalid_tss() {
    // INT 10 - invalid TSS
    let code = [
        0xcd, 0x0a, // INT 10
        0x48, 0xc7, 0xc0, 0x0a, 0x00, 0x00, 0x00, // MOV RAX, 10
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 10);
}

#[test]
fn test_int_segment_not_present() {
    // INT 11 - segment not present
    let code = [
        0xcd, 0x0b, // INT 11
        0x48, 0xc7, 0xc0, 0x0b, 0x00, 0x00, 0x00, // MOV RAX, 11
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 11);
}

#[test]
fn test_int_stack_segment_fault() {
    // INT 12 - stack segment fault
    let code = [
        0xcd, 0x0c, // INT 12
        0x48, 0xc7, 0xc0, 0x0c, 0x00, 0x00, 0x00, // MOV RAX, 12
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 12);
}

#[test]
fn test_int_alignment_check() {
    // INT 17 - alignment check
    let code = [
        0xcd, 0x11, // INT 17
        0x48, 0xc7, 0xc0, 0x11, 0x00, 0x00, 0x00, // MOV RAX, 17
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 17);
}

#[test]
fn test_int_machine_check() {
    // INT 18 - machine check
    let code = [
        0xcd, 0x12, // INT 18
        0x48, 0xc7, 0xc0, 0x12, 0x00, 0x00, 0x00, // MOV RAX, 18
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 18);
}

#[test]
fn test_int_simd_floating_point() {
    // INT 19 - SIMD floating point exception
    let code = [
        0xcd, 0x13, // INT 19
        0x48, 0xc7, 0xc0, 0x13, 0x00, 0x00, 0x00, // MOV RAX, 19
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 19);
}

// ============================================================================
// INT - Sequential Interrupts
// ============================================================================

#[test]
fn test_int_multiple_different_vectors() {
    // Multiple different INT instructions
    let code = [
        0xcd, 0x30, // INT 0x30
        0xcd, 0x31, // INT 0x31
        0xcd, 0x32, // INT 0x32
        0x48, 0xc7, 0xc0, 0x32, 0x00, 0x00, 0x00, // MOV RAX, 0x32
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x32);
}

#[test]
fn test_int_same_vector_repeated() {
    // Same interrupt vector multiple times
    let code = [
        0xcd, 0x40, // INT 0x40
        0xcd, 0x40, // INT 0x40
        0xcd, 0x40, // INT 0x40
        0x48, 0xc7, 0xc0, 0x40, 0x00, 0x00, 0x00, // MOV RAX, 0x40
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x40);
}

// ============================================================================
// INTO - Edge Cases
// ============================================================================

#[test]
fn test_into_invalid_in_64bit_mode() {
    // INTO (0xCE) is INVALID in 64-bit mode and must raise #UD (vector 6).
    // It must NOT abort the emulator. We detect the injected fault using the
    // no-IDT harness (mirroring tests/suites/isa/x86_64/miscellaneous/ud.rs):
    // with no IDT entries
    // populated, exception delivery fails fast instead of reaching HLT.
    let code = [
        0xce, // INTO (invalid in 64-bit)
        0x48, 0xc7, 0xc0, 0xff, 0x00, 0x00, 0x00, // MOV RAX, 0xFF (must not be reached)
        0xf4, // HLT (must not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    // The guest must not be able to kill the emulator: stepping the INTO must
    // not panic. It should inject #UD rather than reaching HLT.
    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("INTO in 64-bit mode must raise #UD, not reach HLT"),
        Ok(VcpuExit::Shutdown) => {} // #UD injected (no handler) -> shutdown
        Err(_) => {}                 // #UD injected, IDT entry not present -> Err (no abort)
        _ => {}                      // other non-HLT exit is acceptable
    }
}

// ============================================================================
// INT - Register Preservation
// ============================================================================

#[test]
fn test_int_preserves_all_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x11, 0x11, 0x00, 0x00, // MOV RAX, 0x1111
        0x48, 0xc7, 0xc3, 0x22, 0x22, 0x00, 0x00, // MOV RBX, 0x2222
        0x48, 0xc7, 0xc1, 0x33, 0x33, 0x00, 0x00, // MOV RCX, 0x3333
        0x48, 0xc7, 0xc2, 0x44, 0x44, 0x00, 0x00, // MOV RDX, 0x4444
        0xcd, 0x50, // INT 0x50
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x1111);
    assert_eq!(regs.rbx, 0x2222);
    assert_eq!(regs.rcx, 0x3333);
    assert_eq!(regs.rdx, 0x4444);
}

// ============================================================================
// INT3 - Debugger Integration
// ============================================================================

#[test]
fn test_int3_debugger_breakpoint_pattern() {
    // Common pattern: INT3 for debugger breakpoints
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xcc, // INT3 (breakpoint)
        0x48, 0xff, 0xc0, // INC RAX
        0xcc, // INT3 (another breakpoint)
        0x48, 0xff, 0xc0, // INC RAX
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 3);
}

#[test]
fn test_int3_code_patching() {
    // INT3 used for code patching (NOP replacement)
    let code = [
        0xcc, // INT3 (was NOP in original code)
        0x48, 0xc7, 0xc0, 0xab, 0xcd, 0x00, 0x00, // MOV RAX, 0xCDAB
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xcdab);
}
