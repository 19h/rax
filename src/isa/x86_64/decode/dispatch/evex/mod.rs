//! EVEX-encoded (AVX-512) instruction dispatch.
//!
//! EVEX prefix format (after 0x62):
//! - P0: R X B R' 0 m m m
//! - P1: W v v v v 1 p p
//! - P2: z L' L b V' a a a
//!
//! mm field (opcode map):
//! - 1: 0F (two-byte opcode)
//! - 2: 0F 38 (three-byte opcode)
//! - 3: 0F 3A (three-byte opcode with immediate)
//! - 5: MAP5 (AVX-512 FP16)
//! - 6: MAP6 (AVX-512 FP16)

use crate::error::{Error, Result};
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

// ---- module tree (auto-split) ----
mod apx;
pub(crate) use apx::*;
mod apx_conditional;
pub(crate) use apx_conditional::*;
mod apx_count;
pub(crate) use apx_count::*;
mod apx_movbe;
pub(crate) use apx_movbe::*;
mod apx_movrs;
pub(crate) use apx_movrs::*;
mod apx_crc32;
pub(crate) use apx_crc32::*;
mod apx_invpcid;
pub(crate) use apx_invpcid::*;
mod apx_movdir;
pub(crate) use apx_movdir::*;
mod fp;
pub(crate) use fp::*;
mod map0f;
pub(crate) use map0f::*;
mod map0f38;
pub(crate) use map0f38::*;
mod map0f3a;
pub(crate) use map0f3a::*;
mod map5;
pub(crate) use map5::*;
mod misc;
pub(crate) use misc::*;

#[cfg(test)]
mod apx_crc32_tests;

#[cfg(test)]
mod apx_count_tests;

#[cfg(test)]
mod apx_cet_tests;

#[cfg(test)]
mod apx_movbe_tests;

#[cfg(test)]
mod apx_movrs_tests;

#[cfg(test)]
mod apx_group3_tests;

#[cfg(test)]
mod apx_invpcid_tests;

#[cfg(test)]
mod apx_movdir_tests;

#[cfg(test)]
mod apx_nf_tests;

#[cfg(test)]
mod apx_reserved_tests;

#[cfg(test)]
mod align_tests;

#[cfg(test)]
mod bw_immediate_tests;

#[cfg(test)]
mod bw_shuffle_madd_tests;

#[cfg(test)]
mod chunk_extract_tests;

#[cfg(test)]
mod chunk_insert_tests;

#[cfg(test)]
mod chunk_shuffle_tests;

#[cfg(test)]
mod fp_class_tests;

#[cfg(test)]
mod gpr_broadcast_tests;

#[cfg(test)]
mod mask_broadcast_tests;

#[cfg(test)]
mod pair_intersect_tests;

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use vm_memory::{GuestAddress, GuestMemoryMmap};

    use crate::isa::x86_64::flags;

    const CODE: u64 = 0x1000;
    const DATA: u64 = 0x2000;
    const INVALID: u64 = 0x2_0000;

    fn long_mode_vcpu(code: &[u8]) -> X86_64Vcpu {
        let mem =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        let mut vcpu = X86_64Vcpu::new(0, mem);
        vcpu.regs.rip = CODE;
        vcpu.regs.rflags = 0x2;
        vcpu.sregs.efer = 0x400;
        vcpu.sregs.cs.l = true;
        vcpu.sregs.cs.db = false;
        vcpu.set_apx_enabled(true);

        let sregs = vcpu.sregs.clone();
        vcpu.mmu.write(CODE, code, &sregs).unwrap();
        vcpu
    }

    fn step_ok(vcpu: &mut X86_64Vcpu) {
        assert!(vcpu.step().unwrap().is_none());
    }

    fn write_u64(vcpu: &mut X86_64Vcpu, addr: u64, value: u64) {
        let sregs = vcpu.sregs.clone();
        vcpu.mmu.write_u64(addr, value, &sregs).unwrap();
    }

    fn write_u32(vcpu: &mut X86_64Vcpu, addr: u64, value: u32) {
        let sregs = vcpu.sregs.clone();
        vcpu.mmu.write_u32(addr, value, &sregs).unwrap();
    }

    fn read_u64(vcpu: &mut X86_64Vcpu, addr: u64) -> u64 {
        let sregs = vcpu.sregs.clone();
        vcpu.mmu.read_u64(addr, &sregs).unwrap()
    }

    fn read_u32(vcpu: &mut X86_64Vcpu, addr: u64) -> u32 {
        let sregs = vcpu.sregs.clone();
        vcpu.mmu.read_u32(addr, &sregs).unwrap()
    }

    fn read_u8(vcpu: &mut X86_64Vcpu, addr: u64) -> u8 {
        let sregs = vcpu.sregs.clone();
        vcpu.mmu.read_u8(addr, &sregs).unwrap()
    }

    #[test]
    fn evex_vpmulhrsw_wig_executes_w0_and_w1_identically() {
        // vpmulhrsw %xmm11, %xmm10, %xmm9. Only EVEX.W differs.
        let execute = |w: bool| {
            let mut code = [0x62, 0x52, 0x2D, 0x08, 0x0B, 0xCB];
            if w {
                code[2] |= 0x80;
            }
            let mut vcpu = long_mode_vcpu(&code);
            vcpu.regs.xmm[9] = [u64::MAX; 2];
            vcpu.regs.ymm_high[9] = [u64::MAX; 2];
            vcpu.regs.zmm_high[9] = [u64::MAX; 4];
            vcpu.regs.xmm[10] = [0x4000_C000_7FFF_8000, 0x0001_FFFF_1234_EDCB];
            vcpu.regs.xmm[11] = [0x4000_4000_7FFF_8000, 0x7FFF_8000_CDEF_3210];

            step_ok(&mut vcpu);
            (
                vcpu.regs.xmm[9],
                vcpu.regs.ymm_high[9],
                vcpu.regs.zmm_high[9],
                vcpu.regs.rip,
            )
        };

        let w0 = execute(false);
        let w1 = execute(true);
        assert_eq!(w1, w0, "EVEX.W must not affect VPMULHRSW semantics");
        assert_eq!(w0.1, [0; 2], "128-bit EVEX form clears YMM upper state");
        assert_eq!(w0.2, [0; 4], "128-bit EVEX form clears ZMM upper state");
    }

    #[test]
    fn evex_packed_extend_wig_executes_w0_and_w1_identically() {
        let execute = |opcode: u8, w: bool, source: [u64; 2]| {
            let mut code = [0x62, 0xF2, 0x7D, 0x08, opcode, 0xC2];
            if w {
                code[2] |= 0x80;
            }
            let mut vcpu = long_mode_vcpu(&code);
            vcpu.regs.xmm[0] = [u64::MAX; 2];
            vcpu.regs.ymm_high[0] = [u64::MAX; 2];
            vcpu.regs.zmm_high[0] = [u64::MAX; 4];
            vcpu.regs.xmm[2] = source;

            step_ok(&mut vcpu);
            (
                vcpu.regs.xmm[0],
                vcpu.regs.ymm_high[0],
                vcpu.regs.zmm_high[0],
                vcpu.regs.rip,
            )
        };

        let signed_source = [0x0123_4567_FF01_7F80, 0xDEAD_BEEF_CAFE_BABE];
        for opcode in [0x20, 0x21, 0x22, 0x23, 0x24, 0x30, 0x31, 0x32, 0x33, 0x34] {
            assert_eq!(
                execute(opcode, true, signed_source),
                execute(opcode, false, signed_source),
                "EVEX.W is ignored for opcode {opcode:#04x}"
            );
        }

        // VPMOVSXBW sign-extends the low eight bytes to eight words.
        let signed_w0 = execute(0x20, false, signed_source);
        assert_eq!(signed_w0.0[0], 0xFFFF_0001_007F_FF80);

        // VPMOVZXWQ zero-extends the low two words to two qwords.
        let unsigned_source = [0x0123_4567_8000_FFFF, 0xDEAD_BEEF_CAFE_BABE];
        let unsigned_w0 = execute(0x34, false, unsigned_source);
        assert_eq!(unsigned_w0.0, [0xFFFF, 0x8000]);
        assert_eq!(unsigned_w0.1, [0; 2], "EVEX.128 clears YMM upper state");
        assert_eq!(unsigned_w0.2, [0; 4], "EVEX.128 clears ZMM upper state");
    }

    fn assert_evex_packed_extend_reserved_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        vcpu.regs.xmm[0] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        vcpu.regs.xmm[2] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        vcpu.regs.k[1] = 0xA5A5;
        let before = vcpu.regs.clone();

        let error = vcpu
            .step()
            .expect_err("reserved EVEX packed-extend form must #UD");
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
    }

    #[test]
    fn evex_packed_extend_reserved_fields_raise_precise_ud() {
        for code in [
            &[0x62, 0xF2, 0x6D, 0x08, 0x20, 0xC2][..], // nonreserved vvvv
            &[0x62, 0xF2, 0x7D, 0x00, 0x20, 0xC2],     // nonreserved V'
            &[0x62, 0xF2, 0x7D, 0x18, 0x20, 0xC2],     // EVEX.b
            &[0x62, 0xF2, 0x7D, 0x68, 0x20, 0xC2],     // reserved L'L=3
            &[0x62, 0xF2, 0x7D, 0x88, 0x20, 0xC2],     // {z} with k0
            &[0x62, 0xF2, 0xFD, 0x08, 0x25, 0xC2],     // VPMOVSXDQ requires W0
            &[0x62, 0xF2, 0xFD, 0x08, 0x35, 0xC2],     // VPMOVZXDQ requires W0
        ] {
            assert_evex_packed_extend_reserved_ud(code);
        }
    }

    #[test]
    fn evex_fixed_packed_compare_wig_executes_w0_and_w1_identically() {
        let execute = |opcode: u8, w: bool| {
            // vpcmpeq*/vpcmpgt* k3, xmm1, xmm2; only EVEX.W differs.
            let mut code = [0x62, 0xF1, 0x75, 0x08, opcode, 0xDA];
            if w {
                code[2] |= 0x80;
            }
            let mut vcpu = long_mode_vcpu(&code);
            vcpu.regs.xmm[1] = [0x7FFF_8000_0101_FF00, 0x8000_7FFF_FE02_0100];
            vcpu.regs.xmm[2] = [0x7FFE_8000_0001_FF00, 0x8001_7FFF_FF02_0000];
            vcpu.regs.k[3] = u64::MAX;

            step_ok(&mut vcpu);
            (vcpu.regs.k[3], vcpu.regs.rip)
        };

        for opcode in [0x64, 0x65, 0x74, 0x75] {
            let w0 = execute(opcode, false);
            let w1 = execute(opcode, true);
            assert_eq!(w1, w0, "EVEX.W must be ignored for opcode {opcode:#04x}");
            assert_eq!(w0.1, CODE + 6);
        }
    }

    fn assert_evex_fixed_packed_compare_reserved_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        vcpu.regs.xmm[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        vcpu.regs.xmm[2] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        vcpu.regs.k = [
            0x0000_0000_0000_0001,
            0x0000_0000_0000_0002,
            0x0000_0000_0000_0004,
            0x0000_0000_0000_0008,
            0x0000_0000_0000_0010,
            0x0000_0000_0000_0020,
            0x0000_0000_0000_0040,
            0x0000_0000_0000_0080,
        ];
        let before = vcpu.regs.clone();

        let error = vcpu
            .step()
            .expect_err("reserved EVEX fixed packed-compare form must #UD");
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
    }

    #[test]
    fn evex_fixed_packed_compare_reserved_fields_raise_precise_ud() {
        for code in [
            &[0x62, 0x71, 0x75, 0x08, 0x74, 0xCA][..], // extended k destination via R
            &[0x62, 0xE1, 0x75, 0x08, 0x74, 0xCA],     // extended k destination via R'
            &[0x62, 0xF1, 0x75, 0x89, 0x74, 0xCA],     // EVEX.z is reserved
            &[0x62, 0xF1, 0x75, 0x69, 0x74, 0xCA],     // reserved L'L=3
            &[0x62, 0xF1, 0x75, 0x19, 0x74, 0xCA],     // EVEX.b with register source
            &[0x62, 0xF1, 0xF5, 0x08, 0x76, 0xCA],     // VPCMPEQD requires W0
            &[0x62, 0xF2, 0x75, 0x08, 0x29, 0xCA],     // VPCMPEQQ requires W1
            &[0x62, 0xF1, 0x75, 0x18, 0x74, 0x00],     // byte compare has no broadcast
        ] {
            assert_evex_fixed_packed_compare_reserved_ud(code);
        }
    }

    #[test]
    fn evex_mask_blends_select_exact_elements_and_zero_upper_state() {
        let src1 = [0x7766_5544_3322_1100, 0xFFEE_DDCC_BBAA_9988];
        let src2 = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        let src1_u128 = u128::from(src1[0]) | (u128::from(src1[1]) << 64);
        let src2_u128 = u128::from(src2[0]) | (u128::from(src2[1]) << 64);
        let selector = 0b1010_0101_1100_0011u64;

        for (opcode, w, elem_size) in [
            (0x64, false, 4usize),
            (0x64, true, 8),
            (0x65, false, 4),
            (0x65, true, 8),
            (0x66, false, 1),
            (0x66, true, 2),
        ] {
            // vpblendm*/vblendm* xmm3{k1}, xmm1, xmm2
            let code = [
                0x62,
                0xF2,
                0x75 | if w { 0x80 } else { 0 },
                0x09,
                opcode,
                0xDA,
            ];
            let mut vcpu = long_mode_vcpu(&code);
            vcpu.regs.xmm[1] = src1;
            vcpu.regs.xmm[2] = src2;
            vcpu.regs.xmm[3] = [u64::MAX; 2];
            vcpu.regs.ymm_high[3] = [u64::MAX; 2];
            vcpu.regs.zmm_high[3] = [u64::MAX; 4];
            vcpu.regs.k[1] = selector;

            step_ok(&mut vcpu);

            let elem_bits = elem_size * 8;
            let elem_mask = (1u128 << elem_bits) - 1;
            let mut expected = 0u128;
            for lane in 0..(16 / elem_size) {
                let shift = lane * elem_bits;
                let source = if selector & (1u64 << lane) != 0 {
                    src2_u128
                } else {
                    src1_u128
                };
                expected |= ((source >> shift) & elem_mask) << shift;
            }
            assert_eq!(
                vcpu.regs.xmm[3],
                [expected as u64, (expected >> 64) as u64],
                "opcode {opcode:#04x}, W={w}"
            );
            assert_eq!(vcpu.regs.ymm_high[3], [0; 2]);
            assert_eq!(vcpu.regs.zmm_high[3], [0; 4]);
            assert_eq!(vcpu.regs.rip, CODE + 6);
        }

        // EVEX.z zeros selector-zero lanes rather than merging SRC1.
        let mut zeroing = long_mode_vcpu(&[0x62, 0xF2, 0x75, 0x89, 0x66, 0xDA]);
        zeroing.regs.xmm[1] = src1;
        zeroing.regs.xmm[2] = src2;
        zeroing.regs.xmm[3] = [u64::MAX; 2];
        zeroing.regs.k[1] = selector;
        step_ok(&mut zeroing);
        let mut expected_zeroing = 0u128;
        for lane in 0..16 {
            if selector & (1u64 << lane) != 0 {
                expected_zeroing |= ((src2_u128 >> (lane * 8)) & 0xFF) << (lane * 8);
            }
        }
        assert_eq!(
            zeroing.regs.xmm[3],
            [expected_zeroing as u64, (expected_zeroing >> 64) as u64]
        );

        // k0 means no selector mask, so every lane comes from SRC2.
        let mut no_mask = long_mode_vcpu(&[0x62, 0xF2, 0x75, 0x08, 0x64, 0xDA]);
        no_mask.regs.xmm[1] = src1;
        no_mask.regs.xmm[2] = src2;
        no_mask.regs.xmm[3] = [u64::MAX; 2];
        step_ok(&mut no_mask);
        assert_eq!(no_mask.regs.xmm[3], src2);
    }

    fn assert_evex_mask_blend_reserved_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        vcpu.regs.xmm[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        vcpu.regs.xmm[2] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        vcpu.regs.xmm[3] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        vcpu.regs.ymm_high[3] = [0xAAAA_AAAA_AAAA_AAAA; 2];
        vcpu.regs.zmm_high[3] = [0xBBBB_BBBB_BBBB_BBBB; 4];
        vcpu.regs.k[1] = 0xA55A_3CC3_F00F_9669;
        let before = vcpu.regs.clone();

        let error = vcpu
            .step()
            .expect_err("reserved EVEX mask-blend form must #UD");
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
    }

    #[test]
    fn evex_mask_blend_reserved_fields_raise_precise_ud() {
        for code in [
            &[0x62, 0xF2, 0x75, 0x68, 0x64, 0xDA][..], // reserved L'L=3
            &[0x62, 0xF2, 0x75, 0x88, 0x64, 0xDA],     // {z} requires k1-k7
            &[0x62, 0xF2, 0x75, 0x19, 0x64, 0xDA],     // EVEX.b with register source
            &[0x62, 0xF2, 0x75, 0x19, 0x66, 0x00],     // byte/word has no broadcast
        ] {
            assert_evex_mask_blend_reserved_ud(code);
        }
    }

    #[test]
    fn evex_vector_to_mask_collects_sign_bits_and_clears_high_mask_bits() {
        for (opcode, w, elem_size) in [
            (0x29, false, 1usize),
            (0x29, true, 2),
            (0x39, false, 4),
            (0x39, true, 8),
        ] {
            for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
                let mut source = [0u8; 64];
                let mut expected = 0u64;
                for lane in 0..(64 / elem_size) {
                    let sign_byte = lane * elem_size + elem_size - 1;
                    source[sign_byte] = ((lane * 37 + elem_size * 11) & 0x7F) as u8;
                    if lane % 3 != 1 {
                        source[sign_byte] |= 0x80;
                        if lane < vl_bytes / elem_size {
                            expected |= 1u64 << lane;
                        }
                    }
                }
                let source_qwords = std::array::from_fn(|index| {
                    u64::from_le_bytes(source[index * 8..index * 8 + 8].try_into().unwrap())
                });

                for source_reg in [0u8, 8, 16, 24] {
                    let mut p0 = 0xF2;
                    if source_reg & 0x08 != 0 {
                        p0 &= !0x20;
                    }
                    if source_reg & 0x10 != 0 {
                        p0 &= !0x40;
                    }
                    let code = [
                        0x62,
                        p0,
                        0x7E | if w { 0x80 } else { 0 },
                        (ll << 5) | 0x08,
                        opcode,
                        0xD8 | (source_reg & 0x07),
                    ];
                    let mut vcpu = long_mode_vcpu(&code);
                    if source_reg < 16 {
                        let index = source_reg as usize;
                        vcpu.regs.xmm[index].copy_from_slice(&source_qwords[..2]);
                        vcpu.regs.ymm_high[index].copy_from_slice(&source_qwords[2..4]);
                        vcpu.regs.zmm_high[index].copy_from_slice(&source_qwords[4..8]);
                    } else {
                        vcpu.regs.zmm_ext[(source_reg - 16) as usize] = source_qwords;
                    }
                    vcpu.regs.k = [0xA55A_A55A_A55A_A55A; 8];
                    let before = vcpu.regs.clone();

                    step_ok(&mut vcpu);

                    assert_eq!(vcpu.regs.k[3], expected, "{code:02X?}");
                    for index in 0..8 {
                        if index != 3 {
                            assert_eq!(
                                vcpu.regs.k[index], before.k[index],
                                "{code:02X?}, k{index}"
                            );
                        }
                    }
                    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
                    assert_eq!(
                        vcpu.regs.ymm_high, before.ymm_high,
                        "{code:02X?}: YMM state"
                    );
                    assert_eq!(
                        vcpu.regs.zmm_high, before.zmm_high,
                        "{code:02X?}: ZMM state"
                    );
                    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
                    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
                    assert_eq!(vcpu.regs.rip, CODE + 6, "{code:02X?}: RIP");
                }
            }
        }
    }

    #[test]
    fn evex_mask_to_vector_ignores_k_source_extension_fields() {
        let execute = |p0: u8| {
            let code = [0x62, p0, 0x7E, 0x08, 0x28, 0xD1];
            let mut vcpu = long_mode_vcpu(&code);
            vcpu.regs.k[1] = 0xA55A;
            vcpu.regs.xmm[2] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
            vcpu.regs.ymm_high[2] = [u64::MAX; 2];
            vcpu.regs.zmm_high[2] = [u64::MAX; 4];
            let before_flags = vcpu.regs.rflags;

            step_ok(&mut vcpu);

            assert_eq!(vcpu.regs.ymm_high[2], [0; 2], "{code:02X?}");
            assert_eq!(vcpu.regs.zmm_high[2], [0; 4], "{code:02X?}");
            assert_eq!(vcpu.regs.rflags, before_flags, "{code:02X?}");
            assert_eq!(vcpu.regs.rip, CODE + 6, "{code:02X?}");
            vcpu.regs.xmm[2]
        };

        let canonical = execute(0xF2);
        for p0 in [0xD2, 0xB2, 0x92] {
            assert_eq!(
                execute(p0),
                canonical,
                "EVEX.X/B must be ignored: {p0:#04X}"
            );
        }
    }

    #[test]
    fn evex_mask_to_vector_expands_all_shapes_and_clears_upper_lanes() {
        for (opcode, w, elem_size) in [
            (0x28, false, 1usize),
            (0x28, true, 2),
            (0x38, false, 4),
            (0x38, true, 8),
        ] {
            for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
                for destination in [1u8, 9, 17, 25] {
                    for source in [0u8, 3, 7] {
                        let mut p0 = 0xF2;
                        if destination & 0x08 != 0 {
                            p0 &= !0x80;
                        }
                        if destination & 0x10 != 0 {
                            p0 &= !0x10;
                        }
                        let code = [
                            0x62,
                            p0,
                            0x7E | if w { 0x80 } else { 0 },
                            (ll << 5) | 0x08,
                            opcode,
                            0xC0 | ((destination & 0x07) << 3) | source,
                        ];
                        let mut vcpu = long_mode_vcpu(&code);
                        for index in 0..16 {
                            vcpu.regs.xmm[index] = [
                                0x1111_2222_3333_4444 ^ index as u64,
                                0xAAAA_BBBB_CCCC_DDDD ^ index as u64,
                            ];
                            vcpu.regs.ymm_high[index] = [0x5555_5555_5555_5555 ^ index as u64; 2];
                            vcpu.regs.zmm_high[index] = [0xCCCC_CCCC_CCCC_CCCC ^ index as u64; 4];
                            vcpu.regs.zmm_ext[index] = [0xF0F0_F0F0_F0F0_F0F0 ^ index as u64; 8];
                        }
                        vcpu.regs.k = std::array::from_fn(|index| {
                            0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32)
                        });
                        let before = vcpu.regs.clone();
                        let source_mask = before.k[source as usize];

                        step_ok(&mut vcpu);

                        let mut expected_bytes = [0u8; 64];
                        for lane in 0..(vl_bytes / elem_size) {
                            if (source_mask >> lane) & 1 != 0 {
                                expected_bytes[lane * elem_size..(lane + 1) * elem_size].fill(0xFF);
                            }
                        }
                        let expected = std::array::from_fn(|index| {
                            u64::from_le_bytes(
                                expected_bytes[index * 8..index * 8 + 8].try_into().unwrap(),
                            )
                        });
                        let actual = if destination < 16 {
                            let index = destination as usize;
                            let mut value = [0u64; 8];
                            value[..2].copy_from_slice(&vcpu.regs.xmm[index]);
                            value[2..4].copy_from_slice(&vcpu.regs.ymm_high[index]);
                            value[4..].copy_from_slice(&vcpu.regs.zmm_high[index]);
                            value
                        } else {
                            vcpu.regs.zmm_ext[(destination - 16) as usize]
                        };
                        assert_eq!(actual, expected, "{code:02X?}");
                        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
                        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
                        assert_eq!(vcpu.regs.rip, CODE + 6, "{code:02X?}: RIP");

                        for index in 0..32u8 {
                            if index == destination {
                                continue;
                            }
                            if index < 16 {
                                let index = index as usize;
                                assert_eq!(vcpu.regs.xmm[index], before.xmm[index], "{code:02X?}");
                                assert_eq!(
                                    vcpu.regs.ymm_high[index], before.ymm_high[index],
                                    "{code:02X?}"
                                );
                                assert_eq!(
                                    vcpu.regs.zmm_high[index], before.zmm_high[index],
                                    "{code:02X?}"
                                );
                            } else {
                                let index = (index - 16) as usize;
                                assert_eq!(
                                    vcpu.regs.zmm_ext[index], before.zmm_ext[index],
                                    "{code:02X?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn assert_evex_mask_to_vector_reserved_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        vcpu.regs.xmm[2] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        vcpu.regs.ymm_high[2] = [0xAAAA_AAAA_AAAA_AAAA; 2];
        vcpu.regs.zmm_high[2] = [0xBBBB_BBBB_BBBB_BBBB; 4];
        vcpu.regs.k =
            std::array::from_fn(|index| 0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32));
        let before = vcpu.regs.clone();

        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => {
                panic!("reserved EVEX mask-to-vector form committed: {code:02X?}: {exit:?}")
            }
        };
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
    }

    #[test]
    fn evex_mask_to_vector_reserved_fields_raise_precise_ud() {
        for code in [
            &[0x62, 0xF2, 0x76, 0x08, 0x28, 0xD1][..], // EVEX.vvvv != 1111b
            &[0x62, 0xF2, 0x7E, 0x00, 0x28, 0xD1],     // EVEX.V' is reserved
            &[0x62, 0xF2, 0x7E, 0x09, 0x28, 0xD1],     // writemask is forbidden
            &[0x62, 0xF2, 0x7E, 0x88, 0x28, 0xD1],     // EVEX.z is reserved
            &[0x62, 0xF2, 0x7E, 0x18, 0x28, 0xD1],     // EVEX.b is reserved
            &[0x62, 0xF2, 0x7E, 0x68, 0x28, 0xD1],     // L'L=3 is reserved
            &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0x11],     // memory source is forbidden
        ] {
            assert_evex_mask_to_vector_reserved_ud(code);
        }
    }

    fn assert_evex_vector_to_mask_reserved_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        vcpu.regs.zmm_ext[0] = [
            0x807F_FF00_0123_FEDC,
            0x1122_3344_5566_7788,
            0x99AA_BBCC_DDEE_FF00,
            0x0123_4567_89AB_CDEF,
            0xFEDC_BA98_7654_3210,
            0x8000_0000_0000_0001,
            0x7FFF_FFFF_FFFF_FFFF,
            0xFFFF_0000_AAAA_5555,
        ];
        vcpu.regs.k =
            std::array::from_fn(|index| 0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32));
        let before = vcpu.regs.clone();

        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => {
                panic!("reserved EVEX vector-to-mask form committed: {code:02X?}: {exit:?}")
            }
        };
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
    }

    #[test]
    fn evex_vector_to_mask_reserved_fields_raise_precise_ud() {
        for code in [
            &[0x62, 0xF2, 0x76, 0x08, 0x29, 0xD8][..], // EVEX.vvvv != 1111b
            &[0x62, 0xF2, 0x7E, 0x00, 0x29, 0xD8],     // EVEX.V' is reserved
            &[0x62, 0xF2, 0x7E, 0x09, 0x29, 0xD8],     // writemask is forbidden
            &[0x62, 0xF2, 0x7E, 0x88, 0x29, 0xD8],     // EVEX.z is reserved
            &[0x62, 0xF2, 0x7E, 0x18, 0x29, 0xD8],     // EVEX.b is reserved
            &[0x62, 0xF2, 0x7E, 0x68, 0x29, 0xD8],     // L'L=3 is reserved
            &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0x18],     // memory source is forbidden
            &[0x62, 0x72, 0x7E, 0x08, 0x29, 0xD8],     // extended K destination via R
            &[0x62, 0xE2, 0x7E, 0x08, 0x29, 0xD8],     // extended K destination via R'
        ] {
            assert_evex_vector_to_mask_reserved_ud(code);
        }
    }

    fn assert_evex_lane_shuffle_reserved_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        for index in 0..16 {
            vcpu.regs.xmm[index] = [
                0x1111_2222_3333_4444u64.rotate_left(index as u32),
                0xAAAA_BBBB_CCCC_DDDDu64.rotate_right(index as u32),
            ];
            vcpu.regs.ymm_high[index] = [0x5555_5555_5555_5555 ^ index as u64; 2];
            vcpu.regs.zmm_high[index] = [0xCCCC_CCCC_CCCC_CCCC ^ index as u64; 4];
            vcpu.regs.zmm_ext[index] = [0xF0F0_F0F0_F0F0_F0F0 ^ index as u64; 8];
        }
        vcpu.regs.k =
            std::array::from_fn(|index| 0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32));
        vcpu.regs.rflags = 0x2 | flags::bits::CF | flags::bits::ZF | flags::bits::OF;
        let before = vcpu.regs.clone();

        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => panic!("reserved EVEX lane-shuffle form committed: {code:02X?}: {exit:?}"),
        };
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
    }

    #[test]
    fn evex_lane_shuffle_reserved_fields_raise_precise_ud() {
        for code in [
            &[0x62, 0xF1, 0x6E, 0x09, 0x12, 0xC8][..], // duplicate: nonreserved vvvv
            &[0x62, 0xF1, 0x7E, 0x01, 0x12, 0xC8],     // duplicate: nonreserved V'
            &[0x62, 0xF1, 0x7E, 0x19, 0x12, 0xC8],     // duplicate: EVEX.b
            &[0x62, 0xF1, 0x7E, 0x69, 0x12, 0xC8],     // duplicate: reserved L'L=3
            &[0x62, 0xF1, 0x7E, 0x88, 0x12, 0xC8],     // duplicate: {z} with k0
            &[0x62, 0xF1, 0x6D, 0x09, 0x70, 0xC8, 0x93], // shuffle: nonreserved vvvv
            &[0x62, 0xF1, 0x7D, 0x01, 0x70, 0xC8, 0x93], // shuffle: nonreserved V'
            &[0x62, 0xF1, 0x7D, 0x19, 0x70, 0xC8, 0x93], // shuffle: EVEX.b register
            &[0x62, 0xF1, 0x7E, 0x19, 0x70, 0x08, 0x93], // word shuffle: EVEX.b memory
            &[0x62, 0xF1, 0x7D, 0x69, 0x70, 0xC8, 0x93], // shuffle: reserved L'L=3
            &[0x62, 0xF1, 0x7D, 0x88, 0x70, 0xC8, 0x93], // shuffle: {z} with k0
        ] {
            assert_evex_lane_shuffle_reserved_ud(code);
        }
    }

    #[test]
    fn evex_word_lane_shuffles_treat_w_as_ignored() {
        let execute = |pp: u8, w: bool| {
            let code = [
                0x62,
                0xF1,
                0x7C | pp | if w { 0x80 } else { 0 },
                0x08,
                0x70,
                0xCA,
                0x93,
            ];
            let mut vcpu = long_mode_vcpu(&code);
            vcpu.regs.xmm[1] = [u64::MAX; 2];
            vcpu.regs.ymm_high[1] = [u64::MAX; 2];
            vcpu.regs.zmm_high[1] = [u64::MAX; 4];
            vcpu.regs.xmm[2] = [0x7766_5544_3322_1100, 0xFFEE_DDCC_BBAA_9988];
            let before_flags = vcpu.regs.rflags;

            step_ok(&mut vcpu);
            assert_eq!(vcpu.regs.ymm_high[1], [0; 2]);
            assert_eq!(vcpu.regs.zmm_high[1], [0; 4]);
            assert_eq!(vcpu.regs.rflags, before_flags);
            (vcpu.regs.xmm[1], vcpu.regs.rip)
        };

        for pp in [2, 3] {
            assert_eq!(execute(pp, true), execute(pp, false), "EVEX.pp={pp}");
        }
    }

    #[test]
    fn evex_vpshufd_memory_broadcast_remains_legal() {
        // VPSHUFD xmm1{k1}, dword ptr [rax]{1to4}, 0; EVEX.b is legal only
        // because ModR/M selects the m32bcst source.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF1, 0x7D, 0x19, 0x70, 0x08, 0x00]);
        vcpu.regs.rax = DATA;
        vcpu.regs.k[1] = 0xF;
        vcpu.regs.xmm[1] = [u64::MAX; 2];
        vcpu.regs.ymm_high[1] = [u64::MAX; 2];
        vcpu.regs.zmm_high[1] = [u64::MAX; 4];
        write_u32(&mut vcpu, DATA, 0x1122_3344);
        let before_flags = vcpu.regs.rflags;

        step_ok(&mut vcpu);

        assert_eq!(vcpu.regs.xmm[1], [0x1122_3344_1122_3344; 2]);
        assert_eq!(vcpu.regs.ymm_high[1], [0; 2]);
        assert_eq!(vcpu.regs.zmm_high[1], [0; 4]);
        assert_eq!(vcpu.regs.rflags, before_flags);
        assert_eq!(vcpu.regs.rip, CODE + 7);
    }

    fn enable_paging_for_wrapped_stack_test(vcpu: &mut X86_64Vcpu) {
        const PRESENT_WRITABLE: u64 = 0x3;
        const HUGE_PAGE: u64 = 0x80;
        const PML4: u64 = 0x3000;
        const LOW_PDPT: u64 = 0x4000;
        const LOW_PD: u64 = 0x5000;
        const HIGH_PDPT: u64 = 0x6000;
        const HIGH_PD: u64 = 0x7000;
        const HIGH_PT: u64 = 0x8000;
        const HIGH_STACK_PHYS: u64 = 0x9000;

        let sregs = vcpu.sregs.clone();
        vcpu.mmu
            .write_u64(PML4, LOW_PDPT | PRESENT_WRITABLE, &sregs)
            .unwrap();
        vcpu.mmu
            .write_u64(LOW_PDPT, LOW_PD | PRESENT_WRITABLE, &sregs)
            .unwrap();
        vcpu.mmu
            .write_u64(LOW_PD, PRESENT_WRITABLE | HUGE_PAGE, &sregs)
            .unwrap();

        vcpu.mmu
            .write_u64(PML4 + 511 * 8, HIGH_PDPT | PRESENT_WRITABLE, &sregs)
            .unwrap();
        vcpu.mmu
            .write_u64(HIGH_PDPT + 511 * 8, HIGH_PD | PRESENT_WRITABLE, &sregs)
            .unwrap();
        vcpu.mmu
            .write_u64(HIGH_PD + 511 * 8, HIGH_PT | PRESENT_WRITABLE, &sregs)
            .unwrap();
        vcpu.mmu
            .write_u64(
                HIGH_PT + 511 * 8,
                HIGH_STACK_PHYS | PRESENT_WRITABLE,
                &sregs,
            )
            .unwrap();

        vcpu.sregs.cr3 = PML4;
        vcpu.sregs.cr0 = 0x8000_0001;
        vcpu.sregs.efer = 0x500;
    }

    #[test]
    fn apx_push2_wraps_aligned_rsp_without_wrapping_the_transfer() {
        // LLVM 23: `push2 %rax, %rbx` => 62 f4 64 18 ff f0.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0]);
        enable_paging_for_wrapped_stack_test(&mut vcpu);
        // Zero is 16-byte aligned. The architectural RSP decrement wraps to
        // 0xffff_ffff_ffff_fff0, but the aligned 16-byte transfer remains
        // wholly inside the final canonical page and does not wrap to address 0.
        vcpu.regs.rsp = 0;
        vcpu.regs.rax = 0x1111_2222_3333_4444;
        vcpu.regs.rbx = 0xAAAA_BBBB_CCCC_DDDD;

        step_ok(&mut vcpu);

        assert_eq!(vcpu.regs.rsp, u64::MAX - 15);
        assert_eq!(read_u64(&mut vcpu, u64::MAX - 15), 0x1111_2222_3333_4444);
        assert_eq!(read_u64(&mut vcpu, u64::MAX - 7), 0xAAAA_BBBB_CCCC_DDDD);
    }

    fn assert_apx_reserved_group_ud(code: &[u8]) {
        let mut vcpu = long_mode_vcpu(code);
        vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
        vcpu.regs.rbx = 0xFEDC_BA98_7654_3210;
        vcpu.regs.rsp = DATA;
        vcpu.regs.rflags = 0x2 | flags::bits::CF | flags::bits::ZF;
        write_u64(&mut vcpu, DATA, 0x1111_2222_3333_4444);
        write_u64(&mut vcpu, DATA + 8, 0xAAAA_BBBB_CCCC_DDDD);

        let before = vcpu.regs.clone();
        let error = vcpu.step().expect_err("reserved APX group form must #UD");
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "wrong exception for {code:02X?}: {error:?}"
        );
        assert_eq!(vcpu.regs.rax, before.rax, "{code:02X?}: RAX");
        assert_eq!(vcpu.regs.rbx, before.rbx, "{code:02X?}: RBX");
        assert_eq!(vcpu.regs.rsp, before.rsp, "{code:02X?}: RSP");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: RFLAGS");
        assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
        assert_eq!(
            read_u64(&mut vcpu, DATA),
            0x1111_2222_3333_4444,
            "{code:02X?}: lower stack qword"
        );
        assert_eq!(
            read_u64(&mut vcpu, DATA + 8),
            0xAAAA_BBBB_CCCC_DDDD,
            "{code:02X?}: upper stack qword"
        );
    }

    #[test]
    fn apx_reserved_pop2_and_group45_forms_raise_precise_ud() {
        // Intel APX revision 5.0 assigns MAP4 8F only to POP2 /0 and assigns
        // FE/FF only to INC /0, DEC /1, plus FF /6 PUSH2. Every other group is
        // reserved. ModRM.Mod != 3 is independently #UD for PUSH2 and POP2,
        // and must not trigger SIB/displacement decoding or stack observation.
        for mode in 0..=3 {
            for group in 1..=7 {
                let modrm = (mode << 6) | (group << 3) | 3;
                assert_apx_reserved_group_ud(&[0x62, 0xF4, 0x7C, 0x18, 0x8F, modrm]);
            }

            for group in 2..=7 {
                let modrm = (mode << 6) | (group << 3) | 3;
                assert_apx_reserved_group_ud(&[0x62, 0xF4, 0x64, 0x18, 0xFE, modrm]);
            }

            for group in [2, 3, 4, 5, 7] {
                let modrm = (mode << 6) | (group << 3) | 3;
                assert_apx_reserved_group_ud(&[0x62, 0xF4, 0x64, 0x18, 0xFF, modrm]);
            }
        }

        for mode in 0..3 {
            assert_apx_reserved_group_ud(&[0x62, 0xF4, 0x7C, 0x18, 0x8F, (mode << 6) | 4]);
            assert_apx_reserved_group_ud(&[
                0x62,
                0xF4,
                0x64,
                0x18,
                0xFF,
                (mode << 6) | (6 << 3) | 4,
            ]);
        }
    }

    #[test]
    fn apx_ctest_default_flags_clear_stale_lazy_flags() {
        let code = [
            0x83, 0xC1, 0x01, // addl $1, %ecx
            0x62, 0xF4, 0xE4, 0x00, 0x85, 0xC3, // ctesto {dfv=of,sf} %rax, %rbx
            0x0F, 0x94, 0xC2, // setz %dl
        ];
        let mut vcpu = long_mode_vcpu(&code);
        vcpu.regs.rcx = u32::MAX as u64;
        vcpu.regs.rax = 1;
        vcpu.regs.rbx = 1;

        step_ok(&mut vcpu);
        step_ok(&mut vcpu);
        step_ok(&mut vcpu);

        assert_eq!(vcpu.regs.rdx & 0xFF, 0);
    }

    #[test]
    fn apx_imul_clears_stale_lazy_flags_for_following_condition() {
        let code = [
            0x83, 0xC1, 0x01, // addl $1, %ecx
            0x62, 0xF4, 0xFC, 0x08, 0xAF, 0xC3, // {evex} imulq %rbx, %rax
            0x0F, 0x94, 0xC2, // setz %dl
        ];
        let mut vcpu = long_mode_vcpu(&code);
        vcpu.regs.rcx = u32::MAX as u64;
        vcpu.regs.rax = 2;
        vcpu.regs.rbx = 3;

        step_ok(&mut vcpu);
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 6);
        step_ok(&mut vcpu);

        assert_eq!(vcpu.regs.rdx & 0xFF, 0);
    }

    #[test]
    fn apx_adc_reg_materializes_lazy_cf() {
        let code = [
            0x83, 0xC1, 0x01, // addl $1, %ecx
            0x62, 0xF4, 0x7C, 0x08, 0x11, 0xD8, // {evex} adcl %ebx, %eax
        ];
        let mut vcpu = long_mode_vcpu(&code);
        vcpu.regs.rcx = u32::MAX as u64;
        vcpu.regs.rax = 1;
        vcpu.regs.rbx = 0;

        step_ok(&mut vcpu);
        step_ok(&mut vcpu);

        assert_eq!(vcpu.regs.rax, 2);
    }

    #[test]
    fn apx_sbb_imm_materializes_lazy_cf() {
        let code = [
            0x83, 0xC1, 0x01, // addl $1, %ecx
            0x62, 0xF4, 0x7C, 0x08, 0x83, 0xD8, 0x00, // {evex} sbbl $0, %eax
        ];
        let mut vcpu = long_mode_vcpu(&code);
        vcpu.regs.rcx = u32::MAX as u64;
        vcpu.regs.rax = 1;

        step_ok(&mut vcpu);
        step_ok(&mut vcpu);

        assert_eq!(vcpu.regs.rax, 0);
    }

    #[test]
    fn apx_map4_setzu_and_evex_setcc_split_by_nd_like_llvm() {
        // LLVM 20: `setzub %al` => 62 f4 7f 18 42 c0.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0x7F, 0x18, 0x42, 0xC0]);
        vcpu.regs.rax = 0xAAAA_BBBB_CCCC_DDDD;
        vcpu.regs.rflags = 0x2 | flags::bits::CF;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 1);
        assert_eq!(vcpu.regs.rip, CODE + 6);

        // LLVM 20: `{evex} setb %al` => 62 f4 7f 08 42 c0.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0x7F, 0x08, 0x42, 0xC0]);
        vcpu.regs.rax = 0x1122_3344_5566_77FF;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 0x1122_3344_5566_7700);
        assert_eq!(vcpu.regs.rip, CODE + 6);
    }

    #[test]
    fn apx_cmov_nd_uses_vvvv_destination_like_llvm() {
        // LLVM 20: `cmovbq %rbx, %rax, %r8` => 62 f4 bc 18 42 c3.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xBC, 0x18, 0x42, 0xC3]);
        vcpu.regs.rax = 0x1111;
        vcpu.regs.rbx = 0x2222;
        vcpu.regs.r8 = 0x3333;
        vcpu.regs.rflags = 0x2 | flags::bits::CF;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.r8, 0x2222);

        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xBC, 0x18, 0x42, 0xC3]);
        vcpu.regs.rax = 0x1111;
        vcpu.regs.rbx = 0x2222;
        vcpu.regs.r8 = 0x3333;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.r8, 0x1111);
    }

    #[test]
    fn apx_cfcmov_two_operand_directions_and_false_zero_like_llvm() {
        // LLVM 20: clear NF decodes as `cfcmovbq %rax, %rbx`
        // from 62 f4 fc 08 42 d8: dst=ModRM.reg, src=r/m.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x08, 0x42, 0xD8]);
        vcpu.regs.rax = 0xAAAA;
        vcpu.regs.rbx = 0xBBBB;
        vcpu.regs.rflags = 0x2 | flags::bits::CF;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rbx, 0xAAAA);

        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x08, 0x42, 0xD8]);
        vcpu.regs.rax = 0xAAAA;
        vcpu.regs.rbx = 0xBBBB;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rbx, 0);

        // LLVM 20: `cfcmovbq %rbx, %rax` => 62 f4 fc 0c 42 d8.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x0C, 0x42, 0xD8]);
        vcpu.regs.rax = 0xAAAA;
        vcpu.regs.rbx = 0xBBBB;
        vcpu.regs.rflags = 0x2 | flags::bits::CF;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 0xBBBB);

        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x0C, 0x42, 0xD8]);
        vcpu.regs.rax = 0xAAAA;
        vcpu.regs.rbx = 0xBBBB;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 0);
    }

    #[test]
    fn apx_cfcmov_memory_source_suppresses_false_fault_like_llvm() {
        // LLVM 20: `cfcmovbq (%rbx), %rax` => 62 f4 fc 08 42 03.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x08, 0x42, 0x03]);
        vcpu.regs.rax = 0xAAAA;
        vcpu.regs.rbx = INVALID;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 0);

        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x08, 0x42, 0x03]);
        write_u64(&mut vcpu, DATA, 0xDEAD_BEEF_CAFE_BABE);
        vcpu.regs.rbx = DATA;
        vcpu.regs.rflags = 0x2 | flags::bits::CF;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.rax, 0xDEAD_BEEF_CAFE_BABE);

        // LLVM 20: `cfcmovbq (%rbx), %rax, %r8` => 62 f4 bc 1c 42 03.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xBC, 0x1C, 0x42, 0x03]);
        vcpu.regs.rax = 0x1234_5678;
        vcpu.regs.rbx = INVALID;
        vcpu.regs.r8 = 0xFFFF;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);
        assert_eq!(vcpu.regs.r8, 0x1234_5678);
    }

    #[test]
    fn apx_cfcmov_memory_destination_suppresses_false_fault_like_llvm() {
        // LLVM 20: `cfcmovbq %rbx, (%rax)` => 62 f4 fc 0c 42 18.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x0C, 0x42, 0x18]);
        vcpu.regs.rax = INVALID;
        vcpu.regs.rbx = 0xDEAD_BEEF_CAFE_BABE;
        vcpu.regs.rflags = 0x2;
        step_ok(&mut vcpu);

        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xFC, 0x0C, 0x42, 0x18]);
        vcpu.regs.rax = DATA;
        vcpu.regs.rbx = 0xDEAD_BEEF_CAFE_BABE;
        vcpu.regs.rflags = 0x2 | flags::bits::CF;
        step_ok(&mut vcpu);
        assert_eq!(read_u64(&mut vcpu, DATA), 0xDEAD_BEEF_CAFE_BABE);
    }

    #[test]
    fn apx_shld_imm_rip_relative_includes_imm8_in_target() {
        // LLVM 23: `{evex} shldl $1, %eax, 0x20(%rip)`
        let code = [
            0x62, 0xF4, 0x7C, 0x08, 0x24, 0x05, 0x20, 0x00, 0x00, 0x00, 0x01,
        ];
        let target = CODE + code.len() as u64 + 0x20;
        let mut vcpu = long_mode_vcpu(&code);
        vcpu.regs.rax = 0x8000_0000;
        write_u32(&mut vcpu, target, 0x4000_0000);

        step_ok(&mut vcpu);

        assert_eq!(read_u32(&mut vcpu, target), 0x8000_0001);
        assert_eq!(read_u8(&mut vcpu, target - 1), 0);
    }

    #[test]
    fn apx_shrd_imm_rip_relative_includes_imm8_in_target() {
        // LLVM 23: `{evex} shrdl $1, %eax, 0x20(%rip)`
        let code = [
            0x62, 0xF4, 0x7C, 0x08, 0x2C, 0x05, 0x20, 0x00, 0x00, 0x00, 0x01,
        ];
        let target = CODE + code.len() as u64 + 0x20;
        let mut vcpu = long_mode_vcpu(&code);
        vcpu.regs.rax = 1;
        write_u32(&mut vcpu, target, 2);

        step_ok(&mut vcpu);

        assert_eq!(read_u32(&mut vcpu, target), 0x8000_0001);
        assert_eq!(read_u8(&mut vcpu, target - 1), 0);
    }

    #[test]
    fn apx_cmov_nd_memory_source_still_faults_when_false() {
        // LLVM 20: `cmovbq (%rbx), %rax, %r8` => 62 f4 bc 18 42 03.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0xBC, 0x18, 0x42, 0x03]);
        vcpu.regs.rax = 0x1234;
        vcpu.regs.rbx = INVALID;
        vcpu.regs.r8 = 0xFFFF;
        vcpu.regs.rflags = 0x2;

        assert!(vcpu.step().is_err());
        assert_eq!(vcpu.regs.r8, 0xFFFF);
    }

    #[test]
    fn apx_conditional_map4_rejects_invalid_pp2_like_llvm() {
        // LLVM rejects PP=2 for the MAP4 conditional range.
        let mut vcpu = long_mode_vcpu(&[0x62, 0xF4, 0x7E, 0x18, 0x42, 0xC0]);
        let err = vcpu.step().unwrap_err();
        assert!(
            format!("{err:?}").contains("IDT entry 6 not present"),
            "{err:?}"
        );
    }

    #[test]
    fn f32_to_bf16_matches_vcvtne_rounding_edges() {
        let cases = [
            (0x3f80_7fff, 0x3f80),
            (0x3f80_8000, 0x3f80),
            (0x3f80_8001, 0x3f81),
            (0x3f81_8000, 0x3f82),
            (0xbf80_7fff, 0xbf80),
            (0xbf80_8000, 0xbf80),
            (0xbf80_8001, 0xbf81),
            (0xbf81_8000, 0xbf82),
            (0x007f_7fff, 0x0000),
            (0x807f_7fff, 0x8000),
            (0x0080_0000, 0x0080),
            (0x8080_0000, 0x8080),
            (0x7f7f_8000, 0x7f80),
            (0xff7f_8000, 0xff80),
            (0x7f80_0001, 0x7fc0),
        ];

        for (input, expected) in cases {
            assert_eq!(
                f32_to_bf16(f32::from_bits(input)),
                expected,
                "{input:#010x}"
            );
        }
    }
}

/// APX ALU operation types
#[derive(Clone, Copy)]
enum ApxAluOp {
    Add,
    Adc,
    Or,
    And,
    Sub,
    Sbb,
    Xor,
}

/// Convert IEEE 754 half-precision (FP16) to single-precision (f32)
fn fp16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;

    let f32_bits = if exp == 0 {
        if mant == 0 {
            // Zero (preserve sign)
            sign << 31
        } else {
            // Denormalized number - normalize it
            let mut m = mant;
            let mut e = 0i32;
            while (m & 0x400) == 0 {
                m <<= 1;
                e += 1;
            }
            m &= 0x3FF; // Remove implicit bit
            let new_exp = (127 - 14 - e) as u32;
            (sign << 31) | (new_exp << 23) | (m << 13)
        }
    } else if exp == 0x1F {
        // Infinity or NaN
        (sign << 31) | (0xFF << 23) | (mant << 13)
    } else {
        // Normalized number
        // FP16 exponent bias is 15, f32 is 127
        let new_exp = exp + 127 - 15;
        (sign << 31) | (new_exp << 23) | (mant << 13)
    };

    f32::from_bits(f32_bits)
}

/// Convert single-precision (f32) to IEEE 754 half-precision (FP16)
fn f32_to_fp16(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let abs = bits & 0x7fff_ffff;
    let exp = (abs >> 23) as i32;
    let mant = abs & 0x007f_ffff;

    if exp == 0xff {
        if mant == 0 {
            return (sign | 0x7c00) as u16;
        }
        let payload = (mant >> 13).max(1);
        return (sign | 0x7c00 | payload) as u16;
    }

    // Too small to round to the smallest half subnormal.
    if abs < 0x3300_0000 {
        return sign as u16;
    }

    // Half subnormal: round the f32 significand to a 10-bit denormal.
    if abs < 0x3880_0000 {
        let mant24 = mant | 0x0080_0000;
        let shift = (126 - exp) as u32;
        let round = 1u32 << (shift - 1);
        let half_mant = (mant24 + round - 1 + ((mant24 >> shift) & 1)) >> shift;
        return (sign | half_mant) as u16;
    }

    // Half normal: rebias exponent and round mantissa to nearest-even.
    let mut half = (abs - 0x3800_0000) >> 13;
    let remainder = abs & 0x1fff;
    if remainder > 0x1000 || (remainder == 0x1000 && (half & 1) != 0) {
        half += 1;
    }

    if half >= 0x7c00 {
        (sign | 0x7c00) as u16
    } else {
        (sign | half) as u16
    }
}

fn ftz_f32_bits(bits: u32) -> u32 {
    let exp = (bits >> 23) & 0xff;
    let mant = bits & 0x007f_ffff;
    if exp == 0 && mant != 0 {
        bits & 0x8000_0000
    } else {
        bits
    }
}

/// Convert BFloat16 (BF16) to single-precision (f32)
fn bf16_to_f32(bf: u16) -> f32 {
    // BF16 is simply the upper 16 bits of f32.
    f32::from_bits((bf as u32) << 16)
}

/// Convert single-precision (f32) to BFloat16 (BF16)
fn f32_to_bf16(f: f32) -> u16 {
    // BF16 is the upper 16 bits of f32 with round-to-nearest-even.
    let bits = f.to_bits();

    // Check for NaN and preserve signaling NaN.
    if (bits & 0x7FFFFFFF) > 0x7F800000 {
        // NaN - ensure we keep a non-zero mantissa
        return ((bits >> 16) as u16) | 0x0040;
    }

    let rounding_bias = 0x7FFF + ((bits >> 16) & 1);
    let rounded = ((bits.wrapping_add(rounding_bias)) >> 16) as u16;

    // x86 VCVTNE*PS2BF16 does not produce BF16 subnormals. Finite results
    // that underflow the BF16 normal range become signed zero.
    if (rounded & 0x7f80) == 0 {
        rounded & 0x8000
    } else {
        rounded
    }
}
