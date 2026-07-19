//! Address-size-override effective-address tests.

use super::*;
use crate::smir::lift::x86_64::*;

/// A 67h ModR/M address retains its architectural components under one
/// explicit addr32 contract. FS/GS is added only after the W32 offset wraps
/// and is zero-extended.
#[test]
fn memory_preserves_state_backed_zero_extended_offset() {
    let ops = lift_one(&[0x67, 0x48, 0x8b, 0x03]).expect("mov rax,[ebx]");
    assert!(matches!(
        ops.as_slice(),
        [SmirOp {
            kind:
        OpKind::Load {
                    addr: Address::X86Addr32(inner),
            width: MemWidth::B8,
            ..
                },
            ..
        }] if matches!(inner.as_ref(), Address::Direct(reg) if *reg == x86_gpr(3))
    ));

    let fs = lift_one(&[0x67, 0x64, 0x48, 0x8b, 0x44, 0x0b, 0x08]).expect("mov rax,fs:[ebx+ecx+8]");
    assert!(matches!(
        fs.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                addr: Address::X86Addr32(inner),
                ..
            },
            ..
        }] if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(base),
                index: Some(index),
                scale: 1,
                disp: 8,
            } if *base == x86_gpr(3) && *index == x86_gpr(1)
        )
    ));

    // REX.X/B remain effective in addr32 mode: [r8d+r12d*4+8].
    let high = lift_one(&[0x67, 0x4b, 0x8b, 0x44, 0xa0, 0x08]).expect("mov rax,[r8d+r12d*4+8]");
    assert!(matches!(
        high.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                addr: Address::X86Addr32(inner),
                ..
            },
            ..
        }] if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 8,
                ..
            } if *base == x86_gpr(8) && *index == x86_gpr(12)
        )
    ));
}

#[test]
fn modrm_rm5_remains_eip_relative_and_wraps_at_32_bits() {
    let mut prefix = X86Prefix::default();
    prefix.address_size_override = true;
    let decoded = decode_modrm(&[0x05, 0, 0, 0, 0], &prefix, 0).expect("decode addr32 rm5");
    assert!(decoded.addr.as_ref().unwrap().rip_relative);

    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let high_pc = 0x1_0000_1000;
    let high = lifter
        .lift_insn(high_pc, &[0x67, 0x48, 0x8b, 0x05, 0, 0, 0, 0], &mut ctx)
        .expect("addr32 EIP-relative load");
    assert!(matches!(
        high.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                addr: Address::X86Addr32(inner),
                ..
            },
            ..
        }] if matches!(inner.as_ref(), Address::Absolute(0x1008))
    ));

    let low = lifter
        .lift_insn(
            0,
            &[0x67, 0x48, 0x8b, 0x05, 0xf7, 0xff, 0xff, 0xff],
            &mut ctx,
        )
        .expect("wrapping addr32 EIP-relative load");
    assert!(matches!(
        low.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                addr: Address::X86Addr32(inner),
                ..
            },
            ..
        }] if matches!(inner.as_ref(), Address::Absolute(0xffff_ffff))
    ));
}

#[test]
fn sib_no_base_remains_absolute_and_fs_follows_eip_truncation() {
    let absolute = lift_one(&[0x67, 0x48, 0x8b, 0x04, 0x25, 0xfc, 0xff, 0xff, 0xff])
        .expect("addr32 SIB absolute disp32");
    assert!(matches!(
        absolute.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                addr: Address::X86Addr32(inner),
                ..
            },
            ..
        }] if matches!(inner.as_ref(), Address::Absolute(0xffff_fffc))
    ));

    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let fs = lifter
        .lift_insn(
            0x1_0000_2000,
            &[0x64, 0x67, 0x48, 0x8b, 0x05, 0xfc, 0xff, 0xff, 0xff],
            &mut ctx,
        )
        .expect("FS addr32 EIP-relative load");
    assert!(matches!(
        fs.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                addr: Address::X86Addr32(inner),
                ..
            },
            ..
        }] if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                    base: None,
                    index: None,
                    scale: 1,
                    disp: 0x2005,
            }
        )
    ));
}

#[test]
fn pop_memory_reuses_next_rip_for_addr32_eip_relative_destination() {
    let result =
        lift_single(&[0x67, 0x8f, 0x05, 0, 0, 0, 0]).expect("pop qword [addr32 EIP-relative]");
    assert!(result.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Store {
            addr: Address::Absolute(0x1007),
            width: MemWidth::B8,
            ..
        }
    )));
}
