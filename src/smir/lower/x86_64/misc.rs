//! Uncategorized lowering helpers

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap, X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FenceKind, FpRoundMode, GuestAddr, MemWidth,
    OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{
    CallTarget, SmirBlock, SmirFunction, Terminator, X86InstructionBytes,
    x86_evex_native_replay_spans,
};

use crate::smir::lower::regalloc::{PhysReg, RegAlloc, RegLocation};
use crate::smir::lower::{
    CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer,
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

impl X86_64Lowerer {
    /// Create a new x86_64 lowerer
    pub fn new() -> Self {
        X86_64Lowerer {
            code: CodeBuffer::with_capacity(4096),
            regalloc: RegAlloc::new(),
            block_offsets: HashMap::new(),
            relocations: Vec::new(),
            pending_jumps: Vec::new(),
            guest_base: 0,
            pcrel_adjust: true,
            jit_fault_deopt_guards: false,
            guest_pcrel_lea_immediates: false,
            block_guest_pcs: HashMap::new(),
            x86_instruction_bytes: HashMap::new(),
            pending_cond: None,
            native_exits: std::collections::HashMap::new(),
            native_exit_edges: std::collections::HashMap::new(),
            mem_helpers: false,
            preserve_vector_mem_helpers: false,
            preserve_vector_call_helpers: false,
            narrow_vector_opmask_helpers: false,
            call_helpers: false,
            epilogue_stack_patches: Vec::new(),
        }
    }

    pub(crate) fn try_lower_push_pop(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        if idx + 1 >= ops.len() {
            return Ok(None);
        }

        match (&ops[idx].kind, &ops[idx + 1].kind) {
            (
                OpKind::Sub {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                OpKind::Store {
                    src,
                    addr: Address::Direct(addr_base),
                    width: MemWidth::B8,
                },
            ) if *dst == *src1 && self.is_rsp(*dst) && self.is_rsp(*addr_base) => {
                if let VReg::Imm(val) = src {
                    let hint = ops[idx + 1].x86_hint;
                    let mut emitter = X86Emitter::new(&mut self.code);
                    match hint {
                        Some(X86OpHint::PushImm8) => {
                            emitter.emit_push_imm8(*val as i8);
                            return Ok(Some(2));
                        }
                        Some(X86OpHint::PushImm32) => {
                            emitter.emit_push_imm32(*val as i32);
                            return Ok(Some(2));
                        }
                        _ => {}
                    }
                }

                if matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Rsp))) {
                    return Ok(None);
                }
                let src_reg = self.get_reg(*src)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(src_reg);
                return Ok(Some(2));
            }
            (
                OpKind::Load {
                    dst,
                    addr: Address::Direct(addr_base),
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
                OpKind::Add {
                    dst: add_dst,
                    src1,
                    src2: SrcOperand::Imm(8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ) if *add_dst == *src1 && self.is_rsp(*add_dst) && self.is_rsp(*addr_base) => {
                if matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Rsp))) {
                    return Ok(None);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_pop(dst_reg);
                return Ok(Some(2));
            }
            _ => {}
        }

        Ok(None)
    }

    pub(crate) fn try_lower_vmem_binop(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        let (tmp, addr, width) = match ops.get(idx).map(|op| &op.kind) {
            Some(OpKind::VLoad { dst, addr, width }) => (*dst, addr, *width),
            _ => return Ok(None),
        };

        if width != VecWidth::V128 {
            return Ok(None);
        }

        let op = match ops.get(idx + 1) {
            Some(op) => op,
            None => return Ok(None),
        };

        match &op.kind {
            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } if *src2 == tmp && *dst == *src1 => {
                if *elem != VecElementType::I32 || *lanes != 4 {
                    return Ok(None);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                if !dst_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VAdd".to_string(),
                        operand: "destination must be vector register".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = VecEncoding { width, ..enc_hint };
                    self.emit_vec_mem(enc, dst_reg, Some(dst_reg), addr)?;
                } else {
                    if self.vec_requires_vex(&[dst_reg]) {
                        return Ok(None);
                    }
                    let prefix = self.sse_prefix(op.x86_hint);
                    let opcode = self.sse_opcode(op.x86_hint, 0xFE);
                    self.emit_sse_mov_mem(prefix, opcode, dst_reg, addr)?;
                }
                return Ok(Some(2));
            }
            OpKind::VMul {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } if *src2 == tmp && *dst == *src1 => {
                if *elem != VecElementType::I32 || *lanes != 4 {
                    return Ok(None);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                if !dst_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMul".to_string(),
                        operand: "destination must be vector register".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = VecEncoding { width, ..enc_hint };
                    self.emit_vec_mem(enc, dst_reg, Some(dst_reg), addr)?;
                } else {
                    if self.vec_requires_vex(&[dst_reg]) {
                        return Ok(None);
                    }
                    self.emit_sse_op38_mem(Some(0x66), 0x40, dst_reg, addr)?;
                }
                return Ok(Some(2));
            }
            _ => {}
        }

        Ok(None)
    }

    pub(crate) fn emit_flag_preserving_stack_pop8(&mut self) {
        // lea rsp,[rsp+8]  (48 8D 64 24 08)
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x8D);
        self.code.emit_u8(0x64);
        self.code.emit_u8(0x24);
        self.code.emit_u8(0x08);
    }

    /// Lower XSETBV as a state-backed control update followed by an immediate
    /// native-region handoff. Returning at `resume_pc` is required: changing
    /// XCR0 can enable or disable subsequent vector instructions, so code that
    /// was admitted under the entry state must not continue in the same region.
    /// Invalid state restores every input and returns at the XSETBV PC, letting
    /// the interpreter deliver #UD or #GP(0) with precise restart semantics.
    pub(crate) fn emit_xsetbv(&mut self, op: &SmirOp, resume_pc: u64) -> Result<(), LowerError> {
        let (selector, src_low, src_high) = match &op.kind {
            OpKind::X86XSetBv {
                selector,
                src_low,
                src_high,
            } => (*selector, *src_low, *src_high),
            _ => unreachable!("emit_xsetbv requires X86XSetBv"),
        };
        if self.get_reg(selector)? != PhysReg::Rcx
            || self.get_reg(src_low)? != PhysReg::Rax
            || self.get_reg(src_high)? != PhysReg::Rdx
        {
            return Err(LowerError::InvalidOperand {
                op: "X86XSetBv".to_string(),
                operand: "requires ECX selector and EDX:EAX source".to_string(),
            });
        }

        // [rsp+0]=RDX, [rsp+8]=RCX, [rsp+16]=RAX, [rsp+24]=RFLAGS.
        // The architectural instruction preserves every one of these values on
        // both success and fault, so all scratch computation is stack-backed.
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.code.emit_u8(0x51); // push rcx
        self.code.emit_u8(0x52); // push rdx
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x8B);
        self.code.emit_u8(0x45);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]

        let mut fault_branches = Vec::new();

        // CR4.OSXSAVE=0 raises #UD before selector/value validation.
        self.code.emit_u8(0xF7);
        self.code.emit_u8(0x80);
        self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
        self.code.emit_u32(1 << 18); // test dword [rax+cr4],1<<18
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        // In protected mode XSETBV requires CPL=0. Real mode ignores CS.RPL.
        self.code.emit_u8(0xF7);
        self.code.emit_u8(0x80);
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1); // test dword [rax+cr0],CR0.PE
        let privilege_ok = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x83);
        self.code.emit_u8(0xB8);
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0); // cmp qword [rax+cpl],0
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(privilege_ok)?;

        // Only XCR0 (ECX=0) is writable.
        self.code.emit_u8(0x83);
        self.code.emit_u8(0x7C);
        self.code.emit_u8(0x24);
        self.code.emit_u8(0x08);
        self.code.emit_u8(0); // cmp dword [rsp+8],0
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

        // RCX = zero-extended EDX:EAX candidate, reconstructed from snapshots.
        self.code.emit_u8(0x8B);
        self.code.emit_u8(0x4C);
        self.code.emit_u8(0x24);
        self.code.emit_u8(0x10); // mov ecx,[rsp+16]
        self.code.emit_u8(0x8B);
        self.code.emit_u8(0x14);
        self.code.emit_u8(0x24); // mov edx,[rsp]
        self.code.emit_u8(0x48);
        self.code.emit_u8(0xC1);
        self.code.emit_u8(0xE2);
        self.code.emit_u8(0x20); // shl rdx,32
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x09);
        self.code.emit_u8(0xD1); // or rcx,rdx

        // X87 must remain enabled.
        self.code.emit_u8(0xF6);
        self.code.emit_u8(0xC1);
        self.code.emit_u8(0x01); // test cl,1
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        // Supported state is x87/SSE/AVX/AVX-512 plus APX_F only when the
        // emulator exposes APX. TEST r64,imm32 sign-extends the immediate, and
        // both complements have identical ones in bits 63:32.
        self.code.emit_u8(0x83);
        self.code.emit_u8(0xB8);
        self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
        self.code.emit_u8(0); // cmp dword [rax+apx_enabled],0
        let no_apx = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x48);
        self.code.emit_u8(0xF7);
        self.code.emit_u8(0xC1);
        self.code.emit_u32(!(0xE7u32 | (1 << 19))); // test rcx,!supported_with_apx
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.code.emit_u8(0xE9);
        let supported = self.code.position();
        self.code.emit_u32(0);
        self.patch_rel32_to_current(no_apx)?;
        self.code.emit_u8(0x48);
        self.code.emit_u8(0xF7);
        self.code.emit_u8(0xC1);
        self.code.emit_u32(!0xE7u32); // test rcx,!supported_without_apx
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(supported)?;

        // AVX (bit 2) depends on SSE (bit 1).
        self.code.emit_u8(0xF6);
        self.code.emit_u8(0xC1);
        self.code.emit_u8(0x04); // test cl,4
        let avx_dependency_ok = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0xF6);
        self.code.emit_u8(0xC1);
        self.code.emit_u8(0x02); // test cl,2
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));
        self.patch_rel32_to_current(avx_dependency_ok)?;

        // AVX-512 bits 7:5 are all-or-none and require SSE+AVX.
        self.code.emit_u8(0x89);
        self.code.emit_u8(0xCA); // mov edx,ecx
        self.code.emit_u8(0x81);
        self.code.emit_u8(0xE2);
        self.code.emit_u32(0xE0); // and edx,0xE0
        let avx512_ok = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x81);
        self.code.emit_u8(0xFA);
        self.code.emit_u32(0xE0); // cmp edx,0xE0
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.code.emit_u8(0x89);
        self.code.emit_u8(0xCA); // mov edx,ecx
        self.code.emit_u8(0x83);
        self.code.emit_u8(0xE2);
        self.code.emit_u8(0x06); // and edx,6
        self.code.emit_u8(0x83);
        self.code.emit_u8(0xFA);
        self.code.emit_u8(0x06); // cmp edx,6
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(avx512_ok)?;

        // Commit XCR0, restore the complete architectural input state, and
        // force a new region at the next instruction under the new policy.
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x89);
        self.code.emit_u8(0x88);
        self.code.emit_u32(X86_GUEST_XCR0_OFFSET as u32); // mov [rax+xcr0],rcx
        self.code.emit_u8(0x5A);
        self.code.emit_u8(0x59);
        self.code.emit_u8(0x58);
        self.code.emit_u8(0x9D);
        self.emit_native_exit(resume_pc);

        // Every invalid path is non-committing and restarts at XSETBV.
        let fault = self.code.position();
        for branch in fault_branches {
            self.code
                .patch_i32(branch, (fault as i64 - (branch as i64 + 4)) as i32);
        }
        self.code.emit_u8(0x5A);
        self.code.emit_u8(0x59);
        self.code.emit_u8(0x58);
        self.code.emit_u8(0x9D);
        self.emit_native_exit(op.guest_pc);
        Ok(())
    }

    /// Fix up all pending jumps
    pub(crate) fn fixup_jumps(&mut self) -> Result<(), LowerError> {
        for (offset, target, kind) in self.pending_jumps.drain(..).collect::<Vec<_>>() {
            let target_offset =
                self.block_offsets
                    .get(&target)
                    .ok_or_else(|| LowerError::UndefinedLabel {
                        label: format!("block_{}", target.0),
                    })?;

            match kind {
                RelocKind::PcRel32 => {
                    let rel = (*target_offset as i64) - (offset as i64) - 4;
                    if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
                        return Err(LowerError::RelocationOutOfRange {
                            offset,
                            target: *target_offset,
                        });
                    }
                    self.code.patch_i32(offset, rel as i32);
                }
                RelocKind::PcRel8 => {
                    let rel = (*target_offset as i64) - (offset as i64) - 1;
                    if rel < -128 || rel > 127 {
                        return Err(LowerError::RelocationOutOfRange {
                            offset,
                            target: *target_offset,
                        });
                    }
                    self.code.data[offset] = rel as i8 as u8;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
