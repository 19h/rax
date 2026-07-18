//! tests::jit tests

use super::*;
use crate::smir::lower::x86_64::*;

    #[test]
    fn jit_pcrel_lea_materializes_exact_guest_address_without_relocation() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Lea {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                addr: Address::PcRel {
                    offset: -0x27,
                    disp_size: DispSize::Disp32,
                    base: Some(0x1234_5007),
                },
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        // The general lowerer retains relocation metadata for consumers that
        // place code relative to the guest image.
        let mut relocated = X86_64Lowerer::new();
        let relocated_result = relocated.lower_function(&func).unwrap();
        assert_eq!(relocated_result.relocations.len(), 1);
        assert_eq!(
            relocated_result.relocations[0].target,
            RelocTarget::GuestAddr(0x1234_4FE0)
        );

        // Independently allocated JIT code must instead carry the numeric guest
        // effective address directly and expose no unresolved relocation.
        let mut jit = X86_64Lowerer::new();
        jit.set_guest_pcrel_lea_immediates(true);
        let jit_result = jit.lower_function(&func).unwrap();
        assert!(jit_result.relocations.is_empty());
        let code = jit.finalize().unwrap();
        assert!(
            code.windows(7)
                .any(|window| window == [0x48, 0xC7, 0xC0, 0xE0, 0x4F, 0x34, 0x12]),
            "guest-address immediate missing from {code:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_guarded_unsigned_division_stages_every_source_class() {
        for (name, instruction, expected_divide) in [
            (
                "DIV BL",
                &[0xF6, 0xF3, 0xF4][..],
                &[0xF6, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV SP",
                &[0x66, 0xF7, 0xF4, 0xF4][..],
                &[0x66, 0xF7, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV EBP",
                &[0xF7, 0xF5, 0xF4][..],
                &[0xF7, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV R15",
                &[0x49, 0xF7, 0xF7, 0xF4][..],
                &[0x48, 0xF7, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV R16",
                &[0xD5, 0x18, 0xF7, 0xF0, 0xF4][..],
                &[0x48, 0xF7, 0x74, 0x24, 0x18][..],
            ),
            (
                "APX NF DIV RBX",
                &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xF3, 0xF4][..],
                &[0x48, 0xF7, 0x74, 0x24, 0x18][..],
            ),
        ] {
            let (lowered, entry) = lower_jit_guarded_x86_block(instruction, false);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected_divide.len())
                    .any(|bytes| bytes == expected_divide),
                "{name} must divide by the staged stack snapshot: {lowered:02X?}"
            );
            assert!(
                lowered.windows(2).any(|bytes| bytes == [0x0F, 0x84]),
                "{name} must guard a zero divisor: {lowered:02X?}"
            );
            assert!(
                lowered.windows(2).any(|bytes| bytes == [0x0F, 0x83]),
                "{name} must guard quotient overflow: {lowered:02X?}"
            );
            if name == "DIV R16" {
                assert!(
                    lowered
                        .windows(7)
                        .any(|bytes| bytes == [0x48, 0x8B, 0x88, 0x80, 0x00, 0x00, 0x00]),
                    "REX2 B4 must select canonical GuestRegs.gpr[16]: {lowered:02X?}"
                );
            }
        }

        let (high, _) = lower_jit_guarded_x86_block(&[0xF6, 0xF5, 0xF4], false);
        assert!(
            high.windows(4)
                .any(|bytes| bytes == [0x48, 0xC1, 0xE8, 0x08]),
            "DIV CH must extract the staged legacy high byte: {high:02X?}"
        );
        assert!(
            high.windows(4)
                .any(|bytes| bytes == [0xF6, 0x74, 0x24, 0x18]),
            "DIV CH must consume the extracted stack byte: {high:02X?}"
        );

        for (name, instruction, expected_divide) in [
            (
                "DIV byte [RBX]",
                &[0xF6, 0x33, 0xF4][..],
                &[0xF6, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV word [RBX]",
                &[0x66, 0xF7, 0x33, 0xF4][..],
                &[0x66, 0xF7, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV dword [RBX]",
                &[0xF7, 0x33, 0xF4][..],
                &[0xF7, 0x74, 0x24, 0x18][..],
            ),
            (
                "DIV qword [RBX]",
                &[0x48, 0xF7, 0x33, 0xF4][..],
                &[0x48, 0xF7, 0x74, 0x24, 0x18][..],
            ),
        ] {
            let (memory, _) = lower_jit_guarded_x86_block(instruction, true);
            assert!(
                memory
                    .windows(5)
                    .any(|bytes| bytes == [0x48, 0x89, 0x44, 0x24, 0x10]),
                "{name} must stage the zero-extended helper result: {memory:02X?}"
            );
            assert!(
                memory
                    .windows(expected_divide.len())
                    .any(|bytes| bytes == expected_divide),
                "{name} must use the staged helper result: {memory:02X?}"
            );
        }
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_guarded_signed_division_stages_every_source_class() {
        for (name, instruction, expected_divide) in [
            (
                "IDIV BL",
                &[0xF6, 0xFB, 0xF4][..],
                &[0xF6, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV SP",
                &[0x66, 0xF7, 0xFC, 0xF4][..],
                &[0x66, 0xF7, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV EBP",
                &[0xF7, 0xFD, 0xF4][..],
                &[0xF7, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV R15",
                &[0x49, 0xF7, 0xFF, 0xF4][..],
                &[0x48, 0xF7, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV R16",
                &[0xD5, 0x18, 0xF7, 0xF8, 0xF4][..],
                &[0x48, 0xF7, 0x7C, 0x24, 0x20][..],
            ),
            (
                "APX NF IDIV RBX",
                &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xFB, 0xF4][..],
                &[0x48, 0xF7, 0x7C, 0x24, 0x20][..],
            ),
        ] {
            let (lowered, entry) = lower_jit_guarded_x86_block(instruction, false);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected_divide.len())
                    .any(|bytes| bytes == expected_divide),
                "{name} must divide by the unchanged staged signed source: {lowered:02X?}"
            );
            assert!(
                lowered.windows(2).any(|bytes| bytes == [0x0F, 0x84]),
                "{name} must guard a zero divisor: {lowered:02X?}"
            );
            assert!(
                lowered.windows(2).any(|bytes| bytes == [0x0F, 0x83]),
                "{name} must guard the signed quotient threshold: {lowered:02X?}"
            );
            assert!(
                lowered.windows(2).any(|bytes| bytes == [0x0F, 0x89]),
                "{name} must select the threshold by quotient sign: {lowered:02X?}"
            );
            if name == "IDIV R16" {
                assert!(
                    lowered
                        .windows(7)
                        .any(|bytes| bytes == [0x48, 0x8B, 0x88, 0x80, 0x00, 0x00, 0x00]),
                    "REX2 B4 must select canonical GuestRegs.gpr[16]: {lowered:02X?}"
                );
            }
        }

        let (high, _) = lower_jit_guarded_x86_block(&[0xF6, 0xFD, 0xF4], false);
        assert!(
            high.windows(4)
                .any(|bytes| bytes == [0x48, 0xC1, 0xE8, 0x08]),
            "IDIV CH must extract the staged legacy high byte: {high:02X?}"
        );
        assert!(
            high.windows(4)
                .any(|bytes| bytes == [0xF6, 0x7C, 0x24, 0x20]),
            "IDIV CH must consume the unchanged extracted stack byte: {high:02X?}"
        );

        for (name, instruction, expected_divide) in [
            (
                "IDIV byte [RBX]",
                &[0xF6, 0x3B, 0xF4][..],
                &[0xF6, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV word [RBX]",
                &[0x66, 0xF7, 0x3B, 0xF4][..],
                &[0x66, 0xF7, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV dword [RBX]",
                &[0xF7, 0x3B, 0xF4][..],
                &[0xF7, 0x7C, 0x24, 0x20][..],
            ),
            (
                "IDIV qword [RBX]",
                &[0x48, 0xF7, 0x3B, 0xF4][..],
                &[0x48, 0xF7, 0x7C, 0x24, 0x20][..],
            ),
        ] {
            let (memory, _) = lower_jit_guarded_x86_block(instruction, true);
            assert!(
                memory
                    .windows(5)
                    .any(|bytes| bytes == [0x48, 0x89, 0x44, 0x24, 0x10]),
                "{name} must stage the zero-extended helper result: {memory:02X?}"
            );
            assert!(
                memory
                    .windows(expected_divide.len())
                    .any(|bytes| bytes == expected_divide),
                "{name} must use the raw staged helper result: {memory:02X?}"
            );
        }
    }
    #[test]
    fn lower_x86_alignment_check_emits_precise_deopt_and_rejects_malformed_ir() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let code = lower_single_op(OpKind::X86CheckAlignment {
            addr: Address::Direct(rax),
            alignment: 64,
        });
        assert!(
            code.windows(7)
                .any(|bytes| bytes == [0x48, 0xF7, 0xC6, 0x3F, 0, 0, 0]),
            "alignment mask test missing from {code:02X?}"
        );
        assert!(
            code.windows(2).any(|bytes| bytes == [0x0F, 0x85]),
            "misalignment must branch to the current-PC exit"
        );

        for malformed in [
            OpKind::X86CheckAlignment {
                addr: Address::Direct(rax),
                alignment: 8,
            },
            OpKind::X86CheckAlignment {
                addr: Address::Direct(VReg::Virtual(crate::smir::ir::types::VirtualId(7))),
                alignment: 16,
            },
            OpKind::X86CheckAlignment {
                addr: Address::BaseIndexScale {
                    base: None,
                    index: rax,
                    scale: 3,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                alignment: 32,
            },
            OpKind::X86CheckAlignment {
                addr: Address::PcRel {
                    offset: 0,
                    disp_size: DispSize::Auto,
                    base: None,
                },
                alignment: 64,
            },
            OpKind::X86CheckAlignment {
                addr: Address::GpRel { offset: 0 },
                alignment: 16,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                        | LowerError::UnsupportedOp { .. }
                        | LowerError::InvalidRegister(_)
                        | LowerError::RegisterAllocationFailed { .. }
                ),
                "malformed alignment check must fail lowering"
            );
        }
    }
