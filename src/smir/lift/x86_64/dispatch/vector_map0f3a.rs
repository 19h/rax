//! VEX/EVEX opcode-map `0F 3A` dispatch.

use crate::smir::ir::ops::{X86SsePrefix, X86VecMap};
use crate::smir::ir::types::VecElementType;
use crate::smir::lift::x86_64::{VecEncodingKind, VecPrefix, X86_64Lifter};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vector_map0f3a(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        debug_assert_eq!(prefix.map, X86VecMap::Map0F3A);
        match opcode {
            0x30..=0x33 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_opmask(prefix, opcode, bytes, pc, ctx)
            }
            0x26 | 0x27
                if prefix.encoding == VecEncodingKind::Evex
                    && matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize) =>
            {
                self.lift_evex_get_mantissa(prefix, opcode, bytes, pc, ctx)
            }
            0x1D if prefix.pp == X86SsePrefix::OpSize => {
                self.lift_vec_packed_f32_to_f16_store(prefix, bytes, pc, ctx)
            }
            0x18 | 0x19 | 0x1A | 0x1B | 0x38 | 0x39 | 0x3A | 0x3B
                if prefix.encoding == VecEncodingKind::Evex =>
            {
                self.lift_evex_chunk_extract_insert(prefix, opcode, bytes, pc, ctx)
            }
            0x23 | 0x43 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_shuffle_128_chunks(prefix, opcode, bytes, pc, ctx)
            }
            0x66 | 0x67 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_fp_class(prefix, opcode, bytes, pc, ctx)
            }
            0xCE | 0xCF => self.lift_vec_gfni(prefix, opcode, bytes, pc, ctx),
            0x1E | 0x1F | 0x3E | 0x3F if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_integer_compare(prefix, opcode, bytes, pc, ctx)
            }
            0x03 => self.lift_evex_vector_align(prefix, bytes, pc, ctx),
            0x70..=0x73 => self.lift_evex_packed_funnel_shift(prefix, opcode, bytes, pc, ctx),
            0x25 => self.lift_evex_ternary_logic(prefix, bytes, pc, ctx),
            0x00 | 0x01 | 0x04 | 0x05 => {
                self.lift_vec_permute_immediate(prefix, opcode, bytes, pc, ctx)
            }
            0x06 | 0x46 => self.lift_vex_permute2x128(prefix, bytes, pc, ctx),
            0xDE => self.lift_vex_sm3_rounds2(prefix, bytes, pc, ctx),
            0x08..=0x0B if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_round_scale(prefix, opcode, bytes, pc, ctx)
            }
            0x08..=0x0B => self.lift_vex_round(prefix, opcode, bytes, pc, ctx),
            0x56 | 0x57 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_reduce(prefix, opcode, bytes, pc, ctx)
            }
            0x50 | 0x51 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_range(prefix, opcode, bytes, pc, ctx)
            }
            0x54 | 0x55 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_evex_fixup_imm(prefix, opcode, bytes, pc, ctx)
            }
            0x02 | 0x0C..=0x0E if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_immediate_blend(prefix, opcode, bytes, pc, ctx)
            }
            0x0F => self.lift_vec_palignr(prefix, bytes, pc, ctx),
            0x14..=0x17 => self.lift_vec_extract_0f3a(prefix, opcode, bytes, pc, ctx),
            0x20..=0x22 => self.lift_vec_insert_0f3a(prefix, opcode, bytes, pc, ctx),
            0x40 | 0x41 => self.lift_vex_dot_product(prefix, opcode, bytes, pc, ctx),
            0x42 => self.lift_vec_mpsadbw(prefix, bytes, pc, ctx),
            0x44 => self.lift_vec_pclmulqdq(prefix, bytes, pc, ctx),
            0xDF => self.lift_vec_aes_keygen(prefix, bytes, pc, ctx),
            0x4A..=0x4C if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_variable_blend(prefix, opcode, bytes, pc, ctx)
            }
            0xF0 if prefix.encoding == VecEncodingKind::Vex => {
                self.lift_vex_bmi2_rorx_dispatch(prefix, bytes, pc, ctx)
            }
            0xF0 if prefix.encoding == VecEncodingKind::Evex => {
                self.lift_apx_bmi_rorx(bytes, pc, ctx)
            }
            0xC2 => self.lift_vec_fp_compare(prefix, bytes, pc, ctx),
            _ => Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "VEX 0F3A".to_string(),
            }),
        }
    }
}
