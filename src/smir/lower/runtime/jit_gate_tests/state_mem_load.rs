//! Native-admission coverage for helper-backed loads into guest RSP/RBP.
//!
//! `mov rbp,[rsp+N]` / `mov rsp,[rbp-N]` and every other scalar load whose
//! architectural destination is a stack register used to reject the whole hot
//! region. The MMU-helper path commits into the destination's `GuestRegs` slot,
//! so those loads are admitted whenever memory JIT is enabled — and only then.

use super::*;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn load(dst: X86Reg, addr: Address, width: MemWidth, sign: SignExtend) -> OpKind {
    OpKind::Load {
        dst: x86(dst),
        addr,
        width,
        sign,
    }
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (index, op) in ops.into_iter().enumerate() {
        builder.push_op(0x1000 + index as u64, op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    is_native_clobber_safe_excluding(
        &builder.finish(),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn stack_destination_loads_are_admitted_only_under_memory_jit() {
    let shapes = [
        (
            "mov rbp,[rsp+8] frame reload",
            load(
                X86Reg::Rbp,
                Address::BaseOffset {
                    base: x86(X86Reg::Rsp),
                    offset: 8,
                    disp_size: DispSize::Disp8,
                },
                MemWidth::B8,
                SignExtend::Zero,
            ),
        ),
        (
            "mov rsp,[rbp-0x28] stack restore",
            load(
                X86Reg::Rsp,
                Address::BaseOffset {
                    base: x86(X86Reg::Rbp),
                    offset: -0x28,
                    disp_size: DispSize::Disp8,
                },
                MemWidth::B8,
                SignExtend::Zero,
            ),
        ),
        (
            "mov ebp,[rbx] zero-extending",
            load(
                X86Reg::Rbp,
                Address::Direct(x86(X86Reg::Rbx)),
                MemWidth::B4,
                SignExtend::Zero,
            ),
        ),
        (
            "movsx rbp,byte ptr [rbx] sign-extending",
            load(
                X86Reg::Rbp,
                Address::Direct(x86(X86Reg::Rbx)),
                MemWidth::B1,
                SignExtend::Sign,
            ),
        ),
        (
            "mov spl,[rbx+rcx*4+16] partial destination",
            load(
                X86Reg::Rsp,
                Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rbx)),
                    index: x86(X86Reg::Rcx),
                    scale: 4,
                    disp: 16,
                    disp_size: DispSize::Disp8,
                },
                MemWidth::B1,
                SignExtend::Zero,
            ),
        ),
        (
            "mov rbp,fs:[rbx] segment-relative",
            load(
                X86Reg::Rbp,
                Address::SegmentRel {
                    segment: x86(X86Reg::FsBase),
                    base: Some(x86(X86Reg::Rbx)),
                    index: None,
                    scale: 1,
                    disp: 0,
                },
                MemWidth::B8,
                SignExtend::Zero,
            ),
        ),
    ];

    for (name, kind) in shapes {
        assert!(
            gate(vec![kind.clone()], true),
            "{name} must be admitted under memory JIT"
        );
        assert!(
            !gate(vec![kind], false),
            "{name} must be rejected without memory JIT"
        );
    }
}

#[test]
fn unmodeled_stack_destination_loads_still_fail_closed() {
    for (name, kind) in [
        (
            "vector-width destination",
            load(
                X86Reg::Rsp,
                Address::Direct(x86(X86Reg::Rbx)),
                MemWidth::B16,
                SignExtend::Zero,
            ),
        ),
        (
            "unmodeled address form",
            load(
                X86Reg::Rbp,
                Address::GpRel { offset: 16 },
                MemWidth::B8,
                SignExtend::Zero,
            ),
        ),
    ] {
        assert!(
            !gate(vec![kind], true),
            "{name} must be rejected even under memory JIT"
        );
    }
}

#[test]
fn a_frame_teardown_region_survives_o2_and_stays_admitted() {
    // mov rbp,[rsp+8] ; lea rsp,[rsp+0x10] ; mov rax,rbp
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        load(
            X86Reg::Rbp,
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: 8,
                disp_size: DispSize::Disp8,
            },
            MemWidth::B8,
            SignExtend::Zero,
        ),
    );
    builder.push_op(
        0x1005,
        OpKind::X86Lea {
            dst: x86(X86Reg::Rsp),
            addr: Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: 0x10,
                disp_size: DispSize::Disp8,
            },
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x100A,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
    assert!(!is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        false,
    ));
}
