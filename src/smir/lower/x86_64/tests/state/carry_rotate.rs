//! State-backed carry-rotate lowering tests.

use super::*;
use crate::smir::lower::x86_64::tests::*;
use crate::smir::lower::x86_64::*;

#[test]
fn lower_state_backed_gpr_carry_rotate_emits_count_flag_contracts_and_rejects_malformed_shapes() {
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let rotate_flags = FlagSet::CF.union(FlagSet::OF);

    let one = lower_single_op(OpKind::Rcl {
        dst: x86(X86Reg::Rsp),
        src: x86(X86Reg::Rbp),
        amount: SrcOperand::Imm(1),
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(rotate_flags),
    });
    assert!(
        one.windows(3).any(|bytes| bytes == [0x48, 0xD1, 0xD2]),
        "state-backed RCL must rotate RDX through incoming CF: {one:02X?}"
    );
    assert_eq!(
        one.iter().filter(|byte| **byte == 0x9C).count(),
        2,
        "flagful RCL must save incoming and native RFLAGS: {one:02X?}"
    );
    assert_eq!(one.iter().filter(|byte| **byte == 0x9D).count(), 1);
    assert!(
        one.windows(9)
            .any(|bytes| bytes == [0x48, 0x81, 0x64, 0x24, 0x10, 0xFE, 0xF7, 0xFF, 0xFF]),
        "count-one RCL must replace exactly CF and OF: {one:02X?}"
    );

    let dynamic = lower_single_op(OpKind::Rcr {
        dst: x86(X86Reg::R31),
        src: x86(X86Reg::R16),
        amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
        width: OpWidth::W8,
        flags: FlagUpdate::All,
    });
    assert!(
        dynamic.windows(2).any(|bytes| bytes == [0xD2, 0xDA]),
        "state-backed RCR must use staged CL and DL: {dynamic:02X?}"
    );
    assert!(
        dynamic
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x83, 0xE7, 0x1F]),
        "byte RCR must classify the 5-bit masked count: {dynamic:02X?}"
    );
    assert!(
        dynamic
            .windows(2)
            .filter(|bytes| *bytes == [0x0F, 0x84])
            .count()
            >= 2,
        "dynamic RCR must branch on zero and one masked counts: {dynamic:02X?}"
    );

    let suppressed = lower_single_op(OpKind::Rcl {
        dst: x86(X86Reg::Rbp),
        src: x86(X86Reg::R31),
        amount: SrcOperand::Imm(9),
        width: OpWidth::W16,
        flags: FlagUpdate::None,
    });
    assert!(
        suppressed
            .windows(4)
            .any(|bytes| bytes == [0x66, 0xC1, 0xD2, 0x09]),
        "state-backed suppressed-output RCL must use staged DX: {suppressed:02X?}"
    );
    assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9C).count(), 1);
    assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9D).count(), 1);
    assert!(
        suppressed
            .windows(4)
            .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
        "word RCL must partially synchronize guest RBP: {suppressed:02X?}"
    );

    for malformed in [
        OpKind::Rcl {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W128,
            flags: FlagUpdate::Specific(rotate_flags),
        },
        OpKind::Rcr {
            dst: x86(X86Reg::R31),
            src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(rotate_flags),
        },
        OpKind::Rcl {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            amount: SrcOperand::Imm64(1),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(rotate_flags),
        },
        OpKind::Rcr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rbp),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
    ] {
        assert!(
            matches!(
                lower_single_op_err(malformed),
                LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
            ),
            "malformed state-backed carry rotate must fail lowering"
        );
    }
    assert!(matches!(
        lower_single_hinted_op_err(
            OpKind::Rcl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        LowerError::InvalidOperand { .. }
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_gpr_carry_rotate_preserves_alias_count_and_flag_contracts() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const STATUS_MASK: u64 = 0x8D5;
    let rotate_flags = FlagSet::CF.union(FlagSet::OF);

    struct Case {
        name: &'static str,
        right: bool,
        dst: X86Reg,
        src: X86Reg,
        count_reg: Option<X86Reg>,
        immediate: i64,
        width: OpWidth,
        flags: FlagUpdate,
        source: u64,
        count: u64,
        status: u64,
    }

    let cases = [
        Case {
            name: "RCL RSP,RBP,0 preserves every flag",
            right: false,
            dst: X86Reg::Rsp,
            src: X86Reg::Rbp,
            count_reg: None,
            immediate: 0,
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(rotate_flags),
            source: 0x8123_4567_89AB_CDEF,
            count: 0,
            status: 0x8D5,
        },
        Case {
            name: "RCL BPL,SPL,1 consumes incoming CF",
            right: false,
            dst: X86Reg::Rbp,
            src: X86Reg::Rsp,
            count_reg: None,
            immediate: 1,
            width: OpWidth::W8,
            flags: FlagUpdate::Specific(rotate_flags),
            source: 0x2233_4455_6677_5642,
            count: 1,
            status: 0x0D5,
        },
        Case {
            name: "RCR R16B,R31B,10 effective one preserves raw-multi OF",
            right: true,
            dst: X86Reg::R16,
            src: X86Reg::R31,
            count_reg: None,
            immediate: 10,
            width: OpWidth::W8,
            flags: FlagUpdate::All,
            source: 0xFFEE_DDCC_BBAA_1301,
            count: 10,
            status: 0x8D4,
        },
        Case {
            name: "RCL R31B,R16B,SP full through-carry period",
            right: false,
            dst: X86Reg::R31,
            src: X86Reg::R16,
            count_reg: Some(X86Reg::Rsp),
            immediate: 0,
            width: OpWidth::W8,
            flags: FlagUpdate::Specific(rotate_flags),
            source: 0xAABB_CCDD_EEFF_13A5,
            count: 9,
            status: 0x8D5,
        },
        Case {
            name: "RCL R31W,R16W,SP effective one raw multi",
            right: false,
            dst: X86Reg::R31,
            src: X86Reg::R16,
            count_reg: Some(X86Reg::Rsp),
            immediate: 0,
            width: OpWidth::W16,
            flags: FlagUpdate::Specific(rotate_flags),
            source: 0xAABB_CCDD_EEFF_4000,
            count: 18,
            status: 0x8D4,
        },
        Case {
            name: "RCR R16D,R16D,R16 all aliases",
            right: true,
            dst: X86Reg::R16,
            src: X86Reg::R16,
            count_reg: Some(X86Reg::R16),
            immediate: 0,
            width: OpWidth::W32,
            flags: FlagUpdate::Specific(rotate_flags),
            source: 0xAABB_CCDD_8000_0001,
            count: 0x8000_0001,
            status: 0x0D5,
        },
        Case {
            name: "RCL RCX,RAX,RCX NDD destination-count alias",
            right: false,
            dst: X86Reg::Rcx,
            src: X86Reg::Rax,
            count_reg: Some(X86Reg::Rcx),
            immediate: 0,
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(rotate_flags),
            source: 0x8123_4567_89AB_CDEF,
            count: 2,
            status: 0x8D5,
        },
        Case {
            name: "suppressed RCR RSP,R31D,BP consumes CF and zero-extends",
            right: true,
            dst: X86Reg::Rsp,
            src: X86Reg::R31,
            count_reg: Some(X86Reg::Rbp),
            immediate: 0,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            source: 0xFFEE_DDCC_8000_0001,
            count: 1,
            status: 0x8D5,
        },
    ];

    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    for case in cases {
        let amount = case
            .count_reg
            .map_or(SrcOperand::Imm(case.immediate), |reg| {
                SrcOperand::Reg(x86(reg))
            });
        let kind = if case.right {
            OpKind::Rcr {
                dst: x86(case.dst),
                src: x86(case.src),
                amount,
                width: case.width,
                flags: case.flags,
            }
        } else {
            OpKind::Rcl {
                dst: x86(case.dst),
                src: x86(case.src),
                amount,
                width: case.width,
                flags: case.flags,
            }
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
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
            *value = 0x1357_0000_2468_0000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let src_idx = case.src.gpr_index().unwrap() as usize;
        regs.gpr[src_idx] = case.source;
        if let Some(count_reg) = case.count_reg {
            let count_idx = count_reg.gpr_index().unwrap() as usize;
            if count_idx != src_idx {
                regs.gpr[count_idx] = case.count;
            }
        }
        regs.rflags = 0x2 | case.status;

        let mut expected = regs;
        let bits = u64::from(case.width.bits());
        let count_mask = if bits == 64 { 0x3f } else { 0x1f };
        let raw_count = case.count_reg.map_or(case.immediate as u64, |reg| {
            regs.gpr[reg.gpr_index().unwrap() as usize]
        });
        let masked = raw_count & count_mask;
        let effective = masked % (bits + 1);
        let source = regs.gpr[src_idx] & case.width.mask();
        let mut result = source;
        let mut carry = expected.rflags & 1 != 0;
        for _ in 0..effective {
            if case.right {
                let next = result & 1 != 0;
                result = (result >> 1) | (u64::from(carry) << (bits - 1));
                carry = next;
            } else {
                let next = result & case.width.sign_bit() != 0;
                result = ((result << 1) | u64::from(carry)) & case.width.mask();
                carry = next;
            }
        }
        expected.gpr[dst_idx] = match case.width {
            OpWidth::W8 | OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
            OpWidth::W32 | OpWidth::W64 => result,
            OpWidth::W128 => unreachable!(),
        };
        if case.flags.updates_any() && effective != 0 {
            expected.rflags = (expected.rflags & !1) | u64::from(carry);
            if masked == 1 {
                let msb = result & case.width.sign_bit() != 0;
                let of = if case.right {
                    let second = result & (case.width.sign_bit() >> 1) != 0;
                    msb != second
                } else {
                    msb != carry
                };
                expected.rflags = (expected.rflags & !(1 << 11)) | (u64::from(of) << 11);
            }
        }

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(
            regs.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{} status flags",
            case.name
        );
    }
}
