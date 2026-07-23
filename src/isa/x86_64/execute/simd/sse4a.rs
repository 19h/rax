//! AMD SSE4A EXTRQ/INSERTQ and MOVNTSS/MOVNTSD execution.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR4_OSFXSR: u64 = 1 << 9;

#[inline]
fn sse4a_mask(length: u8) -> u64 {
    if length == 0 {
        u64::MAX
    } else {
        u64::MAX >> (64 - length)
    }
}

/// Validate the dynamic SSE4A execution state in architectural exception
/// priority order. Reserved encoding checks remain the caller's responsibility
/// and must run before this helper so an encoding #UD wins over CR0.TS #NM.
fn require_sse4a(vcpu: &mut X86_64Vcpu) -> Result<bool> {
    if !vcpu.sse4a_enabled() || vcpu.sregs.cr0 & CR0_EM != 0 || vcpu.sregs.cr4 & CR4_OSFXSR == 0 {
        vcpu.inject_undefined_instruction()?;
        return Ok(false);
    }
    if vcpu.sregs.cr0 & CR0_TS != 0 {
        vcpu.inject_exception(7, None)?;
        return Ok(false);
    }
    Ok(true)
}

/// AMD SSE4A EXTRQ/INSERTQ (66/F2 0F 78/79).
///
/// These instructions are register-only and modify only the low 64 bits of
/// the destination XMM register. AMD defines the upper 64 bits as undefined;
/// retaining them gives the emulator a stable permitted result.
pub(in crate::isa::x86_64) fn execute_sse4a_bitfield(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let extract = ctx.operand_size_override && ctx.rep_prefix.is_none();
    let insert = !ctx.operand_size_override && ctx.rep_prefix == Some(0xF2);
    if !matches!(opcode, 0x78 | 0x79) || !(extract || insert) {
        return vcpu.inject_undefined_instruction();
    }

    // Decode-only validation precedes execution-state faults: reserved ModR/M
    // groups and memory forms are #UD even when CR0.TS would otherwise cause
    // #NM. Decoding the register selectors has no architectural side effect.
    let (reg, rm, is_memory, _, _) = vcpu.decode_modrm(ctx)?;
    if is_memory || extract && opcode == 0x78 && (ctx.bytes[ctx.cursor - 1] >> 3) & 7 != 0 {
        return vcpu.inject_undefined_instruction();
    }

    // AMD APM Vol. 4 specifies #UD before #NM when the feature or architectural
    // SSE enable state is absent. No register value or immediate is consumed
    // before this dynamic check, so the fault remains non-committing.
    if !require_sse4a(vcpu)? {
        return Ok(None);
    }

    let (length, index, dst, source) = match (extract, opcode) {
        (true, 0x78) => {
            let length = ctx.consume_u8()? & 0x3F;
            let index = ctx.consume_u8()? & 0x3F;
            (length, index, rm as usize, 0)
        }
        (true, 0x79) => {
            let control = vcpu.regs.xmm[rm as usize][0];
            (
                (control & 0x3F) as u8,
                ((control >> 8) & 0x3F) as u8,
                reg as usize,
                0,
            )
        }
        (false, 0x78) => {
            let length = ctx.consume_u8()? & 0x3F;
            let index = ctx.consume_u8()? & 0x3F;
            (length, index, reg as usize, vcpu.regs.xmm[rm as usize][0])
        }
        (false, 0x79) => {
            let source = vcpu.regs.xmm[rm as usize];
            (
                (source[1] & 0x3F) as u8,
                ((source[1] >> 8) & 0x3F) as u8,
                reg as usize,
                source[0],
            )
        }
        _ => unreachable!("validated SSE4A opcode and mandatory prefix"),
    };

    let mask = sse4a_mask(length);
    let old = vcpu.regs.xmm[dst][0];
    vcpu.regs.xmm[dst][0] = if extract {
        old.wrapping_shr(u32::from(index)) & mask
    } else {
        let shifted_mask = mask.wrapping_shl(u32::from(index));
        (old & !shifted_mask) | ((source & mask).wrapping_shl(u32::from(index)))
    };
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AMD SSE4A MOVNTSS/MOVNTSD (F3/F2 0F 2B /r).
///
/// RAX does not model cacheability or write-combining queues, so the
/// non-temporal hint has the same functional memory effect as an ordinary
/// 32-bit or 64-bit store. Encoding and execution-state faults remain exact.
pub(in crate::isa::x86_64) fn execute_sse4a_movnt_store(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    size: u8,
) -> Result<Option<VcpuExit>> {
    let prefix_valid = matches!(
        (size, ctx.rep_prefix, ctx.operand_size_override),
        (4, Some(0xF3), false) | (8, Some(0xF2), false)
    );
    if !prefix_valid {
        return vcpu.inject_undefined_instruction();
    }

    // The encoding is memory-only. Validate ModR/M before the dynamic state
    // guard so a reserved register form raises #UD even when CR0.TS is set.
    let (reg, _rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    if !is_memory {
        return vcpu.inject_undefined_instruction();
    }
    if !require_sse4a(vcpu)? {
        return Ok(None);
    }

    vcpu.write_mem(addr, vcpu.regs.xmm[reg as usize][0], size)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    use super::{CR0_EM, CR0_TS, CR4_OSFXSR};
    use crate::isa::x86_64::cpu::X86_64Vcpu;
    use crate::vm::vcpu::VCpu;

    const INITIAL_FLAGS: u64 = 0x2 | 0x08D5 | (1 << 10);

    fn vcpu_with_code(code: &[u8]) -> X86_64Vcpu {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10_000)]).unwrap());
        memory.write_slice(code, GuestAddress(0)).unwrap();
        let mut vcpu = X86_64Vcpu::new(0, memory);
        vcpu.sregs.cs.l = true;
        vcpu.sregs.efer = 1 << 10;
        vcpu.sregs.cr0 = 0x21;
        vcpu.sregs.cr4 = CR4_OSFXSR;
        vcpu.regs.rflags = INITIAL_FLAGS;
        vcpu.set_sse4a_enabled(true);
        vcpu
    }

    #[test]
    fn direct_sse4a_bitfield_forms_cover_controls_aliases_and_extended_xmm() {
        struct Case {
            name: &'static str,
            code: &'static [u8],
            dst: usize,
            src: usize,
            dst_value: [u64; 2],
            src_value: [u64; 2],
            expected_low: u64,
        }

        let cases = [
            Case {
                name: "EXTRQ immediate",
                code: &[0x66, 0x0F, 0x78, 0xC1, 0xC8, 0xC4],
                dst: 1,
                src: 1,
                dst_value: [0xFEDC_BA98_7654_3210, 0xA1A2_A3A4_A5A6_A7A8],
                src_value: [0; 2],
                expected_low: 0x21,
            },
            Case {
                name: "EXTRQ register",
                code: &[0x66, 0x0F, 0x79, 0xCA],
                dst: 1,
                src: 2,
                dst_value: [0xFEDC_BA98_7654_3210, 0xB1B2_B3B4_B5B6_B7B8],
                src_value: [0xFFFF_FFFF_FFFF_100C, 0],
                expected_low: 0x654,
            },
            Case {
                name: "INSERTQ immediate",
                code: &[0xF2, 0x0F, 0x78, 0xCA, 0x08, 0x10],
                dst: 1,
                src: 2,
                dst_value: [0xFFFF_0000_FFFF_0000, 0xC1C2_C3C4_C5C6_C7C8],
                src_value: [0xA5, 0],
                expected_low: 0xFFFF_0000_FFA5_0000,
            },
            Case {
                name: "INSERTQ register alias",
                code: &[0xF2, 0x0F, 0x79, 0xC9],
                dst: 1,
                src: 1,
                dst_value: [0x0123_4567_89AB_CDEF, 0xFFFF_FFFF_FFFF_2008],
                src_value: [0; 2],
                expected_low: 0x0123_45EF_89AB_CDEF,
            },
            Case {
                name: "EXTRQ extended XMM",
                code: &[0x66, 0x45, 0x0F, 0x79, 0xCA],
                dst: 9,
                src: 10,
                dst_value: [0x8877_6655_4433_2211, 0xD1D2_D3D4_D5D6_D7D8],
                src_value: [(4 << 8) | 8, 0xE1E2_E3E4_E5E6_E7E8],
                expected_low: 0x21,
            },
        ];

        for case in cases {
            let mut vcpu = vcpu_with_code(case.code);
            let destination_ymm = [0x8182_8384_8586_8788, 0x9192_9394_9596_9798];
            let destination_zmm = [
                0xA1A2_A3A4_A5A6_A7A8,
                0xB1B2_B3B4_B5B6_B7B8,
                0xC1C2_C3C4_C5C6_C7C8,
                0xD1D2_D3D4_D5D6_D7D8,
            ];
            vcpu.regs.xmm[case.dst] = case.dst_value;
            vcpu.regs.ymm_high[case.dst] = destination_ymm;
            vcpu.regs.zmm_high[case.dst] = destination_zmm;
            if case.src != case.dst {
                vcpu.regs.xmm[case.src] = case.src_value;
            }

            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("{}: {error:?}", case.name))
                    .is_none(),
                "{}",
                case.name
            );
            assert_eq!(
                vcpu.regs.xmm[case.dst][0], case.expected_low,
                "{}",
                case.name
            );
            assert_eq!(
                vcpu.regs.xmm[case.dst][1], case.dst_value[1],
                "{} deterministic undefined upper qword",
                case.name
            );
            assert_eq!(
                vcpu.regs.ymm_high[case.dst], destination_ymm,
                "{} YMM upper lanes",
                case.name
            );
            assert_eq!(
                vcpu.regs.zmm_high[case.dst], destination_zmm,
                "{} ZMM upper lanes",
                case.name
            );
            assert_eq!(vcpu.regs.rflags, INITIAL_FLAGS, "{} flags", case.name);
            assert_eq!(vcpu.regs.rip, case.code.len() as u64, "{} RIP", case.name);
        }
    }

    #[test]
    fn direct_sse4a_movnt_stores_exact_scalar_width_and_extended_xmm() {
        for (name, code, source, address, expected) in [
            (
                "MOVNTSS",
                &[0xF3, 0x0F, 0x2B, 0x08][..],
                1_usize,
                0x2000_u64,
                &[0x88, 0x77, 0x66, 0x55][..],
            ),
            (
                "MOVNTSD extended XMM and disp8",
                &[0xF2, 0x44, 0x0F, 0x2B, 0x48, 0x08][..],
                9,
                0x2008,
                &[0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11][..],
            ),
        ] {
            let mut vcpu = vcpu_with_code(code);
            vcpu.regs.rax = 0x2000;
            vcpu.regs.xmm[source] = [0x1122_3344_5566_7788, 0xA1A2_A3A4_A5A6_A7A8];
            vcpu.write_mem(address, 0xCCCC_CCCC_CCCC_CCCC, 8).unwrap();
            let before_xmm = vcpu.regs.xmm;

            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("{name}: {error:?}"))
                    .is_none()
            );
            let stored = vcpu.read_mem(address, 8).unwrap().to_le_bytes();
            assert_eq!(&stored[..expected.len()], expected, "{name}: scalar lane");
            if expected.len() == 4 {
                assert_eq!(&stored[4..], &[0xCC; 4], "{name}: adjacent bytes");
            }
            assert_eq!(vcpu.regs.xmm, before_xmm, "{name}: XMM source");
            assert_eq!(vcpu.regs.rflags, INITIAL_FLAGS, "{name}: flags");
            assert_eq!(vcpu.regs.rip, code.len() as u64, "{name}: RIP");
        }
    }

    #[test]
    fn direct_sse4a_movnt_dynamic_faults_are_precise_and_noncommitting() {
        for (name, enabled, cr0, cr4, vector) in [
            ("feature absent", false, 0x21, CR4_OSFXSR, 6),
            ("CR0.EM", true, 0x21 | CR0_EM, CR4_OSFXSR, 6),
            ("CR0.TS", true, 0x21 | CR0_TS, CR4_OSFXSR, 7),
            ("CR4.OSFXSR absent", true, 0x21, 0, 6),
            (
                "feature absence precedes CR0.TS",
                false,
                0x21 | CR0_TS,
                CR4_OSFXSR,
                6,
            ),
            (
                "CR0.EM precedes CR0.TS",
                true,
                0x21 | CR0_EM | CR0_TS,
                CR4_OSFXSR,
                6,
            ),
            (
                "CR4.OSFXSR absence precedes CR0.TS",
                true,
                0x21 | CR0_TS,
                0,
                6,
            ),
        ] {
            let mut vcpu = vcpu_with_code(&[0xF3, 0x0F, 0x2B, 0x08]);
            vcpu.regs.rax = 0x2000;
            vcpu.regs.xmm[1] = [0x1122_3344_5566_7788, 0xA1A2_A3A4_A5A6_A7A8];
            vcpu.write_mem(0x2000, 0xCCCC_CCCC_CCCC_CCCC, 8).unwrap();
            vcpu.set_sse4a_enabled(enabled);
            vcpu.sregs.cr0 = cr0;
            vcpu.sregs.cr4 = cr4;
            let before = vcpu.regs.clone();

            let error = format!("{:#}", vcpu.step().expect_err(name));
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "{name}: {error}"
            );
            assert_eq!(
                vcpu.read_mem(0x2000, 8).unwrap(),
                0xCCCC_CCCC_CCCC_CCCC,
                "{name}: memory"
            );
            assert_eq!(vcpu.regs.xmm, before.xmm, "{name}: XMM");
            assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: flags");
            assert_eq!(vcpu.regs.rip, before.rip, "{name}: RIP");
        }
    }

    #[test]
    fn direct_sse4a_movnt_reserved_shapes_precede_ts_and_memory_faults_do_not_commit() {
        for (name, code) in [
            ("register form", &[0xF3, 0x0F, 0x2B, 0xC1][..]),
            ("combined 66 prefix", &[0x66, 0xF3, 0x0F, 0x2B, 0x08][..]),
            ("LOCK", &[0xF0, 0xF3, 0x0F, 0x2B, 0x08][..]),
            ("REX2", &[0xF3, 0xD5, 0x00, 0x0F, 0x2B, 0x08][..]),
        ] {
            let mut vcpu = vcpu_with_code(code);
            vcpu.sregs.cr0 |= CR0_TS;
            vcpu.regs.rax = 0x2000;
            vcpu.write_mem(0x2000, 0xCCCC_CCCC_CCCC_CCCC, 8).unwrap();
            let before = vcpu.regs.clone();

            let error = format!("{:#}", vcpu.step().expect_err(name));
            assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
            assert_eq!(
                vcpu.read_mem(0x2000, 8).unwrap(),
                0xCCCC_CCCC_CCCC_CCCC,
                "{name}: memory"
            );
            assert_eq!(vcpu.regs.xmm, before.xmm, "{name}: XMM");
            assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: flags");
            assert_eq!(vcpu.regs.rip, before.rip, "{name}: RIP");
        }

        let mut fault = vcpu_with_code(&[0xF2, 0x0F, 0x2B, 0x08]);
        fault.regs.rax = 0x20_000;
        fault.regs.xmm[1] = [0x1122_3344_5566_7788, 0xA1A2_A3A4_A5A6_A7A8];
        let before = fault.regs.clone();
        let error = format!("{:#}", fault.step().expect_err("unmapped MOVNTSD"));
        assert!(error.contains("failed to write at 0x20000"), "{error}");
        assert_eq!(fault.regs.xmm, before.xmm);
        assert_eq!(fault.regs.rflags, before.rflags);
        assert_eq!(fault.regs.rip, before.rip);
    }

    #[test]
    fn direct_sse4a_dynamic_faults_are_precise_and_noncommitting() {
        for (name, enabled, cr0, cr4, vector) in [
            ("feature absent", false, 0x21, CR4_OSFXSR, 6),
            ("CR0.EM", true, 0x21 | CR0_EM, CR4_OSFXSR, 6),
            ("CR0.TS", true, 0x21 | CR0_TS, CR4_OSFXSR, 7),
            ("CR4.OSFXSR absent", true, 0x21, 0, 6),
            (
                "feature absence precedes CR0.TS",
                false,
                0x21 | CR0_TS,
                CR4_OSFXSR,
                6,
            ),
            (
                "CR0.EM precedes CR0.TS",
                true,
                0x21 | CR0_EM | CR0_TS,
                CR4_OSFXSR,
                6,
            ),
            (
                "CR4.OSFXSR absence precedes CR0.TS",
                true,
                0x21 | CR0_TS,
                0,
                6,
            ),
        ] {
            let mut vcpu = vcpu_with_code(&[0x66, 0x0F, 0x78, 0xC1, 8, 4]);
            vcpu.set_sse4a_enabled(enabled);
            vcpu.sregs.cr0 = cr0;
            vcpu.sregs.cr4 = cr4;
            vcpu.regs.xmm[1] = [0xFEDC_BA98_7654_3210, 0x1112_1314_1516_1718];
            let before = vcpu.regs.clone();

            let error = format!("{:#}", vcpu.step().expect_err(name));
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "{name}: {error}"
            );
            assert_eq!(vcpu.regs.xmm, before.xmm, "{name}: XMM commit");
            assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: flags commit");
            assert_eq!(vcpu.regs.rip, before.rip, "{name}: RIP commit");
        }
    }

    #[test]
    fn direct_sse4a_reserved_shapes_raise_ud_without_state_commit() {
        for (name, code) in [
            (
                "EXTRQ immediate nonzero group",
                &[0x66, 0x0F, 0x78, 0xC9, 8, 4][..],
            ),
            ("EXTRQ memory", &[0x66, 0x0F, 0x79, 0x00][..]),
            ("INSERTQ memory", &[0xF2, 0x0F, 0x79, 0x00][..]),
            ("missing mandatory prefix", &[0x0F, 0x79, 0xC1][..]),
            ("wrong mandatory prefix", &[0xF3, 0x0F, 0x79, 0xC1][..]),
            ("LOCK", &[0xF0, 0x66, 0x0F, 0x79, 0xC1][..]),
            ("REX2", &[0x66, 0xD5, 0x00, 0x0F, 0x79, 0xC1][..]),
        ] {
            let mut vcpu = vcpu_with_code(code);
            vcpu.regs.xmm[0] = [0x0123_4567_89AB_CDEF, 0x1112_1314_1516_1718];
            vcpu.regs.xmm[1] = [0xFEDC_BA98_7654_3210, 0x2122_2324_2526_2728];
            let before = vcpu.regs.clone();

            let error = format!("{:#}", vcpu.step().expect_err(name));
            assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
            assert_eq!(vcpu.regs.xmm, before.xmm, "{name}: XMM commit");
            assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: flags commit");
            assert_eq!(vcpu.regs.rip, before.rip, "{name}: RIP commit");
        }

        let mut ts_reserved = vcpu_with_code(&[0x66, 0x0F, 0x78, 0xC9, 0x08, 0x04]);
        ts_reserved.sregs.cr0 |= CR0_TS;
        let before = ts_reserved.regs.clone();
        let error = format!(
            "{:#}",
            ts_reserved
                .step()
                .expect_err("reserved group must precede CR0.TS")
        );
        assert!(error.contains("IDT entry 6 not present"), "{error}");
        assert_eq!(ts_reserved.regs.xmm, before.xmm);
        assert_eq!(ts_reserved.regs.rflags, before.rflags);
        assert_eq!(ts_reserved.regs.rip, before.rip);
    }
}
