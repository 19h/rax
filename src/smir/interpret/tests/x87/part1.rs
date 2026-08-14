//! x87 part 1 tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_emms_empties_tags_and_preserves_aliased_payloads_and_x87_state() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    let flags_before = 0xCD7;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let mm_before = [
        0x0123_4567_89AB_CDEF,
        0x1111_2222_3333_4444,
        0x5555_6666_7777_8888,
        0x9999_AAAA_BBBB_CCCC,
        0xDEAD_BEEF_CAFE_BABE,
        0x0F0E_0D0C_0B0A_0908,
        0x8877_6655_4433_2211,
        u64::MAX,
    ];
    let x87_before = if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm = mm_before;
        x86.x87.control_word = 0x027F;
        x86.x87.status_word = 5 << 11 | 0x45;
        x86.x87.tag_word = 0;
        x86.x87.data_ptr = 0x1122_3344_5566_7788;
        x86.x87.instr_ptr = 0x8877_6655_4433_2211;
        x86.x87.last_opcode = 0x345;
        x86.x87.regs[3] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        x86.x87.clone()
    } else {
        unreachable!()
    };

    assert!(matches!(
        execute_lifted_x86(&[0x0F, 0x77], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.mm, mm_before);
        let mut expected = x87_before;
        expected.tag_word = 0xFFFF;
        assert_eq!(x86.x87, expected);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_femms_empties_tags_without_modifying_other_defined_state() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    let flags_before = 0x8D7;
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    let x87_before = if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.mm = [
            0x0123_4567_89AB_CDEF,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
            0x9999_AAAA_BBBB_CCCC,
            0xDEAD_BEEF_CAFE_BABE,
            0x0F0E_0D0C_0B0A_0908,
            0x8877_6655_4433_2211,
            u64::MAX,
        ];
        x86.x87.control_word = 0x027F;
        x86.x87.status_word = 3 << 11 | 0x41;
        x86.x87.tag_word = 0;
        x86.x87.data_ptr = 0x0123_4567_89AB_CDEF;
        x86.x87.instr_ptr = 0xFEDC_BA98_7654_3210;
        x86.x87.last_opcode = 0x456;
        x86.x87.clone()
    } else {
        unreachable!()
    };

    assert!(matches!(
        execute_lifted_x86(&[0x0F, 0x0E], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = x87_before;
        expected.tag_word = 0xFFFF;
        assert_eq!(x86.x87, expected);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_x87_environment_control_state_memory_and_fault_atomicity() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let raw_reg = [0xA5, 0x5A, 1, 2, 3, 4, 5, 6, 0x34, 0xC0];
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x300);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.status_word = 0xFFFF;
        x86.x87.regs[3] = raw_reg;
    }
    execute_lifted_x86(&[0xDB, 0xE2], &mut ctx, &mut memory); // FNCLEX
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.status_word, 0x7F00);
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.regs[3], raw_reg);
    }

    ctx.write_vreg(rax, 0x1122_3344_5566_7788);
    execute_lifted_x86(&[0xDF, 0xE0], &mut ctx, &mut memory); // FNSTSW AX
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_7F00);

    ctx.write_vreg(rbx, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.control_word = 0x0B7F;
        x86.x87.status_word = 0x5A5A;
    }
    execute_lifted_x86(&[0xD9, 0x3B], &mut ctx, &mut memory); // FNSTCW [RBX]
    execute_lifted_x86(&[0xDD, 0x7B, 0x02], &mut ctx, &mut memory); // FNSTSW [RBX+2]
    let mut word = [0u8; 2];
    memory.read(0x100, &mut word).unwrap();
    assert_eq!(u16::from_le_bytes(word), 0x0B7F);
    memory.read(0x102, &mut word).unwrap();
    assert_eq!(u16::from_le_bytes(word), 0x5A5A);

    memory.write(0x104, &0x077Fu16.to_le_bytes()).unwrap();
    execute_lifted_x86(&[0xD9, 0x6B, 0x04], &mut ctx, &mut memory); // FLDCW [RBX+4]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.control_word, 0x077F);
    }

    // addr32 discards RBX[63:32] before applying the displacement.
    memory.write(0x120, &0x037Fu16.to_le_bytes()).unwrap();
    ctx.write_vreg(rbx, 0xDEAD_BEEF_0000_0100);
    execute_lifted_x86(&[0x67, 0xD9, 0x6B, 0x20], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.control_word, 0x037F);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.control_word = 0x0040;
        x86.x87.status_word = 0xFFFF;
        x86.x87.tag_word = 0;
        x86.x87.data_ptr = 0x1111_2222_3333_4444;
        x86.x87.instr_ptr = 0x5555_6666_7777_8888;
        x86.x87.last_opcode = 0x07FF;
    }
    execute_lifted_x86(&[0xDB, 0xE3], &mut ctx, &mut memory); // FNINIT
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.control_word, 0x037F);
        assert_eq!(x86.x87.status_word, 0);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
        assert_eq!(x86.x87.data_ptr, 0);
        assert_eq!(x86.x87.instr_ptr, 0);
        assert_eq!(x86.x87.last_opcode, 0);
        assert_eq!(x86.x87.regs[3], raw_reg, "FNINIT changed data register");
    }

    // FLDCW reads before committing; a two-byte boundary fault preserves FCW.
    let mut short_memory = FlatMemory::new(0x101);
    ctx.write_vreg(rbx, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.control_word = 0x0F7F;
    }
    let exit = execute_lifted_x86(&[0xD9, 0x2B], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.control_word, 0x0F7F);
    }

    let exit = execute_lifted_x86(&[0xD9, 0x3B], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_x87_legacy_environment_save_restore_side_effects_and_fault_atomicity() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.write_vreg(rax, 0x100);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.control_word = 0x0C40;
        x86.x87.status_word = 0x5A81;
        x86.x87.tag_word = 0x39E4;
        x86.x87.instr_ptr = 0x1122_3344_5566_7788;
        x86.x87.data_ptr = 0x99AA_BBCC_DDEE_FF00;
        x86.x87.last_opcode = 0x0765;
        for physical in 0..8 {
            x86.x87.regs[physical] = raw(
                0x8000_0000_0000_0000 | physical as u64,
                0x3FFF + physical as u16,
            );
        }
    }
    let original = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.x87.clone(),
        _ => unreachable!(),
    };

    execute_lifted_x86(&[0xD9, 0x30], &mut ctx, &mut memory); // FNSTENV m28byte
    let (expected_env32, _) =
        SmirInterpreter::x86_x87_environment_image(&original, X86X87EnvWidth::W32);
    let mut env32 = [0u8; 28];
    memory.read(0x100, &mut env32).unwrap();
    assert_eq!(env32, expected_env32);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = original.clone();
        expected.control_word |= 0x003F;
        assert_eq!(x86.x87, expected, "FNSTENV post-store masks");
    }

    // 66H selects the compact protected-mode image and does not overwrite
    // bytes beyond the architectural 14-byte destination.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = original.clone();
    }
    memory.write(0x100, &[0xA5; 32]).unwrap();
    execute_lifted_x86(&[0x66, 0xD9, 0x30], &mut ctx, &mut memory);
    let (expected_env16, _) =
        SmirInterpreter::x86_x87_environment_image(&original, X86X87EnvWidth::W16);
    let mut compact = [0u8; 32];
    memory.read(0x100, &mut compact).unwrap();
    assert_eq!(&compact[..14], &expected_env16[..14]);
    assert_eq!(&compact[14..], &[0xA5; 18]);

    // FLDENV loads only the environment, zero-extends legacy pointer
    // offsets, and preserves all physical register payloads.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        for physical in 0..8 {
            x86.x87.regs[physical] = raw(physical as u64 + 1, 0x4000);
        }
    }
    let regs_before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.x87.regs,
        _ => unreachable!(),
    };
    memory.write(0x100, &expected_env32).unwrap();
    execute_lifted_x86(&[0xD9, 0x20], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.control_word, original.control_word);
        assert_eq!(x86.x87.status_word, original.status_word);
        assert_eq!(x86.x87.tag_word, original.tag_word);
        assert_eq!(x86.x87.instr_ptr, 0x5566_7788);
        assert_eq!(x86.x87.data_ptr, 0xDDEE_FF00);
        assert_eq!(x86.x87.last_opcode, original.last_opcode);
        assert_eq!(x86.x87.regs, regs_before);
    }

    // FNSAVE writes the complete logical-register image, then performs the
    // FINIT environment reset without clearing raw register payloads.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = original.clone();
    }
    execute_lifted_x86(&[0xDD, 0x30], &mut ctx, &mut memory); // FNSAVE m108byte
    let (expected_save, _) = SmirInterpreter::x86_x87_state_image(&original, X86X87EnvWidth::W32);
    let mut saved = [0u8; 108];
    memory.read(0x100, &mut saved).unwrap();
    assert_eq!(saved, expected_save);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.control_word, 0x037F);
        assert_eq!(x86.x87.status_word, 0);
        assert_eq!(x86.x87.tag_word, 0xFFFF);
        assert_eq!(x86.x87.instr_ptr, 0);
        assert_eq!(x86.x87.data_ptr, 0);
        assert_eq!(x86.x87.last_opcode, 0);
        assert_eq!(x86.x87.regs, original.regs);
    }

    execute_lifted_x86(&[0xDD, 0x20], &mut ctx, &mut memory); // FRSTOR m108byte
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = original.clone();
        expected.instr_ptr = original.instr_ptr as u32 as u64;
        expected.data_ptr = original.data_ptr as u32 as u64;
        assert_eq!(x86.x87, expected);
    }

    // m94byte save/restore uses the compact environment and retains FOP on
    // restore because that protected-mode layout contains no opcode field.
    let mut compact_source = original.clone();
    compact_source.instr_ptr = 0x7788;
    compact_source.data_ptr = 0xFF00;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = compact_source.clone();
    }
    memory.write(0x100, &[0xA5; 108]).unwrap();
    execute_lifted_x86(&[0x66, 0xDD, 0x30], &mut ctx, &mut memory);
    let (expected_save16, _) =
        SmirInterpreter::x86_x87_state_image(&compact_source, X86X87EnvWidth::W16);
    let mut saved16 = [0u8; 108];
    memory.read(0x100, &mut saved16).unwrap();
    assert_eq!(&saved16[..94], &expected_save16[..94]);
    assert_eq!(&saved16[94..], &[0xA5; 14]);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.last_opcode = 0x0321;
    }
    execute_lifted_x86(&[0x66, 0xDD, 0x20], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = compact_source;
        expected.last_opcode = 0x0321;
        assert_eq!(x86.x87, expected);
    }

    // Faults on every legacy load/store form leave architectural x87
    // state unchanged; save faults therefore do not initialize the FPU.
    for (name, bytes, memory_len, write) in [
        ("FNSTENV", &[0xD9, 0x30][..], 0x110usize, true),
        ("FLDENV", &[0xD9, 0x20][..], 0x110, false),
        ("FNSAVE", &[0xDD, 0x30][..], 0x160, true),
        ("FRSTOR", &[0xDD, 0x20][..], 0x160, false),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87 = original.clone();
        }
        let before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.x87.clone(),
            _ => unreachable!(),
        };
        let mut short_memory = FlatMemory::new(memory_len);
        let exit = execute_lifted_x86(bytes, &mut ctx, &mut short_memory);
        assert!(
            matches!(exit, BlockResult::Exit(ExitReason::MemoryFault { write: got, .. }) if got == write),
            "{name}: {exit:?}"
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87, before, "{name}");
        }
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn lifted_x87_exact_register_and_m80_transfers_preserve_payload_tags_and_environment() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let raw_a = raw(0x8000_0000_0000_0000, 0x3FFF); // +1.0, valid
    let raw_b = raw(0, 0x8000); // -0.0, zero
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    memory.write(0x100, &raw_a).unwrap();
    memory.write(0x110, &raw_b).unwrap();

    ctx.write_vreg(rax, 0x100);
    execute_lifted_x86(&[0xDB, 0x28], &mut ctx, &mut memory); // FLD m80fp [RAX]
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.regs[7], raw_a);
        assert_eq!(x86.x87.physical_tag(7), 0);
        assert_eq!(x86.x87.instr_ptr, 0x1000);
        assert_eq!(x86.x87.data_ptr, 0x100);
        assert_eq!(x86.x87.last_opcode, 0x0328);
    }

    ctx.write_vreg(rax, 0x110);
    execute_lifted_x86(&[0xDB, 0x28], &mut ctx, &mut memory);
    execute_lifted_x86(&[0xD9, 0xC9], &mut ctx, &mut memory); // FXCH ST(1)
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 6);
        assert_eq!(x86.x87.regs[6], raw_a);
        assert_eq!(x86.x87.physical_tag(6), 0);
        assert_eq!(x86.x87.regs[7], raw_b);
        assert_eq!(x86.x87.physical_tag(7), 1);
        assert_eq!(x86.x87.last_opcode, 0x01C9);
        assert_eq!(x86.x87.data_ptr, 0x110, "register op changed FDP");
    }

    execute_lifted_x86(&[0xDD, 0xD2], &mut ctx, &mut memory); // FST ST(2)
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.regs[0], raw_a);
        assert_eq!(x86.x87.physical_tag(0), 0);
    }
    execute_lifted_x86(&[0xDD, 0xD9], &mut ctx, &mut memory); // FSTP ST(1)
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.physical_tag(6), 3);
        assert_eq!(x86.x87.regs[7], raw_a);
        assert_eq!(x86.x87.physical_tag(7), 0);
    }

    ctx.write_vreg(rax, 0x180);
    execute_lifted_x86(&[0xDB, 0x38], &mut ctx, &mut memory); // FSTP m80fp [RAX]
    let mut stored = [0u8; 10];
    memory.read(0x180, &mut stored).unwrap();
    assert_eq!(stored, raw_a);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 0);
        assert_eq!(x86.x87.physical_tag(7), 3);
        assert_eq!(x86.x87.data_ptr, 0x180);
        assert_eq!(x86.x87.last_opcode, 0x0338);
    }

    // FLD ST(i) copies the raw payload and full tag before decrementing TOP.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.set_logical_raw_tagged(3, raw_b, 1);
    }
    execute_lifted_x86(&[0xD9, 0xC3], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.regs[7], raw_b);
        assert_eq!(x86.x87.physical_tag(7), 1);
    }
    execute_lifted_x86(&[0xDD, 0xC0], &mut ctx, &mut memory); // FFREE ST(0)
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.physical_tag(7), 3);
        assert_eq!(x86.x87.regs[7], raw_b, "FFREE changed payload");
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}

#[test]
fn lifted_legacy_x87_register_forms_match_canonical_state_and_free_pop_exactly() {
    fn raw(logical: u8) -> [u8; 10] {
        let significand = 0x8000_0000_0000_0000 | ((u64::from(logical) + 1) << 48);
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&0x3FFFu16.to_le_bytes());
        value
    }

    fn seeded_context(top: u8) -> SmirContext {
        let mut ctx = SmirContext::new_x86_64();
        ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.control_word = 0x037F;
            x86.x87.status_word = 0x4745;
            x86.x87.set_top(top);
            x86.x87.tag_word = 0xFFFF;
            x86.x87.data_ptr = 0x1122_3344_5566_7788;
            for logical in 0u8..8 {
                x86.x87.set_logical_raw_tagged(logical, raw(logical), 0);
            }
        }
        ctx
    }

    for (name, canonical, alias, alias_fop) in [
        (
            "DD C8-CF FXCH alias",
            &[0xD9, 0xCB][..],
            &[0xDD, 0xCB][..],
            0x05CB,
        ),
        (
            "DC D0-D7 FCOM alias",
            &[0xD8, 0xD3][..],
            &[0xDC, 0xD3][..],
            0x04D3,
        ),
        (
            "DC D8-DF FCOMP alias",
            &[0xD8, 0xDB][..],
            &[0xDC, 0xDB][..],
            0x04DB,
        ),
        (
            "DE D0-D7 FCOMP alias",
            &[0xD8, 0xDB][..],
            &[0xDE, 0xD3][..],
            0x06D3,
        ),
        (
            "DF D0-D7 FSTP alias",
            &[0xDD, 0xDB][..],
            &[0xDF, 0xD3][..],
            0x07D3,
        ),
    ] {
        let mut canonical_ctx = seeded_context(5);
        let mut alias_ctx = seeded_context(5);
        let mut canonical_memory = FlatMemory::new(0x100);
        let mut alias_memory = FlatMemory::new(0x100);
        execute_lifted_x86(canonical, &mut canonical_ctx, &mut canonical_memory);
        execute_lifted_x86(alias, &mut alias_ctx, &mut alias_memory);

        let mut canonical_state = match &canonical_ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.x87.clone(),
            _ => unreachable!(),
        };
        let alias_state = match &alias_ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.x87.clone(),
            _ => unreachable!(),
        };
        assert_eq!(alias_state.last_opcode, alias_fop, "{name}");
        canonical_state.last_opcode = alias_fop;
        assert_eq!(alias_state, canonical_state, "{name}");

        canonical_ctx.flags.materialize_all();
        alias_ctx.flags.materialize_all();
        assert_eq!(
            alias_ctx.flags.materialized.to_rflags(),
            canonical_ctx.flags.materialized.to_rflags(),
            "{name}"
        );
    }

    for (top, st) in [(0, 0), (5, 3), (7, 7)] {
        let mut ctx = seeded_context(top);
        let mut memory = FlatMemory::new(0x100);
        let before = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.x87.clone(),
            _ => unreachable!(),
        };
        execute_lifted_x86(&[0xDF, 0xC0 + st], &mut ctx, &mut memory);

        let after = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.x87.clone(),
            _ => unreachable!(),
        };
        let mut expected = before.clone();
        expected.set_physical_tag(before.physical_index(st), 3);
        expected.set_physical_tag(before.physical_index(0), 3);
        expected.set_top(top.wrapping_add(1));
        expected.instr_ptr = 0x1000;
        expected.last_opcode = 0x07C0 + u16::from(st);
        assert_eq!(after, expected, "FFREEP ST({st}) with TOP={top}");
        assert_eq!(
            after.status_word & 0x4700,
            before.status_word & 0x4700,
            "FFREEP preserves its undefined C0-C3 deterministically"
        );

        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    }
}

#[test]
fn lifted_x87_exact_sign_and_top_rotation_operations_preserve_raw_state() {
    let mut negative = [0xA5, 0x5A, 1, 2, 3, 4, 5, 0x80, 0x34, 0xC0];
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.set_top(7);
        x86.x87.set_logical_raw_tagged(0, negative, 0);
        x86.x87.status_word |= 0x0200;
    }

    execute_lifted_x86(&[0xD9, 0xE1], &mut ctx, &mut memory); // FABS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        negative[9] &= 0x7F;
        assert_eq!(x86.x87.regs[7], negative);
        assert_eq!(x86.x87.physical_tag(7), 0);
        assert_eq!(x86.x87.status_word & 0x0200, 0);
        assert_eq!(x86.x87.last_opcode, 0x01E1);
    }

    execute_lifted_x86(&[0xD9, 0xE0], &mut ctx, &mut memory); // FCHS
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        negative[9] ^= 0x80;
        assert_eq!(x86.x87.regs[7], negative);
        assert_eq!(x86.x87.last_opcode, 0x01E0);
    }

    let (regs, tags) = match &mut ctx.arch_regs {
        ArchRegState::X86_64(x86) => {
            x86.x87.status_word |= 0x0200;
            (x86.x87.regs, x86.x87.tag_word)
        }
        _ => unreachable!(),
    };
    execute_lifted_x86(&[0xD9, 0xF6], &mut ctx, &mut memory); // FDECSTP
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 6);
        assert_eq!(x86.x87.regs, regs);
        assert_eq!(x86.x87.tag_word, tags);
        assert_eq!(x86.x87.status_word & 0x0200, 0);
        assert_eq!(x86.x87.last_opcode, 0x01F6);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.status_word |= 0x0200;
    }
    execute_lifted_x86(&[0xD9, 0xF7], &mut ctx, &mut memory); // FINCSTP
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.regs, regs);
        assert_eq!(x86.x87.tag_word, tags);
        assert_eq!(x86.x87.status_word & 0x0200, 0);
        assert_eq!(x86.x87.last_opcode, 0x01F7);
    }

    // Masked unary stack underflow installs indefinite; with IM clear the
    // empty tag and payload remain unchanged while ES/B become pending.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        x86.x87.regs[0] = negative;
    }
    execute_lifted_x86(&[0xD9, 0xE0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.regs[0], crate::smir::X86X87State::INDEFINITE);
        assert_eq!(x86.x87.physical_tag(0), 2);
        assert_eq!(x86.x87.status_word & 0x0241, 0x0041);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        x86.x87.control_word &= !1;
        x86.x87.regs[0] = negative;
    }
    execute_lifted_x86(&[0xD9, 0xE1], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.regs[0], negative);
        assert_eq!(x86.x87.physical_tag(0), 3);
        assert_eq!(x86.x87.status_word & 0x80C1, 0x80C1);
    }
}

#[test]
fn lifted_x87_stack_metadata_waiting_faults_exit_without_committing() {
    for instruction in [
        &[0xD9, 0xF6][..],
        &[0xD9, 0xF7][..],
        &[0xDD, 0xC3][..],
        &[0xDF, 0xC3][..],
    ] {
        for (cr0_bits, pending) in [
            (1 << 2, false),
            (1 << 3, false),
            ((1 << 2) | (1 << 3), true),
            (0, true),
        ] {
            let mut ctx = SmirContext::new_x86_64();
            let mut memory = FlatMemory::new(0x100);
            let before = if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.cr0 |= cr0_bits | (1 << 5);
                x86.x87.status_word = (5 << 11) | 0x4700 | 0x003F;
                if pending {
                    x86.x87.status_word |= 0x8080;
                }
                x86.x87.tag_word = 0x6996;
                x86.x87.instr_ptr = 0x1122_3344_5566_7788;
                x86.x87.last_opcode = 0x05A5;
                x86.x87.clone()
            } else {
                unreachable!()
            };

            assert!(matches!(
                execute_lifted_x86(instruction, &mut ctx, &mut memory),
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ));
            let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.x87, before, "{instruction:02X?}, CR0={cr0_bits:#x}");
        }
    }

    let mut legacy = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x100);
    if let ArchRegState::X86_64(x86) = &mut legacy.arch_regs {
        x86.cr0 &= !(1 << 5);
        x86.x87.status_word = (5 << 11) | 0x8080;
        x86.x87.tag_word = 0;
    }
    assert!(matches!(
        execute_lifted_x86(&[0xDD, 0xC3], &mut legacy, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let ArchRegState::X86_64(x86) = &legacy.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.x87.physical_tag(x86.x87.physical_index(3)), 3);
    assert_eq!(x86.x87.instr_ptr, 0x1000);
    assert_eq!(x86.x87.last_opcode, 0x05C3);
}
#[test]
fn lifted_x87_load_constants_match_all_fcw_rounding_modes_exactly() {
    let cases: [(u8, u16, [u64; 4], u16); 7] = [
        (0xE8, 0x3FFF, [0x8000_0000_0000_0000; 4], 0u16),
        (
            0xE9,
            0x4000,
            [
                0xD49A_784B_CD1B_8AFE,
                0xD49A_784B_CD1B_8AFE,
                0xD49A_784B_CD1B_8AFF,
                0xD49A_784B_CD1B_8AFE,
            ],
            0,
        ),
        (
            0xEA,
            0x3FFF,
            [
                0xB8AA_3B29_5C17_F0BC,
                0xB8AA_3B29_5C17_F0BB,
                0xB8AA_3B29_5C17_F0BC,
                0xB8AA_3B29_5C17_F0BB,
            ],
            0,
        ),
        (
            0xEB,
            0x4000,
            [
                0xC90F_DAA2_2168_C235,
                0xC90F_DAA2_2168_C234,
                0xC90F_DAA2_2168_C235,
                0xC90F_DAA2_2168_C234,
            ],
            0,
        ),
        (
            0xEC,
            0x3FFD,
            [
                0x9A20_9A84_FBCF_F799,
                0x9A20_9A84_FBCF_F798,
                0x9A20_9A84_FBCF_F799,
                0x9A20_9A84_FBCF_F798,
            ],
            0,
        ),
        (
            0xED,
            0x3FFE,
            [
                0xB172_17F7_D1CF_79AC,
                0xB172_17F7_D1CF_79AB,
                0xB172_17F7_D1CF_79AC,
                0xB172_17F7_D1CF_79AB,
            ],
            0,
        ),
        (0xEE, 0x0000, [0; 4], 1),
    ];

    for rc in 0..4u16 {
        for (opcode, exponent, significands, expected_tag) in cases {
            let mut ctx = SmirContext::new_x86_64();
            let mut memory = FlatMemory::new(0x10);
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.x87.control_word = (x86.x87.control_word & !0x0C00) | (rc << 10);
                x86.x87.status_word = 0x0220; // C1 and PE initially set
            }
            execute_lifted_x86(&[0xD9, opcode], &mut ctx, &mut memory);
            if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
                let mut expected = [0u8; 10];
                expected[..8].copy_from_slice(&significands[rc as usize].to_le_bytes());
                expected[8..].copy_from_slice(&exponent.to_le_bytes());
                assert_eq!(x86.x87.top(), 7, "opcode={opcode:02X} rc={rc}");
                assert_eq!(x86.x87.regs[7], expected, "opcode={opcode:02X} rc={rc}");
                assert_eq!(
                    x86.x87.physical_tag(7),
                    expected_tag,
                    "opcode={opcode:02X} rc={rc}"
                );
                assert_eq!(x86.x87.status_word & 0x0200, 0, "C1");
                assert_ne!(x86.x87.status_word & 0x0020, 0, "PE must be preserved");
                assert_eq!(x86.x87.last_opcode, 0x0100 | opcode as u16);
            }
        }
    }

    // Constant loads use the same masked-overflow response as FLD.
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x10);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.set_physical_tag(7, 0);
    }
    execute_lifted_x86(&[0xD9, 0xEB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.regs[7], crate::smir::X86X87State::INDEFINITE);
        assert_eq!(x86.x87.physical_tag(7), 2);
        assert_eq!(x86.x87.status_word & 0x0241, 0x0241);
    }
}
#[test]
fn lifted_x87_fld_single_double_widens_exactly_and_reports_source_classes() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let single_cases = [
        (0x3FC0_0000u32, raw(0xC000_0000_0000_0000, 0x3FFF), 0u16),
        (0x8000_0000, raw(0, 0x8000), 1),
        (0x0000_0001, raw(0x8000_0000_0000_0000, 0x3F6A), 0),
        (0x7F80_0000, raw(0x8000_0000_0000_0000, 0x7FFF), 2),
        (0x7FC1_2345, raw(0xC123_4500_0000_0000, 0x7FFF), 2),
        (0x7F81_2345, raw(0xC123_4500_0000_0000, 0x7FFF), 2),
    ];
    for (bits, expected, tag) in single_cases {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.write_vreg(rax, 0x100);
        memory.write(0x100, &bits.to_le_bytes()).unwrap();
        execute_lifted_x86(&[0xD9, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.top(), 7, "f32={bits:08X}");
            assert_eq!(x86.x87.regs[7], expected, "f32={bits:08X}");
            assert_eq!(x86.x87.physical_tag(7), tag, "f32={bits:08X}");
            assert_eq!(x86.x87.last_opcode, 0x0100);
            assert_eq!(x86.x87.data_ptr, 0x100);
            assert_eq!(x86.x87.status_word & 1 != 0, bits == 0x7F81_2345);
            assert_eq!(x86.x87.status_word & 2 != 0, bits == 1);
        }
    }

    let double_cases = [
        (
            0xC004_0000_0000_0000u64,
            raw(0xA000_0000_0000_0000, 0xC000),
            0u16,
        ),
        (0x8000_0000_0000_0000, raw(0, 0x8000), 1),
        (0x0000_0000_0000_0001, raw(0x8000_0000_0000_0000, 0x3BCD), 0),
        (0x7FF0_0000_0000_0000, raw(0x8000_0000_0000_0000, 0x7FFF), 2),
        (0x7FF8_1234_5678_9ABC, raw(0xC091_A2B3_C4D5_E000, 0x7FFF), 2),
    ];
    for (bits, expected, tag) in double_cases {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.write_vreg(rax, 0x100);
        memory.write(0x100, &bits.to_le_bytes()).unwrap();
        execute_lifted_x86(&[0xDD, 0x00], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.top(), 7, "f64={bits:016X}");
            assert_eq!(x86.x87.regs[7], expected, "f64={bits:016X}");
            assert_eq!(x86.x87.physical_tag(7), tag, "f64={bits:016X}");
            assert_eq!(x86.x87.last_opcode, 0x0500);
        }
    }

    // Intel specifies that an unmasked denormal exception on FLD still
    // pushes the exactly widened value while setting the pending summary.
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x200);
    ctx.write_vreg(rax, 0x100);
    memory.write(0x100, &1u32.to_le_bytes()).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.control_word &= !0x0002;
    }
    execute_lifted_x86(&[0xD9, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 7);
        assert_eq!(x86.x87.regs[7], raw(0x8000_0000_0000_0000, 0x3F6A));
        assert_eq!(x86.x87.status_word & 0x8082, 0x8082); // B|ES|DE
    }

    // An unmasked SNaN exception quiets no destination and does not change
    // TOP; the pre-existing physical payload and empty tag remain intact.
    let sentinel = raw(0xDEAD_BEEF_CAFE_BABE, 0x1234);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        x86.x87.control_word &= !0x0001;
        x86.x87.regs[7] = sentinel;
    }
    memory.write(0x100, &0x7F81_2345u32.to_le_bytes()).unwrap();
    execute_lifted_x86(&[0xD9, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 0);
        assert_eq!(x86.x87.regs[7], sentinel);
        assert_eq!(x86.x87.physical_tag(7), 3);
        assert_eq!(x86.x87.status_word & 0x8081, 0x8081); // B|ES|IE
    }

    // Complete-width read faults preserve the entire x87 state.
    let before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.x87.clone(),
        _ => unreachable!(),
    };
    let mut short_memory = FlatMemory::new(0x104);
    let exit = execute_lifted_x86(&[0xDD, 0x00], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87, before);
    }
}
#[test]
fn lifted_x87_fcmov_conditions_copy_exact_state_and_gate_stack_faults() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }

    let destination = raw(0x8000_0000_0000_0000, 0x3FFF);
    let source = raw(0xDEAD_BEEF_CAFE_BABE, 0xC123);
    for (bytes, rflags) in [
        (&[0xDA, 0xC2][..], 0x0001u64), // FCMOVB: CF=1
        (&[0xDA, 0xCA][..], 0x0040),    // FCMOVE: ZF=1
        (&[0xDA, 0xD2][..], 0x0001),    // FCMOVBE: CF=1
        (&[0xDA, 0xDA][..], 0x0004),    // FCMOVU: PF=1
        (&[0xDB, 0xC2][..], 0x0000),    // FCMOVNB: CF=0
        (&[0xDB, 0xCA][..], 0x0000),    // FCMOVNE: ZF=0
        (&[0xDB, 0xD2][..], 0x0000),    // FCMOVNBE: CF=ZF=0
        (&[0xDB, 0xDA][..], 0x0000),    // FCMOVNU: PF=0
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x10);
        ctx.flags.materialized = MaterializedFlags::from_rflags(rflags);
        ctx.flags.lazy = None;
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.set_logical_raw_tagged(0, destination, 0);
            x86.x87.set_logical_raw_tagged(2, source, 2);
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.regs[0], source, "{bytes:02X?}");
            assert_eq!(x86.x87.physical_tag(0), 2, "{bytes:02X?}");
            assert_eq!(x86.x87.status_word & 0x0041, 0, "{bytes:02X?}");
            assert_eq!(x86.x87.instr_ptr, 0x1000);
        }
        ctx.flags.materialize_all();
        assert_eq!(ctx.flags.materialized.to_rflags() & 0x45, rflags & 0x45);
    }

    // A false condition neither reads the empty x87 source nor changes C1,
    // while still recording the executed x87 opcode/environment.
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x10);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0); // CF=0
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.set_logical_raw_tagged(0, destination, 0);
        x86.x87.status_word |= 0x0200;
    }
    execute_lifted_x86(&[0xDA, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.regs[0], destination);
        assert_eq!(x86.x87.physical_tag(2), 3);
        assert_eq!(x86.x87.status_word & 0x0241, 0x0200);
        assert_eq!(x86.x87.last_opcode, 0x02C2);
    }

    // A true condition with an empty source follows the masked #IS
    // response; with IM clear it preserves the destination and asserts ES/B.
    ctx.flags.materialized = MaterializedFlags::from_rflags(1); // CF=1
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xDA, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.regs[0], crate::smir::X86X87State::INDEFINITE);
        assert_eq!(x86.x87.physical_tag(0), 2);
        assert_eq!(x86.x87.status_word & 0x0241, 0x0041);
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        x86.x87.control_word &= !1;
        x86.x87.set_logical_raw_tagged(0, destination, 0);
    }
    execute_lifted_x86(&[0xDA, 0xC2], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.regs[0], destination);
        assert_eq!(x86.x87.physical_tag(0), 0);
        assert_eq!(x86.x87.status_word & 0x80C1, 0x80C1);
    }
}
#[test]
fn lifted_x87_fxam_classifies_all_binary80_classes_and_empty_sign() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }

    for (name, value, tag, expected_codes) in [
        ("unsupported", raw(0, 0x7FFF), 2u16, 0x0000u16),
        ("nan", raw(0xC000_0000_0000_0001, 0x7FFF), 2, 0x0100),
        ("normal", raw(0x8000_0000_0000_0000, 0x3FFF), 0, 0x0400),
        ("infinity", raw(0x8000_0000_0000_0000, 0x7FFF), 2, 0x0500),
        ("zero", raw(0, 0), 1, 0x4000),
        ("empty-negative", raw(0, 0x8000), 3, 0x4300),
        ("denormal", raw(1, 0), 2, 0x4400),
        ("pseudo-denormal", raw(0x8000_0000_0000_0001, 0), 2, 0x4400),
        (
            "negative-normal",
            raw(0x8000_0000_0000_0000, 0xBFFF),
            0,
            0x0600,
        ),
        ("unnormal", raw(0x4000_0000_0000_0000, 0x4000), 2, 0x0000),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x10);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.set_top(5);
            x86.x87.set_logical_raw_tagged(0, value, tag);
            x86.x87.status_word |= 0x47A5;
        }
        execute_lifted_x86(&[0xD9, 0xE5], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.top(), 5, "{name}");
            assert_eq!(x86.x87.regs[5], value, "{name}");
            assert_eq!(x86.x87.physical_tag(5), tag, "{name}");
            assert_eq!(x86.x87.status_word & 0x4700, expected_codes, "{name}");
            assert_eq!(x86.x87.status_word & 0x00A5, 0x00A5, "{name}");
            assert_eq!(x86.x87.last_opcode, 0x01E5);
            assert_eq!(x86.x87.instr_ptr, 0x1000);
        }
    }
}
#[test]
fn lifted_x87_ftst_compares_zero_and_honors_exception_masks() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }

    for (name, value, tag, expected_codes, expected_exceptions) in [
        ("positive", raw(0x8000_0000_0000_0000, 0x3FFF), 0u16, 0, 0),
        ("negative", raw(0x8000_0000_0000_0000, 0xBFFF), 0, 0x0100, 0),
        ("positive-zero", raw(0, 0), 1, 0x4000, 0),
        ("negative-zero", raw(0, 0x8000), 1, 0x4000, 0),
        (
            "positive-infinity",
            raw(0x8000_0000_0000_0000, 0x7FFF),
            2,
            0,
            0,
        ),
        (
            "negative-infinity",
            raw(0x8000_0000_0000_0000, 0xFFFF),
            2,
            0x0100,
            0,
        ),
        ("positive-denormal", raw(1, 0), 2, 0, 0x0002),
        ("negative-denormal", raw(1, 0x8000), 2, 0x0100, 0x0002),
        (
            "qnan",
            raw(0xC000_0000_0000_0001, 0x7FFF),
            2,
            0x4500,
            0x0001,
        ),
        ("unsupported", raw(0, 0x7FFF), 2, 0x4500, 0x0001),
        ("empty", raw(0, 0), 3, 0x4500, 0x0041),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x10);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.set_top(3);
            x86.x87.set_logical_raw_tagged(0, value, tag);
            x86.x87.status_word |= 0x4720;
        }
        execute_lifted_x86(&[0xD9, 0xE4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.top(), 3, "{name}");
            assert_eq!(x86.x87.regs[3], value, "{name}");
            assert_eq!(x86.x87.physical_tag(3), tag, "{name}");
            assert_eq!(x86.x87.status_word & 0x4500, expected_codes, "{name}");
            assert_eq!(x86.x87.status_word & 0x0200, 0, "{name}: C1");
            assert_eq!(
                x86.x87.status_word & 0x0043,
                (0x0020 | expected_exceptions) & 0x0043,
                "{name}"
            );
            assert_eq!(x86.x87.last_opcode, 0x01E4);
        }
    }

    for (name, value, tag, clear_mask, expected_status) in [
        (
            "unmasked-invalid",
            raw(0xC000_0000_0000_0001, 0x7FFF),
            2u16,
            0x0001u16,
            0x8081u16,
        ),
        ("unmasked-denormal", raw(1, 0), 2, 0x0002, 0x8082),
        ("unmasked-empty", raw(0, 0), 3, 0x0001, 0x80C1),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x10);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.control_word &= !clear_mask;
            x86.x87.set_logical_raw_tagged(0, value, tag);
            x86.x87.status_word = 0x0700; // prior C0,C1,C2=1
        }
        execute_lifted_x86(&[0xD9, 0xE4], &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.status_word & 0x4500, 0x0500, "{name}");
            assert_eq!(x86.x87.status_word & 0x0200, 0, "{name}: C1");
            assert_eq!(
                x86.x87.status_word & expected_status,
                expected_status,
                "{name}"
            );
            assert_eq!(x86.x87.regs[0], value, "{name}");
            assert_eq!(x86.x87.physical_tag(0), tag, "{name}");
        }
    }
}
#[test]
fn lifted_x87_fcom_fucom_order_binary80_and_apply_pop_counts() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }
    let p0 = raw(0, 0);
    let n0 = raw(0, 0x8000);
    let p1 = raw(0x8000_0000_0000_0000, 0x3FFF);
    let p2 = raw(0x8000_0000_0000_0000, 0x4000);
    let n1 = raw(0x8000_0000_0000_0000, 0xBFFF);
    let n2 = raw(0x8000_0000_0000_0000, 0xC000);
    let pinf = raw(0x8000_0000_0000_0000, 0x7FFF);

    for (name, lhs, lhs_tag, rhs, rhs_tag, expected_codes) in [
        ("greater", p2, 0u16, p1, 0u16, 0x0000u16),
        ("less", p1, 0, p2, 0, 0x0100),
        ("equal", p1, 0, p1, 0, 0x4000),
        ("negative-order", n2, 0, n1, 0, 0x0100),
        ("negative-v-positive", n1, 0, p1, 0, 0x0100),
        ("positive-v-negative", p1, 0, n1, 0, 0x0000),
        ("signed-zero", n0, 1, p0, 1, 0x4000),
        ("infinity", pinf, 2, p2, 0, 0x0000),
        (
            "pseudo-denormal-equals-normal",
            raw(0x8000_0000_0000_0042, 0),
            2,
            raw(0x8000_0000_0000_0042, 1),
            0,
            0x4000,
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x10);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87.set_logical_raw_tagged(0, lhs, lhs_tag);
            x86.x87.set_logical_raw_tagged(1, rhs, rhs_tag);
            x86.x87.status_word |= 0x47A0;
        }
        execute_lifted_x86(&[0xD8, 0xD1], &mut ctx, &mut memory); // FCOM ST(1)
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.status_word & 0x4500, expected_codes, "{name}");
            assert_eq!(x86.x87.status_word & 0x0200, 0, "{name}: C1");
            assert_eq!(x86.x87.top(), 0, "{name}");
            assert_eq!(x86.x87.regs[0], lhs, "{name}");
            assert_eq!(x86.x87.regs[1], rhs, "{name}");
        }
    }

    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x300);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.set_logical_raw(0, p2);
        x86.x87.set_logical_raw(1, p1);
        x86.x87.set_logical_raw(2, n1);
    }
    execute_lifted_x86(&[0xD8, 0xD9], &mut ctx, &mut memory); // FCOMP ST(1)
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 1);
        assert_eq!(x86.x87.physical_tag(0), 3);
        assert_eq!(x86.x87.status_word & 0x4500, 0x0000);
    }

    // Reinitialize and verify both FCOMPP and FUCOMPP pop exactly twice.
    for bytes in [&[0xDE, 0xD9][..], &[0xDA, 0xE9][..]] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.x87 = Default::default();
            x86.x87.set_logical_raw(0, p1);
            x86.x87.set_logical_raw(1, p1);
        }
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.top(), 2, "{bytes:02X?}");
            assert_eq!(x86.x87.physical_tag(0), 3, "{bytes:02X?}");
            assert_eq!(x86.x87.physical_tag(1), 3, "{bytes:02X?}");
            assert_eq!(x86.x87.status_word & 0x4500, 0x4000);
        }
    }

    // Memory forms widen exactly before comparing and retain restartable
    // read-fault semantics.
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x100);
    memory
        .write(0x100, &2.0f32.to_bits().to_le_bytes())
        .unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
        x86.x87.set_logical_raw(0, p1);
    }
    execute_lifted_x86(&[0xD8, 0x10], &mut ctx, &mut memory); // FCOM m32fp
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.status_word & 0x4500, 0x0100);
        assert_eq!(x86.x87.data_ptr, 0x100);
        assert_eq!(x86.x87.last_opcode, 0x0010);
    }
    memory
        .write(0x100, &1.0f64.to_bits().to_le_bytes())
        .unwrap();
    execute_lifted_x86(&[0xDC, 0x18], &mut ctx, &mut memory); // FCOMP m64fp
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.status_word & 0x4500, 0x4000);
        assert_eq!(x86.x87.top(), 1);
    }

    let before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.x87.clone(),
        _ => unreachable!(),
    };
    let mut short_memory = FlatMemory::new(0x104);
    let exit = execute_lifted_x86(&[0xDC, 0x18], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87, before);
    }
}
#[test]
fn lifted_x87_ficom_ficomp_widen_signed_integers_exactly() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x300);
    ctx.write_vreg(rax, 0x100);

    memory.write(0x100, &i16::MIN.to_le_bytes()).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87
            .set_logical_raw(0, raw(0x8000_0000_0000_0000, 0xC00E));
    }
    execute_lifted_x86(&[0xDE, 0x10], &mut ctx, &mut memory); // FICOM m16int
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.status_word & 0x4500, 0x4000);
        assert_eq!(x86.x87.top(), 0);
        assert_eq!(x86.x87.data_ptr, 0x100);
        assert_eq!(x86.x87.last_opcode, 0x0610);
    }

    memory.write(0x100, &i32::MAX.to_le_bytes()).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87
            .set_logical_raw(0, raw(0x8000_0000_0000_0000, 0x401E)); // 2^31
    }
    execute_lifted_x86(&[0xDA, 0x10], &mut ctx, &mut memory); // FICOM m32int
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.status_word & 0x4500, 0x0000);
        assert_eq!(x86.x87.top(), 0);
    }

    memory.write(0x100, &(-1i32).to_le_bytes()).unwrap();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.set_logical_raw(0, raw(0, 0));
    }
    execute_lifted_x86(&[0xDA, 0x18], &mut ctx, &mut memory); // FICOMP m32int
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.status_word & 0x4500, 0x0000);
        assert_eq!(x86.x87.top(), 1);
        assert_eq!(x86.x87.physical_tag(0), 3);
    }

    // Integer sources cannot generate #D or #IA; only an empty ST(0)
    // produces masked #IS and the pop still completes.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87 = Default::default();
    }
    execute_lifted_x86(&[0xDE, 0x18], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87.top(), 1);
        assert_eq!(x86.x87.status_word & 0x4543, 0x4541);
    }

    let before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.x87.clone(),
        _ => unreachable!(),
    };
    let mut short_memory = FlatMemory::new(0x102);
    let exit = execute_lifted_x86(&[0xDA, 0x18], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87, before);
    }
}
#[test]
fn lifted_x87_fild_widens_all_integer_widths_exactly_and_atomically() {
    fn raw(significand: u64, exponent_sign: u16) -> [u8; 10] {
        let mut value = [0u8; 10];
        value[..8].copy_from_slice(&significand.to_le_bytes());
        value[8..].copy_from_slice(&exponent_sign.to_le_bytes());
        value
    }
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    for (name, bytes, source, source_len, expected, tag, fop) in [
        (
            "m16 minimum",
            &[0xDF, 0x00][..],
            (i16::MIN as u16 as u64).to_le_bytes(),
            2usize,
            raw(0x8000_0000_0000_0000, 0xC00E),
            0u16,
            0x0700u16,
        ),
        (
            "m32 maximum",
            &[0xDB, 0x00][..],
            (i32::MAX as u32 as u64).to_le_bytes(),
            4,
            raw(0xFFFF_FFFE_0000_0000, 0x401D),
            0,
            0x0300,
        ),
        (
            "m64 minimum",
            &[0xDF, 0x28][..],
            (i64::MIN as u64).to_le_bytes(),
            8,
            raw(0x8000_0000_0000_0000, 0xC03E),
            0,
            0x0728,
        ),
        (
            "zero",
            &[0xDF, 0x28][..],
            0u64.to_le_bytes(),
            8,
            raw(0, 0),
            1,
            0x0728,
        ),
        (
            "negative one",
            &[0xDF, 0x28][..],
            u64::MAX.to_le_bytes(),
            8,
            raw(0x8000_0000_0000_0000, 0xBFFF),
            0,
            0x0728,
        ),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let mut memory = FlatMemory::new(0x200);
        ctx.write_vreg(rax, 0x100);
        memory.write(0x100, &source[..source_len]).unwrap();
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.x87.top(), 7, "{name}");
            assert_eq!(x86.x87.regs[7], expected, "{name}");
            assert_eq!(x86.x87.physical_tag(7), tag, "{name}");
            assert_eq!(x86.x87.status_word & 0x0043, 0, "{name}");
            assert_eq!(x86.x87.data_ptr, 0x100, "{name}");
            assert_eq!(x86.x87.last_opcode, fop, "{name}");
        }
    }

    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.x87.data_ptr = 0xCAFE;
        x86.x87.instr_ptr = 0xBEEF;
    }
    let before = match &ctx.arch_regs {
        ArchRegState::X86_64(x86) => x86.x87.clone(),
        _ => unreachable!(),
    };
    let mut short_memory = FlatMemory::new(0x104);
    let exit = execute_lifted_x86(&[0xDF, 0x28], &mut ctx, &mut short_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.x87, before);
    }
}
