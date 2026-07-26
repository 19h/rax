//! State-backed x86 LEA lowering coverage.
//!
//! Guest RSP/RBP and APX EGPRs live in the `GuestRegs` file while a native
//! region executes, so `LEA` forms naming them must rebuild the effective
//! address from that file instead of from the host registers of the same name.

use super::*;
use crate::smir::OpId;
use crate::smir::lower::x86_64::*;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn base_offset(base: X86Reg, offset: i64) -> Address {
    Address::BaseOffset {
        base: x86(base),
        offset,
        disp_size: DispSize::Auto,
    }
}

#[test]
fn state_backed_lea_rebuilds_every_admitted_address_form_from_the_guest_file() {
    // LEA RAX,[RSP+10h]: read the RSP slot (index 4 => +20h), add the
    // displacement with a 64-bit LEA, commit the full RAX slot (index 0).
    let bytes = lower_single_op(OpKind::X86Lea {
        dst: x86(X86Reg::Rax),
        addr: base_offset(X86Reg::Rsp, 0x10),
        width: OpWidth::W64,
    });
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8B, 0x50, 0x20]),
        "must read the guest RSP slot: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8D, 0x52, 0x10]),
        "must add the displacement with a 64-bit LEA: {bytes:02X?}"
    );
    assert!(
        bytes.windows(3).any(|b| b == [0x48, 0x89, 0x10]),
        "must commit the guest RAX slot: {bytes:02X?}"
    );

    // LEA R12D,[RBP-28h]: 32-bit LEA truncates and zero-extends, so the slot
    // commit is a full 64-bit store of the zero-extended value.
    let bytes = lower_single_op(OpKind::X86Lea {
        dst: x86(X86Reg::R12),
        addr: base_offset(X86Reg::Rbp, -0x28),
        width: OpWidth::W32,
    });
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8B, 0x50, 0x28]),
        "must read the guest RBP slot: {bytes:02X?}"
    );
    assert!(
        bytes.windows(3).any(|b| b == [0x8D, 0x52, 0xD8]),
        "32-bit destination must use a 32-bit LEA: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x89, 0x50, 0x60]),
        "must fully commit the guest R12 slot: {bytes:02X?}"
    );

    // LEA BP,[RSP]: 16-bit destination preserves the upper 48 bits, so the slot
    // commit is a 16-bit store and the prologue's saved guest RBP word is
    // partially synchronized too.
    let bytes = lower_single_op(OpKind::X86Lea {
        dst: x86(X86Reg::Rbp),
        addr: Address::Direct(x86(X86Reg::Rsp)),
        width: OpWidth::W16,
    });
    assert!(
        bytes.windows(3).any(|b| b == [0x66, 0x8D, 0x12]),
        "16-bit destination must use a 16-bit LEA: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x66, 0x89, 0x50, 0x28]),
        "must partially commit the guest RBP slot: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x66, 0x89, 0x55, 0x00]),
        "must partially synchronize the saved guest RBP word: {bytes:02X?}"
    );

    // LEA RSP,[RBP+RCX*8+4]: base and index are both reloaded from the file.
    let bytes = lower_single_op(OpKind::X86Lea {
        dst: x86(X86Reg::Rsp),
        addr: Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbp)),
            index: x86(X86Reg::Rcx),
            scale: 8,
            disp: 4,
            disp_size: DispSize::Auto,
        },
        width: OpWidth::W64,
    });
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8B, 0x78, 0x08]),
        "must read the guest RCX index slot: {bytes:02X?}"
    );
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x54, 0xFA, 0x04]),
        "must scale and add the index with a single LEA: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x89, 0x50, 0x20]),
        "must commit the guest RSP slot: {bytes:02X?}"
    );
    assert!(
        !bytes.windows(4).any(|b| b == [0x48, 0x89, 0x55, 0x00]),
        "an RSP destination must not rewrite the saved guest RBP word: {bytes:02X?}"
    );

    // LEA RBX,[RSP*4+8]: base-less SIB keeps RSP purely as a scaled index.
    let bytes = lower_single_op(OpKind::X86Lea {
        dst: x86(X86Reg::Rbx),
        addr: Address::BaseIndexScale {
            base: None,
            index: x86(X86Reg::Rsp),
            scale: 4,
            disp: 8,
            disp_size: DispSize::Auto,
        },
        width: OpWidth::W64,
    });
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8B, 0x78, 0x20]),
        "must read the guest RSP slot as the index: {bytes:02X?}"
    );
    assert!(
        bytes
            .windows(8)
            .any(|b| b == [0x48, 0x8D, 0x14, 0xBD, 0x08, 0x00, 0x00, 0x00]),
        "base-less SIB must keep a disp32 form: {bytes:02X?}"
    );
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x89, 0x50, 0x18]),
        "must commit the guest RBX slot: {bytes:02X?}"
    );

    // LEA never writes flags: the sequence must contain no PUSHFQ/POPFQ and
    // must release the pushed slot with the flag-preserving form.
    assert!(
        !bytes.contains(&0x9C) && !bytes.contains(&0x9D),
        "state-backed LEA must not save/restore flags: {bytes:02X?}"
    );
    assert!(
        bytes
            .windows(5)
            .any(|b| b == [0x48, 0x8D, 0x64, 0x24, 0x08]),
        "must release the pushed slot without touching flags: {bytes:02X?}"
    );
}

#[test]
fn state_backed_lea_rejects_every_unmodeled_shape() {
    for (name, kind) in [
        (
            "byte destination width",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rsp),
                addr: base_offset(X86Reg::Rax, 0),
                width: OpWidth::W8,
            },
        ),
        (
            "displacement wider than imm32",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rax),
                addr: base_offset(X86Reg::Rsp, i64::from(i32::MAX) + 1),
                width: OpWidth::W64,
            },
        ),
        (
            "RIP-relative address",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rsp),
                addr: Address::PcRel {
                    offset: 0x20,
                    disp_size: DispSize::Auto,
                    base: Some(0x1000),
                },
                width: OpWidth::W64,
            },
        ),
        (
            "absolute address",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rbp),
                addr: Address::Absolute(0x2000),
                width: OpWidth::W64,
            },
        ),
        (
            "segment-relative address",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rax),
                addr: Address::SegmentRel {
                    segment: x86(X86Reg::FsBase),
                    base: Some(x86(X86Reg::Rsp)),
                    index: None,
                    scale: 1,
                    disp: 0,
                },
                width: OpWidth::W64,
            },
        ),
        (
            "explicit addr32 address",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rax),
                addr: Address::X86Addr32(Box::new(base_offset(X86Reg::Rsp, 0x10))),
                width: OpWidth::W32,
            },
        ),
        (
            "virtual destination",
            OpKind::X86Lea {
                dst: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                addr: base_offset(X86Reg::Rsp, 0x10),
                width: OpWidth::W64,
            },
        ),
        (
            "virtual base",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rsp),
                addr: Address::BaseOffset {
                    base: VReg::Virtual(crate::smir::ir::types::VirtualId(1)),
                    offset: 0,
                    disp_size: DispSize::Auto,
                },
                width: OpWidth::W64,
            },
        ),
        (
            "invalid SIB scale",
            OpKind::X86Lea {
                dst: x86(X86Reg::Rax),
                addr: Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rsp)),
                    index: x86(X86Reg::Rcx),
                    scale: 3,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                width: OpWidth::W64,
            },
        ),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(
            x86_state_backed_gpr_lea_candidate(&op),
            "{name} must still be a state-backed candidate"
        );
        assert!(
            !x86_state_backed_gpr_lea_valid(&op),
            "{name} must not be admitted"
        );
        assert!(
            matches!(
                lower_single_op_err(kind),
                LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
            ),
            "{name} must fail lowering"
        );
    }

    // A hinted LEA is outside the modeled shape even when its operands are.
    let hinted = OpKind::X86Lea {
        dst: x86(X86Reg::Rax),
        addr: base_offset(X86Reg::Rsp, 0x10),
        width: OpWidth::W64,
    };
    assert!(matches!(
        lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
        LowerError::InvalidOperand { .. }
    ));
}

#[test]
fn lea_without_a_state_backed_operand_keeps_the_direct_lowering() {
    let op = SmirOp::new(
        OpId(0),
        0x1000,
        OpKind::X86Lea {
            dst: x86(X86Reg::Rax),
            addr: base_offset(X86Reg::Rbx, 0x10),
            width: OpWidth::W64,
        },
    );
    assert!(!x86_state_backed_gpr_lea_candidate(&op));
    assert!(!x86_state_backed_gpr_lea_valid(&op));

    let bytes = lower_single_op(op.kind);
    assert!(
        bytes.windows(4).any(|b| b == [0x48, 0x8D, 0x43, 0x10]),
        "plain LEA must stay a single host instruction: {bytes:02X?}"
    );
    assert!(
        !bytes.windows(4).any(|b| b == [0x48, 0x8B, 0x45, 0x18]),
        "plain LEA must not load the guest state pointer: {bytes:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_lea_matches_the_architectural_effective_address() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const STATUS: u64 = 0x8D5;

    struct Case {
        name: &'static str,
        dst: X86Reg,
        addr: Address,
        width: OpWidth,
        /// Effective address as a function of the entry GPR file.
        expect: fn(&[u64; 32]) -> u64,
    }

    let cases = [
        Case {
            name: "LEA RAX,[RSP+10h]",
            dst: X86Reg::Rax,
            addr: base_offset(X86Reg::Rsp, 0x10),
            width: OpWidth::W64,
            expect: |gpr| gpr[4].wrapping_add(0x10),
        },
        Case {
            name: "LEA RSP,[RBP-28h] epilogue form",
            dst: X86Reg::Rsp,
            addr: base_offset(X86Reg::Rbp, -0x28),
            width: OpWidth::W64,
            expect: |gpr| gpr[5].wrapping_sub(0x28),
        },
        Case {
            name: "LEA RBP,[RSP] in-place stack frame",
            dst: X86Reg::Rbp,
            addr: Address::Direct(x86(X86Reg::Rsp)),
            width: OpWidth::W64,
            expect: |gpr| gpr[4],
        },
        Case {
            name: "LEA EBX,[RSP+8] zero-extending",
            dst: X86Reg::Rbx,
            addr: base_offset(X86Reg::Rsp, 8),
            width: OpWidth::W32,
            expect: |gpr| u64::from(gpr[4].wrapping_add(8) as u32),
        },
        Case {
            name: "LEA R31,[RBP+RCX*8+4]",
            dst: X86Reg::R31,
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::Rcx),
                scale: 8,
                disp: 4,
                disp_size: DispSize::Auto,
            },
            width: OpWidth::W64,
            expect: |gpr| gpr[5].wrapping_add(gpr[1].wrapping_mul(8)).wrapping_add(4),
        },
        Case {
            name: "LEA RDX,[RSP*4+8] base-less SIB",
            dst: X86Reg::Rdx,
            addr: Address::BaseIndexScale {
                base: None,
                index: x86(X86Reg::Rsp),
                scale: 4,
                disp: 8,
                disp_size: DispSize::Auto,
            },
            width: OpWidth::W64,
            expect: |gpr| gpr[4].wrapping_mul(4).wrapping_add(8),
        },
    ];

    for case in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86Lea {
                dst: x86(case.dst),
                addr: case.addr.clone(),
                width: case.width,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x1357_0000_0000_2468u64
                .wrapping_add((index as u64).wrapping_mul(0x0011_2233_4455_6677));
        }
        regs.rflags = STATUS;
        let entry = regs.gpr;
        let mut expected = regs;
        expected.gpr[case.dst.gpr_index().unwrap() as usize] = (case.expect)(&entry);

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(regs.rflags & STATUS, STATUS, "{} status flags", case.name);
    }
}
