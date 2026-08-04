//! Ordered dispatch for exact helper-backed native vector-memory replays.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::VReg;
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Try the exact vector-memory replay families in their established
    /// first-match order.
    ///
    /// Each candidate is fail-closed and returns the complete contiguous SMIR
    /// span it owns. Keeping this ordering in one semantic module prevents the
    /// main control lowerer from accumulating instruction-family dispatch.
    pub(crate) fn try_lower_jit_exact_vector_memory_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        macro_rules! try_replay {
            ($method:ident) => {
                if let Some(consumed) =
                    self.$method(block, index, virtual_definitions, virtual_uses)?
                {
                    return Ok(Some(consumed));
                }
            };
        }

        try_replay!(try_lower_jit_vpcom_memory_source);
        try_replay!(try_lower_jit_vbit_select_memory_source);
        try_replay!(try_lower_jit_xop_memory_source);
        try_replay!(try_lower_jit_evex_bf16_memory_source);
        try_replay!(try_lower_jit_evex_fp_interleave_memory_source);
        try_replay!(try_lower_jit_evex_fp_shuffle_memory_source);
        try_replay!(try_lower_jit_evex_chunk_shuffle_memory_source);
        try_replay!(try_lower_jit_evex_bw_shuffle_madd_memory_source);
        try_replay!(try_lower_jit_evex_integer_arithmetic_memory_source);
        try_replay!(try_lower_jit_evex_integer_pack_memory_source);
        try_replay!(try_lower_jit_evex_integer_interleave_memory_source);
        try_replay!(try_lower_jit_evex_packed_integer_mask_memory_source);
        try_replay!(try_lower_jit_evex_integer_minmax_memory_source);
        try_replay!(try_lower_jit_evex_logic_memory_source);
        try_replay!(try_lower_jit_evex_masked_logic_memory_source);
        try_replay!(try_lower_jit_evex_multishift_memory_source);
        try_replay!(try_lower_jit_evex_full_permute_memory_source);
        try_replay!(try_lower_jit_evex_two_table_permute_memory_source);
        try_replay!(try_lower_jit_evex_variable_permute_memory_source);
        try_replay!(try_lower_jit_evex_broadcast_interleave_memory_source);
        try_replay!(try_lower_jit_evex_broadcast_logic_memory_source);
        try_replay!(try_lower_jit_evex_packed_fp16_arithmetic_memory_source);
        try_replay!(try_lower_jit_evex_packed_fp_arithmetic_memory_source);
        try_replay!(try_lower_jit_evex_packed_fp_compare_memory_source);
        try_replay!(try_lower_jit_evex_fixup_imm_memory_source);
        try_replay!(try_lower_jit_evex_packed_funnel_shift_memory_source);
        try_replay!(try_lower_jit_evex_packed_rotate_memory_source);
        try_replay!(try_lower_jit_evex_packed_variable_shift_memory_source);
        try_replay!(try_lower_jit_evex_range_memory_source);
        try_replay!(try_lower_jit_evex_scale_f_memory_source);
        try_replay!(try_lower_jit_evex_ternary_logic_memory_source);
        try_replay!(try_lower_jit_evex_shared_count_shift_memory_source);
        try_replay!(try_lower_jit_evex_alignr_memory_source);
        try_replay!(try_lower_jit_evex_vector_align_memory_source);
        try_replay!(try_lower_jit_evex_mask_blend_memory_source);
        try_replay!(try_lower_jit_evex_scalar_fma3_memory_source);
        try_replay!(try_lower_jit_evex_packed_fma3_memory_source);
        try_replay!(try_lower_jit_vex_fma4_memory_source);
        try_replay!(try_lower_jit_vex_vpermil2_memory_source);
        try_replay!(try_lower_jit_vex_sm3_sm4_memory_source);
        try_replay!(try_lower_jit_vex_packed_string_memory_source);
        try_replay!(try_lower_jit_vex_masked_memory);
        try_replay!(try_lower_jit_vpclmulqdq_memory_source);
        Ok(None)
    }
}
