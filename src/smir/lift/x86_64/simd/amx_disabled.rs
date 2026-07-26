//! Terminal lifting for Intel AMX instructions in RAX's AMX-disabled profile.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::X86SsePrefix;
use crate::smir::lift::x86_64::{
    VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{ControlFlow, LiftError, LiftResult};

impl X86_64Lifter {
    /// Return whether an assigned `0F 38` VEX/EVEX cell belongs exclusively to
    /// Intel AMX in the deterministic guest profile.
    ///
    /// The VEX cells are specified by Intel SDM 325383-092 and Intel ISE
    /// 319433-059. The EVEX cells are the AMX-AVX512 forms in 319433-059.
    /// RAX enumerates neither their CPUID feature bits nor XCR0[18:17], so all
    /// assigned forms terminate as #UD before any operand can be observed.
    pub(crate) fn is_profile_disabled_amx_0f38(prefix: VecPrefix, opcode: u8) -> bool {
        if prefix.w {
            return false;
        }

        match prefix.encoding {
            VecEncodingKind::Vex if prefix.l_bits == 0 => matches!(
                (opcode, prefix.pp),
                (0x48, X86SsePrefix::OpSize)
                    | (
                        0x49,
                        X86SsePrefix::None | X86SsePrefix::OpSize | X86SsePrefix::Repne
                    )
                    | (0x4A, X86SsePrefix::OpSize | X86SsePrefix::Repne)
                    | (
                        0x4B,
                        X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
                    )
                    | (0x5C, X86SsePrefix::Rep | X86SsePrefix::Repne)
                    | (0x5E, _)
                    | (0x6C, X86SsePrefix::None | X86SsePrefix::OpSize)
            ),
            VecEncodingKind::Evex if prefix.l_bits == 2 => matches!(
                (opcode, prefix.pp),
                (0x4A, X86SsePrefix::OpSize | X86SsePrefix::Rep) | (0x6D, _)
            ),
            _ => false,
        }
    }

    /// Return whether an assigned `0F 3A` EVEX cell is an AMX-AVX512 form.
    pub(crate) fn is_profile_disabled_amx_0f3a(prefix: VecPrefix, opcode: u8) -> bool {
        prefix.encoding == VecEncodingKind::Evex
            && !prefix.w
            && prefix.l_bits == 2
            && matches!(
                (opcode, prefix.pp),
                (0x07, _) | (0x77, X86SsePrefix::Rep | X86SsePrefix::Repne)
            )
    }

    /// Decode an assigned AMX form through its complete instruction boundary,
    /// then construct the profile's terminal #UD without evaluating the
    /// decoded address or reading architectural state.
    pub(crate) fn lift_profile_disabled_amx(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        has_imm8: bool,
    ) -> Result<LiftResult, LiftError> {
        let modrm_offset = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor: modrm_offset,
            ..X86Prefix::default()
        };
        let modrm =
            decode_modrm(&bytes[modrm_offset..], &modrm_prefix, pc).map_err(
                |error| match error {
                    LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                        addr,
                        have: modrm_offset + have,
                        need: modrm_offset + need,
                    },
                    error => error,
                },
            )?;
        let mut bytes_consumed = modrm_offset + modrm.bytes_consumed;
        if has_imm8 {
            if bytes.len() <= bytes_consumed {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: bytes.len(),
                    need: bytes_consumed + 1,
                });
            }
            bytes_consumed += 1;
        }

        Ok(LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        })
    }
}
