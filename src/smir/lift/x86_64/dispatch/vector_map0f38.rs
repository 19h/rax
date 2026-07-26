//! VEX/EVEX opcode-map `0F 38` dispatch.

use crate::smir::ir::ops::{X86SsePrefix, X86VecMap};
use crate::smir::ir::types::VecElementType;
use crate::smir::lift::x86_64::{VecEncodingKind, VecPrefix, X86_64Lifter};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vector_map0f38(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        debug_assert_eq!(prefix.map, X86VecMap::Map0F38);
        match opcode {
            _ if Self::is_profile_disabled_amx_0f38(prefix, opcode) => {
                self.lift_profile_disabled_amx(prefix, bytes, pc, false)
            }
            0xC8 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_exp2(prefix, opcode, bytes, pc, ctx)
            }
            0x4C..=0x4F if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_approx14(prefix, opcode, bytes, pc, ctx)
            }
            0x52 | 0x53
                if prefix.encoding == VecEncodingKind::Evex && prefix.pp == X86SsePrefix::Repne =>
            {
                self.lift_evex_four_dot_product(prefix, opcode, bytes, pc, ctx)
            }
            0xCA..=0xCD if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_approx28(prefix, opcode, bytes, pc, ctx)
            }
            0x2C | 0x2D if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_scale_f(prefix, opcode, bytes, pc, ctx)
            }
            0x42 | 0x43 if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_evex_get_exponent(prefix, opcode, bytes, pc, ctx)
            }
            0x13 if prefix.pp == X86SsePrefix::OpSize => self.lift_vec_packed_fp16_convert(
                prefix,
                bytes,
                pc,
                ctx,
                VecElementType::F16,
                VecElementType::F32,
            ),
            0x64..=0x66 if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_evex_mask_blend(prefix, opcode, bytes, pc, ctx)
            }
            0x2A | 0x3A if prefix.pp == X86SsePrefix::Rep => {
                self.lift_evex_mask_broadcast(prefix, opcode, bytes, pc, ctx)
            }
            0x18..=0x1B | 0x58..=0x5B | 0x78..=0x7C if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_vec_load_broadcast(prefix, opcode, bytes, pc, ctx)
            }
            0x10..=0x12 | 0x45..=0x47 if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_vec_packed_shift_variable(prefix, opcode, bytes, pc, ctx)
            }
            0x83 => self.lift_evex_multishift_qb(prefix, bytes, pc, ctx),
            0x70..=0x73 if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_evex_packed_funnel_shift(prefix, opcode, bytes, pc, ctx)
            }
            0x14 | 0x15 if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_evex_packed_rotate_variable(prefix, opcode, bytes, pc, ctx)
            }
            0x44 => self.lift_evex_vplzcnt(prefix, bytes, pc, ctx),
            0x50..=0x53 if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_vec_vnni_dot(prefix, opcode, bytes, pc, ctx)
            }
            0x50 | 0x51 => self.lift_vex_vnni_dot_ext(prefix, opcode, bytes, pc, ctx),
            0x52 if prefix.pp == X86SsePrefix::Rep => {
                self.lift_evex_bf16_dot(prefix, bytes, pc, ctx)
            }
            0x72 if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                self.lift_bf16_convert(prefix, bytes, pc, ctx)
            }
            0x62 | 0x63 | 0x88..=0x8B => {
                self.lift_evex_compress_expand(prefix, opcode, bytes, pc, ctx)
            }
            0x68 if prefix.pp == X86SsePrefix::Repne => {
                self.lift_evex_pair_intersect(prefix, bytes, pc, ctx)
            }
            0x8F => self.lift_evex_vpshufbitqmb(prefix, bytes, pc, ctx),
            0xD2 | 0xD3 => self.lift_vex_vnni_dot_ext(prefix, opcode, bytes, pc, ctx),
            0x54 | 0x55 => self.lift_evex_vpopcnt(prefix, opcode, bytes, pc, ctx),
            0xC6 | 0xC7 => self.lift_evex_sparse_prefetch(prefix, opcode, bytes, pc),
            0xC4 => self.lift_evex_vpconflict(prefix, bytes, pc, ctx),
            0x0C | 0x0D | 0x16 | 0x36 | 0x8D => {
                self.lift_vec_permute_variable(prefix, opcode, bytes, pc, ctx)
            }
            0x75..=0x77 | 0x7D..=0x7F => {
                self.lift_evex_permute_two_table(prefix, opcode, bytes, pc, ctx)
            }
            0xB0 | 0xB1 => self.lift_vex_ne_convert(prefix, opcode, bytes, pc, ctx),
            0xCB..=0xCD => self.lift_vex_sha512(prefix, opcode, bytes, pc, ctx),
            0xCF => self.lift_vec_gfni(prefix, opcode, bytes, pc, ctx),
            0xDA if matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) => {
                self.lift_vex_sm3_message(prefix, bytes, pc, ctx)
            }
            0xDA if matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne) => {
                self.lift_vex_sm4(prefix, bytes, pc, ctx)
            }
            0xDB..=0xDF => self.lift_vec_aes_round(prefix, opcode, bytes, pc, ctx),
            0x00 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_pshufb(prefix, bytes, pc, ctx)
            }
            0x00 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_pshufb(prefix, bytes, pc, ctx)
            }
            0x01..=0x03 | 0x05..=0x07 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_horizontal_integer(prefix, opcode, bytes, pc, ctx)
            }
            0x04 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_pmaddubsw(prefix, bytes, pc, ctx)
            }
            0x04 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_pmaddubsw(prefix, bytes, pc, ctx)
            }
            0x08..=0x0A if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_psign(prefix, opcode, bytes, pc, ctx)
            }
            0x0B if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_pmulhrsw(prefix, bytes, pc, ctx)
            }
            0x0B if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_pmulhrsw(prefix, bytes, pc, ctx)
            }
            0x0E | 0x0F => self.lift_vex_testp(prefix, opcode, bytes, pc, ctx),
            0x17 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_ptest(prefix, bytes, pc, ctx)
            }
            0x1C..=0x1E if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_pabs(prefix, opcode, bytes, pc, ctx)
            }
            0x1C..=0x1F if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_pabs(prefix, opcode, bytes, pc, ctx)
            }
            0x26 | 0x27 if matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Rep) => {
                self.lift_evex_integer_test_mask(prefix, opcode, bytes, pc, ctx)
            }
            0x28 | 0x29 | 0x38 | 0x39 if prefix.pp == X86SsePrefix::Rep => {
                self.lift_evex_mask_vector_convert(prefix, opcode, bytes, pc, ctx)
            }
            0x10..=0x15 | 0x20..=0x25 | 0x30..=0x35 if prefix.pp == X86SsePrefix::Rep => {
                self.lift_evex_integer_narrow(prefix, opcode, bytes, pc, ctx)
            }
            0x20..=0x25 | 0x30..=0x35 => {
                self.lift_vec_packed_extend(prefix, opcode, bytes, pc, ctx)
            }
            0x28 => self.lift_vec_pmuldq(prefix, bytes, true, pc, ctx),
            0x2C..=0x2F | 0x8C | 0x8E if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_masked_memory(prefix, opcode, bytes, pc, ctx)
            }
            0x90..=0x93 => self.lift_vec_gather(prefix, opcode, bytes, pc, ctx),
            0xA0..=0xA3 => self.lift_evex_scatter(prefix, opcode, bytes, pc, ctx),
            0xB4 | 0xB5 => self.lift_vec_vpmadd52(prefix, opcode, bytes, pc, ctx),
            0x9A | 0x9B | 0xAA | 0xAB
                if prefix.encoding == VecEncodingKind::Evex && prefix.pp == X86SsePrefix::Repne =>
            {
                self.lift_evex_four_fma(prefix, opcode, bytes, pc, ctx)
            }
            0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF => {
                self.lift_vec_fma3(prefix, opcode, bytes, pc, ctx)
            }
            0x2A => self.lift_vec_movntdqa(prefix, bytes, pc, ctx),
            0x2B if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_integer_pack(prefix, opcode, bytes, pc, ctx)
            }
            0x2B if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_integer_pack(prefix, opcode, bytes, pc, ctx)
            }
            0x29 | 0x37 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_integer_compare(prefix, opcode, bytes, pc, ctx)
            }
            0x29 | 0x37 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_integer_compare(prefix, opcode, bytes, pc, ctx)
            }
            0x38..=0x3F => self.lift_vec_packed_minmax(prefix, opcode, bytes, pc, ctx),
            0x41 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_phminposuw(prefix, bytes, pc, ctx)
            }
            0xE0..=0xEF => self.lift_cmpccxadd(prefix, opcode, bytes, pc, ctx),
            0xF2 | 0xF3 | 0xF5 | 0xF6 | 0xF7 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_bmi_0f38(prefix, opcode, bytes, pc, ctx)
            }
            0xF2 | 0xF3 | 0xF5 | 0xF6 | 0xF7 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_apx_bmi_0f38(opcode, bytes, pc, ctx)
            }
            0x40 => self.lift_vec_pmul_low(prefix, opcode, bytes, pc, ctx),
            _ => Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!("VEX 0F38 opcode 0x{:02X}", opcode),
            }),
        }
    }
}
