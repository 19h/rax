//! Native-admission coverage for state-backed x86 LEA.
//!
//! `LEA` naming guest RSP/RBP was previously an unconditional interpreter
//! frontier: every function prologue/epilogue address computation
//! (`lea rax,[rsp+N]`, `lea rsp,[rbp-N]`) rejected the whole hot region. The
//! state-backed lowering rebuilds the effective address from the `GuestRegs`
//! file, so those forms are now admitted while every unmodeled address form
//! still fails closed.

use super::*;
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::{
    x86_state_backed_gpr_lea_candidate, x86_state_backed_gpr_lea_valid,
};

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

fn lea(dst: X86Reg, addr: Address, width: OpWidth) -> OpKind {
    OpKind::X86Lea {
        dst: x86(dst),
        addr,
        width,
    }
}

#[test]
fn stack_frame_address_computations_are_admitted_and_lower_natively() {
    for (name, kind) in [
        (
            "prologue scratch pointer",
            lea(X86Reg::Rax, base_offset(X86Reg::Rsp, 0x10), OpWidth::W64),
        ),
        (
            "epilogue stack restore",
            lea(X86Reg::Rsp, base_offset(X86Reg::Rbp, -0x28), OpWidth::W64),
        ),
        (
            "frame pointer establish",
            lea(X86Reg::Rbp, Address::Direct(x86(X86Reg::Rsp)), OpWidth::W64),
        ),
        (
            "zero-extending 32-bit form",
            lea(X86Reg::Rbx, base_offset(X86Reg::Rsp, 8), OpWidth::W32),
        ),
        (
            "16-bit partial destination",
            lea(X86Reg::Rsi, base_offset(X86Reg::Rbp, 4), OpWidth::W16),
        ),
        (
            "scaled index off the frame pointer",
            lea(
                X86Reg::R31,
                Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rbp)),
                    index: x86(X86Reg::Rcx),
                    scale: 8,
                    disp: 4,
                    disp_size: DispSize::Auto,
                },
                OpWidth::W64,
            ),
        ),
        (
            "base-less scaled stack pointer",
            lea(
                X86Reg::Rdx,
                Address::BaseIndexScale {
                    base: None,
                    index: x86(X86Reg::Rsp),
                    scale: 4,
                    disp: 8,
                    disp_size: DispSize::Auto,
                },
                OpWidth::W64,
            ),
        ),
        (
            "APX EGPR destination",
            lea(X86Reg::R16, base_offset(X86Reg::Rax, 0x20), OpWidth::W64),
        ),
    ] {
        let op = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
        );
        assert!(op.is_jit_safe(), "{name} must stay on the op whitelist");
        assert!(
            x86_state_backed_gpr_lea_candidate(&op),
            "{name} must be a state-backed candidate"
        );
        assert!(
            x86_state_backed_gpr_lea_valid(&op),
            "{name} must be an admitted state-backed shape"
        );
        assert!(x86_gate(kind.clone()), "{name} must pass the x86-64 gate");

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = crate::smir::lower::x86_64::X86_64Lowerer::new();
        lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{name} lowering: {error:?}"));
    }
}

#[test]
fn unmodeled_stack_address_forms_still_fail_closed() {
    for (name, kind) in [
        (
            "byte destination width",
            lea(X86Reg::Rsp, base_offset(X86Reg::Rax, 0), OpWidth::W8),
        ),
        (
            "displacement wider than imm32",
            lea(
                X86Reg::Rax,
                base_offset(X86Reg::Rsp, i64::from(i32::MAX) + 1),
                OpWidth::W64,
            ),
        ),
        (
            "RIP-relative destination",
            lea(
                X86Reg::Rsp,
                Address::PcRel {
                    offset: 0x20,
                    disp_size: DispSize::Auto,
                    base: Some(0x1000),
                },
                OpWidth::W64,
            ),
        ),
        (
            "segment-relative address",
            lea(
                X86Reg::Rax,
                Address::SegmentRel {
                    segment: x86(X86Reg::FsBase),
                    base: Some(x86(X86Reg::Rsp)),
                    index: None,
                    scale: 1,
                    disp: 0,
                },
                OpWidth::W64,
            ),
        ),
        (
            "explicit addr32 address",
            lea(
                X86Reg::Rax,
                Address::X86Addr32(Box::new(base_offset(X86Reg::Rsp, 0x10))),
                OpWidth::W32,
            ),
        ),
        (
            "invalid SIB scale",
            lea(
                X86Reg::Rax,
                Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rsp)),
                    index: x86(X86Reg::Rcx),
                    scale: 3,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                OpWidth::W64,
            ),
        ),
    ] {
        let op = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
        );
        assert!(
            x86_state_backed_gpr_lea_candidate(&op),
            "{name} must be recognized as a state-backed candidate"
        );
        assert!(
            !x86_state_backed_gpr_lea_valid(&op),
            "{name} must not be admitted"
        );
        assert!(
            !x86_gate(kind),
            "{name} must be rejected by the x86-64 gate"
        );
    }

    // A hinted LEA leaves the modeled shape and must be rejected even when its
    // operands are otherwise admitted.
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        lea(X86Reg::Rax, base_offset(X86Reg::Rsp, 0x10), OpWidth::W64),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_state_backed_gpr_lea_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn a_stack_frame_prologue_region_survives_o2_and_stays_admitted() {
    // sub rsp,0x18 ; lea rax,[rsp+8] ; mov rcx,rax
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Sub {
            dst: x86(X86Reg::Rsp),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Imm(0x18),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.push_op(
        0x1004,
        lea(X86Reg::Rax, base_offset(X86Reg::Rsp, 8), OpWidth::W64),
    );
    builder.push_op(
        0x1009,
        OpKind::Mov {
            dst: x86(X86Reg::Rcx),
            src: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(
        function
            .blocks
            .iter()
            .flat_map(|block| &block.ops)
            .any(|op| matches!(op.kind, OpKind::X86Lea { .. })),
        "O2 must retain the effective-address computation"
    );
    assert!(is_native_clobber_safe(&function));
}
