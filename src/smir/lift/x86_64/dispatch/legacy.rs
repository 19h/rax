//! legacy.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {
    /// Lift the main instruction
    pub(crate) fn lift_insn_inner(
        &self,
        pc: u64,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        if bytes[0] == 0x62 {
            let is_apx_map4 = bytes.get(1).map_or(false, |p0| (p0 & 0x07) == 4);
            if is_apx_map4 || bytes.len() < 2 {
                return self.lift_apx_evex_map4(pc, bytes, ctx);
            }
            return self.lift_vex_evex(pc, bytes, ctx);
        }

        if matches!(bytes[0], 0xC4 | 0xC5) {
            return self.lift_vex_evex(pc, bytes, ctx);
        }

        // Decode prefixes
        let prefix = decode_prefixes(bytes)?;
        let opcode_bytes = &bytes[prefix.cursor..];

        if opcode_bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.cursor + 1,
            });
        }

        let prefixed_vec = matches!(opcode_bytes[0], 0x62 | 0xC4 | 0xC5);
        if prefixed_vec {
            return self.lift_prefixed_vec(pc, bytes, &prefix, ctx);
        }

        if prefix.rex2_m() {
            return self.lift_0f_opcode(opcode_bytes, &prefix, pc, ctx, 1);
        }

        let opcode = opcode_bytes[0];
        let after_opcode = &opcode_bytes[1..];

        match opcode {
            // XCHG rax, r64 / NOP / PAUSE (with REP prefix)
            0x90..=0x97 => {
                if opcode == 0x90 && prefix.rep_prefix == Some(0xF3) {
                    // PAUSE - treat as NOP for lifting
                    Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1))
                } else if opcode == 0x90 && prefix.rex_b() == 0 {
                    // 90 (including 66/REX.W 90) is the architectural NOP
                    // alias, not a 32-bit self-write that clears EAX[63:32].
                    Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1))
                } else {
                    self.lift_xchg_rax(
                        opcode,
                        &X86Prefix {
                            cursor: prefix.cursor + 1,
                            ..prefix
                        },
                        pc,
                    )
                }
            }

            // CMC/CLC/STC
            0xF5 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::CmcCF)],
                prefix.cursor + 1,
            )),
            0xF8 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetCF { value: false })],
                prefix.cursor + 1,
            )),
            0xF9 => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetCF { value: true })],
                prefix.cursor + 1,
            )),
            0xFC => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetDF { value: false })],
                prefix.cursor + 1,
            )),
            0xFD => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::SetDF { value: true })],
                prefix.cursor + 1,
            )),
            0xCC => Ok(LiftResult::fallthrough(
                vec![SmirOp::new(OpId(0), pc, OpKind::Breakpoint)],
                prefix.cursor + 1,
            )),

            // HLT
            0xF4 => Ok(LiftResult {
                ops: vec![],
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::Halt,
                },
                branch_targets: vec![],
            }),

            // Instructions architecturally invalid in 64-bit mode. Model the
            // guaranteed #UD explicitly rather than reporting missing support.
            0x27 | 0x2F | 0x37 | 0x3F // DAA/DAS/AAA/AAS
            | 0x60 | 0x61             // PUSHA/POPA
            | 0x82                    // legacy Group-1 alias
            | 0x9A | 0xEA             // far CALL/JMP immediate
            | 0xCE                    // INTO
            | 0xD4 => Ok(LiftResult { // AAM (D5 is APX REX2 in this decoder)
                ops: vec![],
                bytes_consumed: prefix.cursor + 1,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            }),

            // Two-byte opcode prefix
            0x0F => self.lift_0f_opcode(after_opcode, &prefix, pc, ctx, 2),

            // Control flow
            0xEB => self.lift_jmp_rel8(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE9 => self.lift_jmp_rel32(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE8 => self.lift_call_rel32(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC2 => self.lift_ret_imm16(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC3 => self.lift_ret(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xCA | 0xCB => self.lift_far_ret(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xC8 => self.lift_enter(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC9 => self.lift_leave(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0x99 => self.lift_cwd_cdq_cqo(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0x98 => self.lift_cbw_cwde_cdqe(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0x9C | 0x9D => self.lift_stack_flags(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x9E | 0x9F => self.lift_ah_flags(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            // WAIT/FWAIT has no state effect in the base emulator profile.
            0x9B if !prefix.lock => Ok(LiftResult::fallthrough(vec![], prefix.cursor + 1)),
            0x9B => Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..(prefix.cursor + 1).min(bytes.len())].to_vec(),
            }),
            0x70..=0x7F => self.lift_jcc_rel8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xE0..=0xE3 => self.lift_loop_rel8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xA1 if prefix.rex2.is_some() => self.lift_jmp_abs(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xA0..=0xA3 => self.lift_mov_moffs(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),

            // Data movement
            0xB0..=0xB7 => self.lift_mov_r8_imm8(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xB8..=0xBF => self.lift_mov_r_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x88..=0x8B => self.lift_mov_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x86 | 0x87 => self.lift_xchg_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xC6 | 0xC7 => self.lift_mov_rm_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x8D => self.lift_lea(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x63 => self.lift_movsxd(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x50..=0x57 => self.lift_push_r64(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x58..=0x5F => self.lift_pop_r64(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x8F => self.lift_pop_rm(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x6A | 0x68 => self.lift_push_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xF6 | 0xF7 => self.lift_group3(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x69 | 0x6B => self.lift_imul_rmi(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Arithmetic
            0x00..=0x05 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // ADD
            0x08..=0x0D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // OR
            0x10..=0x15 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // ADC
            0x18..=0x1D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // SBB
            0x20..=0x25 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // AND
            0x28..=0x2D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // SUB
            0x30..=0x35 => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0x38..=0x3D => self.lift_arith(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ), // CMP

            // Group 1 immediate (80/81/83)
            0x80 | 0x81 | 0x83 => self.lift_group1_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Logic
            0x84 | 0x85 => self.lift_test_rm_r(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xA8 | 0xA9 => self.lift_test_acc_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),

            // String ops
            0xA4..=0xA7 | 0xAA..=0xAF => self.lift_string(
                opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xD7 => self.lift_xlat(
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xD8..=0xDF => self.lift_x87_escape(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (C0/C1) - immediate
            0xC0 | 0xC1 => self.lift_shift_imm(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (D0/D1) - count = 1
            0xD0 | 0xD1 => self.lift_shift_one(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Shift/rotate group (D2/D3) - count in CL
            0xD2 | 0xD3 => self.lift_shift_cl(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // Group 5 (FF)
            0xFE => self.lift_group4(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),
            0xFF => self.lift_group5(
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
                ctx,
            ),

            // I/O port instructions
            0xE4 | 0xE5 | 0xEC | 0xED => self.lift_in(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),
            0xE6 | 0xE7 | 0xEE | 0xEF => self.lift_out(
                opcode,
                after_opcode,
                &X86Prefix {
                    cursor: prefix.cursor + 1,
                    ..prefix
                },
                pc,
            ),

            // Unsupported - return error with mnemonic
            _ => {
                if self.strict {
                    Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("0x{:02X}", opcode),
                    })
                } else {
                    // In non-strict mode, emit a Nop and continue
                    Ok(LiftResult::fallthrough(
                        vec![SmirOp::new(OpId(0), pc, OpKind::Nop)],
                        prefix.cursor + 1,
                    ))
                }
            }
        }
    }
}
