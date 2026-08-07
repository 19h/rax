//! Exact state-backed native lowering coverage for x86 RDPID.

use super::*;

#[test]
fn lower_rdpid_accepts_all_gprs_emits_exact_stack_commits_and_rejects_non_gprs() {
    for index in 0u8..32 {
        let dst = VReg::Arch(ArchReg::X86(X86Reg::gpr(index)));
        lower_single_op(OpKind::X86ReadPid { dst });
    }

    let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
    let direct = lower_single_op(OpKind::X86ReadPid { dst: r9 });
    let direct_expected = [
        0x4C, 0x8B, 0x4D, 0x18, // mov r9,[rbp+24] (GuestRegs pointer)
        0x45, 0x8B, 0x89, 0x90, 0x09, 0x00, 0x00, // mov r9d,[r9+2448]
    ];
    assert!(
        direct
            .windows(direct_expected.len())
            .any(|bytes| bytes == direct_expected),
        "direct RDPID state load missing {direct_expected:02X?} in {direct:02X?}"
    );

    let common = [
        0x50, // push rax
        0x51, // push rcx
        0x48, 0x8B, 0x45, 0x18, // mov rax,[rbp+24]
        0x8B, 0x88, 0x90, 0x09, 0x00, 0x00, // mov ecx,[rax+2448]
    ];
    for (name, register, expected) in [
        (
            "RSP",
            X86Reg::Rsp,
            [common.as_slice(), &[0x48, 0x89, 0x48, 0x20, 0x59, 0x58]].concat(),
        ),
        (
            "RBP",
            X86Reg::Rbp,
            [
                common.as_slice(),
                &[0x48, 0x89, 0x48, 0x28, 0x48, 0x89, 0x4D, 0x00, 0x59, 0x58],
            ]
            .concat(),
        ),
        (
            "R16",
            X86Reg::R16,
            [
                common.as_slice(),
                &[0x48, 0x89, 0x88, 0x80, 0x00, 0x00, 0x00, 0x59, 0x58],
            ]
            .concat(),
        ),
    ] {
        let code = lower_single_op(OpKind::X86ReadPid {
            dst: VReg::Arch(ArchReg::X86(register)),
        });
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name} state-backed RDPID missing {expected:02X?} in {code:02X?}"
        );
    }

    for dst in [
        VReg::Virtual(crate::smir::ir::types::VirtualId(99)),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
        VReg::Arch(ArchReg::X86(X86Reg::Rip)),
    ] {
        assert!(
            matches!(
                lower_single_op_err(OpKind::X86ReadPid { dst }),
                LowerError::InvalidRegister(_)
                    | LowerError::RegisterAllocationFailed { .. }
                    | LowerError::InvalidOperand { .. }
            ),
            "malformed RDPID destination must fail lowering: {dst:?}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_rdpid_preserves_complete_state_around_direct_stack_and_apx_destinations() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const TSC_AUX: u32 = 0xA5C3_7E91;
    for index in [4u8, 5, 9, 16] {
        let mut builder = FunctionBuilder::new(FunctionId(u32::from(index)), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86ReadPid {
                dst: VReg::Arch(ArchReg::X86(X86Reg::gpr(index))),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("RDPID GPR {index}: {error:?}"));
        let code = lowerer.finalize().expect("finalize RDPID");
        let exec = ExecMem::new(&code).expect("map RDPID");

        let mut regs = GuestRegs::default();
        for (gpr_index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x0102_0304_0506_0708u64
                .wrapping_add((gpr_index as u64).wrapping_mul(0x1111_1111_1111_1111));
        }
        regs.rflags = 0x2 | 0x8D5;
        regs.tsc_aux = TSC_AUX;
        regs.zmm[0] = [
            0x8000_0000_0000_0001,
            0x7FF8_1234_5678_9ABC,
            0x0123_4567_89AB_CDEF,
            0xFEDC_BA98_7654_3210,
            1,
            2,
            3,
            4,
        ];
        regs.k[1] = 0xA5A5_5A5A_C3C3_3C3C;
        regs.mm[0] = 0xDEAD_BEEF_CAFE_BABE;
        let before = regs;
        exec.run(lowered.entry_offset, &mut regs);

        let mut expected_gprs = before.gpr;
        expected_gprs[usize::from(index)] = u64::from(TSC_AUX);
        assert_eq!(regs.gpr, expected_gprs, "RDPID GPR {index}");
        assert_eq!(regs.rflags, before.rflags, "RDPID GPR {index}: RFLAGS");
        assert_eq!(regs.tsc_aux, before.tsc_aux, "RDPID GPR {index}: TSC_AUX");
        assert_eq!(regs.zmm, before.zmm, "RDPID GPR {index}: ZMM state");
        assert_eq!(regs.k, before.k, "RDPID GPR {index}: opmask state");
        assert_eq!(regs.mm, before.mm, "RDPID GPR {index}: MMX state");

        let second_tsc_aux = !TSC_AUX;
        regs.gpr[usize::from(index)] = u64::MAX;
        regs.tsc_aux = second_tsc_aux;
        let second_before = regs;
        exec.run(lowered.entry_offset, &mut regs);
        let mut second_expected_gprs = second_before.gpr;
        second_expected_gprs[usize::from(index)] = u64::from(second_tsc_aux);
        assert_eq!(
            regs.gpr, second_expected_gprs,
            "RDPID GPR {index}: dynamic second read"
        );
        assert_eq!(
            regs.rflags, second_before.rflags,
            "RDPID GPR {index}: second-read RFLAGS"
        );
        assert_eq!(
            regs.tsc_aux, second_tsc_aux,
            "RDPID GPR {index}: second-read TSC_AUX"
        );
    }
}
