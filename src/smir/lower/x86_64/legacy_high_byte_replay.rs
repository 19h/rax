//! Exact scalar replay wrappers for legacy AH/CH/DH/BH encodings.

use super::*;
use crate::smir::ir::{
    X86LegacyHighByteCrc32Replay, X86LegacyHighByteGroup2Kind, X86LegacyHighByteGroup2Replay,
    X86NativeReplaySpan,
};

const X86_STATUS_RFLAGS: i64 = 0x08D5;
const ROTATE_UNCHANGED_RFLAGS: i64 = 0x00D4;
const CF: i64 = 1;
const AF: i64 = 1 << 4;
const OF: i64 = 1 << 11;

impl X86_64Lowerer {
    /// Execute CRC32 with an AH/CH/DH/BH source and a guest ESP/EBP destination.
    /// The destination is loaded into ESI and committed through `GuestRegs`,
    /// keeping native RSP/RBP inaccessible while preserving every guest GPR and
    /// RFLAGS. Identity-mapped destinations bypass this wrapper and replay exactly.
    fn emit_legacy_high_byte_crc32_replay(&mut self, replay: X86LegacyHighByteCrc32Replay) {
        debug_assert!(matches!(replay.destination, 4 | 5));

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(PhysReg::Rsi);
            emitter.emit_push(PhysReg::Rdi);
            emitter.emit_mov_rm(
                PhysReg::Rdi,
                PhysReg::Rbp,
                X86_STATE_PTR_AT_RBP,
                OpWidth::W64,
            );
            emitter.emit_mov_rm(
                PhysReg::Rsi,
                PhysReg::Rdi,
                i32::from(replay.destination) * 8,
                OpWidth::W32,
            );
        }
        self.code
            .emit_bytes(replay.state_backed_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(
                PhysReg::Rdi,
                i32::from(replay.destination) * 8,
                PhysReg::Rsi,
                OpWidth::W64,
            );
            if replay.destination == 5 {
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rsi, OpWidth::W64);
            }
            emitter.emit_pop(PhysReg::Rdi);
            emitter.emit_pop(PhysReg::Rsi);
        }
    }

    /// Merge selected incoming status bits into the saved native image, clear
    /// deterministic outputs, optionally reconstruct CF from the original
    /// high byte, and restore the guest-visible image. The active stack layout
    /// is saved RDI, native RFLAGS, incoming RFLAGS, original parent GPR at
    /// offsets 0, 8, 16, and 24 bytes respectively.
    fn emit_finish_legacy_high_byte_group2_status(
        &mut self,
        preserve_rflags: i64,
        clear_rflags: i64,
        reconstructed_cf_bit: Option<u8>,
    ) {
        if preserve_rflags != 0 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 16, OpWidth::W64);
            emitter.emit_and_ri(PhysReg::Rdi, preserve_rflags, OpWidth::W64);
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                8,
                DispSize::Auto,
                !preserve_rflags,
                OpWidth::W64,
            );
            emitter.emit_alu_mem_disp(
                0x08,
                PhysReg::Rdi,
                PhysReg::Rsp,
                8,
                DispSize::Auto,
                OpWidth::W64,
                X86AluEncoding::RmReg,
            );
        }
        if clear_rflags != 0 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mi_disp(
                4,
                PhysReg::Rsp,
                8,
                DispSize::Auto,
                !clear_rflags,
                OpWidth::W64,
            );
        }
        if let Some(bit) = reconstructed_cf_bit {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_alu_mi_disp(4, PhysReg::Rsp, 8, DispSize::Auto, !CF, OpWidth::W64);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rsp, 24, OpWidth::W64);
            emitter.emit_shr_ri(PhysReg::Rdi, 8 + bit, OpWidth::W64);
            emitter.emit_and_ri(PhysReg::Rdi, CF, OpWidth::W64);
            emitter.emit_alu_mem_disp(
                0x08,
                PhysReg::Rdi,
                PhysReg::Rsp,
                8,
                DispSize::Auto,
                OpWidth::W64,
                X86AluEncoding::RmReg,
            );
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_pop(PhysReg::Rdi);
        }
        self.code.emit_u8(0x9D); // popfq: merged native image
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            // Discard incoming RFLAGS and the original parent snapshot without
            // disturbing the restored status image.
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
    }

    fn emit_legacy_high_byte_group2_status_case(
        &mut self,
        kind: X86LegacyHighByteGroup2Kind,
        count: u8,
    ) {
        if count == 0 {
            self.emit_finish_legacy_high_byte_group2_status(X86_STATUS_RFLAGS, 0, None);
            return;
        }

        match kind {
            X86LegacyHighByteGroup2Kind::Rol
            | X86LegacyHighByteGroup2Kind::Ror
            | X86LegacyHighByteGroup2Kind::Rcl
            | X86LegacyHighByteGroup2Kind::Rcr => {
                self.emit_finish_legacy_high_byte_group2_status(
                    ROTATE_UNCHANGED_RFLAGS | if count == 1 { 0 } else { OF },
                    0,
                    None,
                );
            }
            X86LegacyHighByteGroup2Kind::Sal if count == 8 => {
                self.emit_finish_legacy_high_byte_group2_status(0, AF | OF, Some(0));
            }
            X86LegacyHighByteGroup2Kind::Sal if count > 8 => {
                self.emit_finish_legacy_high_byte_group2_status(0, AF | CF | OF, None);
            }
            X86LegacyHighByteGroup2Kind::Shl | X86LegacyHighByteGroup2Kind::Shr if count == 8 => {
                let bit = if kind == X86LegacyHighByteGroup2Kind::Shl {
                    0
                } else {
                    7
                };
                self.emit_finish_legacy_high_byte_group2_status(AF, OF, Some(bit));
            }
            X86LegacyHighByteGroup2Kind::Shl | X86LegacyHighByteGroup2Kind::Shr if count > 8 => {
                self.emit_finish_legacy_high_byte_group2_status(AF, CF | OF, None);
            }
            X86LegacyHighByteGroup2Kind::Sar if count >= 8 => {
                self.emit_finish_legacy_high_byte_group2_status(AF, OF, Some(7));
            }
            X86LegacyHighByteGroup2Kind::Shl
            | X86LegacyHighByteGroup2Kind::Shr
            | X86LegacyHighByteGroup2Kind::Sar => {
                self.emit_finish_legacy_high_byte_group2_status(
                    AF,
                    if count == 1 { 0 } else { OF },
                    None,
                );
            }
            X86LegacyHighByteGroup2Kind::Sal => {
                self.emit_finish_legacy_high_byte_group2_status(
                    0,
                    AF | if count == 1 { 0 } else { OF },
                    None,
                );
            }
        }
    }

    fn emit_legacy_high_byte_jump_placeholder(&mut self) -> usize {
        self.code.emit_u8(0xE9);
        let displacement = self.code.position();
        self.code.emit_u32(0);
        displacement
    }

    fn emit_dynamic_legacy_high_byte_group2_status(
        &mut self,
        kind: X86LegacyHighByteGroup2Kind,
    ) -> Result<(), LowerError> {
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rcx, OpWidth::W64);
            emitter.emit_and_ri(PhysReg::Rdi, 0x1F, OpWidth::W64);
            emitter.emit_test_rr(PhysReg::Rdi, PhysReg::Rdi, OpWidth::W64);
        }
        let count_zero = self.emit_jcc_placeholder(X86Cond::E);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_cmp_ri(PhysReg::Rdi, 1, OpWidth::W64);
        }
        let count_one = self.emit_jcc_placeholder(X86Cond::E);

        let is_shift = matches!(
            kind,
            X86LegacyHighByteGroup2Kind::Shl
                | X86LegacyHighByteGroup2Kind::Sal
                | X86LegacyHighByteGroup2Kind::Shr
                | X86LegacyHighByteGroup2Kind::Sar
        );
        let (count_boundary, count_oversized) = if is_shift {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_cmp_ri(PhysReg::Rdi, 8, OpWidth::W64);
            }
            (
                Some(self.emit_jcc_placeholder(X86Cond::E)),
                Some(self.emit_jcc_placeholder(X86Cond::A)),
            )
        } else {
            (None, None)
        };

        self.emit_legacy_high_byte_group2_status_case(kind, 2);
        let multi_done = self.emit_legacy_high_byte_jump_placeholder();

        self.patch_rel32_to_current(count_one)?;
        self.emit_legacy_high_byte_group2_status_case(kind, 1);
        let one_done = self.emit_legacy_high_byte_jump_placeholder();

        self.patch_rel32_to_current(count_zero)?;
        self.emit_legacy_high_byte_group2_status_case(kind, 0);
        if let (Some(count_boundary), Some(count_oversized)) = (count_boundary, count_oversized) {
            let zero_done = self.emit_legacy_high_byte_jump_placeholder();

            self.patch_rel32_to_current(count_boundary)?;
            self.emit_legacy_high_byte_group2_status_case(kind, 8);
            let boundary_done = self.emit_legacy_high_byte_jump_placeholder();

            self.patch_rel32_to_current(count_oversized)?;
            self.emit_legacy_high_byte_group2_status_case(kind, 9);
            self.patch_rel32_to_current(zero_done)?;
            self.patch_rel32_to_current(boundary_done)?;
        }
        self.patch_rel32_to_current(multi_done)?;
        self.patch_rel32_to_current(one_done)?;
        Ok(())
    }

    fn emit_legacy_high_byte_group2_replay(
        &mut self,
        replay: X86LegacyHighByteGroup2Replay,
    ) -> Result<(), LowerError> {
        let parent = match replay.parent {
            0 => PhysReg::Rax,
            1 => PhysReg::Rcx,
            2 => PhysReg::Rdx,
            3 => PhysReg::Rbx,
            invalid => {
                return Err(LowerError::InvalidOperand {
                    op: "legacy high-byte Group 2 replay".to_string(),
                    operand: format!("invalid parent GPR index {invalid}"),
                });
            }
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(parent); // original parent for boundary CF
        }
        self.code.emit_u8(0x9C); // pushfq: complete incoming image
        self.code
            .emit_bytes(replay.canonical_instruction.as_slice());
        self.code.emit_u8(0x9C); // pushfq: native Group 2 image
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(PhysReg::Rdi); // guest-preserved status scratch
        }

        if let Some(raw_count) = replay.raw_count {
            self.emit_legacy_high_byte_group2_status_case(replay.kind, raw_count & 0x1F);
        } else {
            self.emit_dynamic_legacy_high_byte_group2_status(replay.kind)?;
        }
        Ok(())
    }

    /// Emit one validated legacy high-byte wrapper. Returns `false` when
    /// `span` belongs to another replay family.
    pub(crate) fn try_emit_legacy_high_byte_replay(
        &mut self,
        span: &X86NativeReplaySpan,
    ) -> Result<bool, LowerError> {
        if let Some(replay) = span.instruction.legacy_high_byte_setcc_replay() {
            // Intel defines ModR/M.reg as ignored and the operand as fixed at
            // 8 bits. Use the validated prefix-free /0 equivalent because
            // some translated x86-64 hosts raise #UD for redundant prefixes
            // or nonzero reg images even though they are architecturally valid.
            self.code
                .emit_bytes(replay.canonical_instruction.as_slice());
            return Ok(true);
        }

        if let Some(replay) = span.instruction.legacy_high_byte_crc32_replay() {
            if matches!(replay.destination, 4 | 5) {
                self.emit_legacy_high_byte_crc32_replay(replay);
            } else {
                self.code.emit_bytes(span.instruction.as_slice());
            }
            return Ok(true);
        }

        if let Some(destination) = span
            .instruction
            .legacy_high_byte_cmpxchg_destination_index()
        {
            // Intel defines CMPXCHG's arithmetic flags as AL minus the
            // destination. Some translated x86-64 hosts instead publish the
            // reverse subtraction for AH/CH/DH/BH encodings. CMPXCHG consumes
            // no flags, so compute and preserve the specified image around the
            // exact state transition.
            self.code.emit_bytes(&[0x3A, 0xC0 | destination]); // cmp al,r/m8
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_bytes(span.instruction.as_slice());
            self.code.emit_u8(0x9D); // popfq
            return Ok(true);
        }

        let Some(replay) = span.instruction.legacy_high_byte_group2_replay() else {
            return Ok(false);
        };
        self.emit_legacy_high_byte_group2_replay(replay)?;
        Ok(true)
    }
}
