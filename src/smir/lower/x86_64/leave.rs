//! Exact helper-backed lowering for long-mode x86 `LEAVE`.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86LeaveOp, X86LeaveWidth};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, OpWidth, SignExtend, SrcOperand, VReg, X86Reg,
};
use crate::smir::ir::{SmirBlock, X86InstructionBytes};
use crate::smir::lift::x86_64::decode_prefixes;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_AC_FLAG_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET,
    X86_GUEST_CS_L_OFFSET, X86_GUEST_EFER_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LeaveEncoding {
    pub(crate) width: X86LeaveWidth,
    pub(crate) requires_apx: bool,
    pub(crate) next_pc: u64,
}

/// Recover and validate one exact long-mode LEAVE source encoding and its
/// dedicated SMIR payload. Runtime and space complexity are O(N) and O(1),
/// respectively, where N is the number of operations in the containing block.
pub(crate) fn x86_leave_encoding(
    block: &SmirBlock,
    op_index: usize,
    instruction_bytes: &HashMap<(BlockId, u64), X86InstructionBytes>,
) -> Option<X86LeaveEncoding> {
    let op = block.ops.get(op_index)?;
    let OpKind::X86Leave(X86LeaveOp {
        width,
        requires_apx,
        next_pc,
    }) = &op.kind
    else {
        return None;
    };
    if op.x86_hint.is_some() {
        return None;
    }
    let source = instruction_bytes.get(&(block.id, op.guest_pc))?;
    let bytes = source.as_slice();
    let prefix = decode_prefixes(bytes).ok()?;
    if prefix.lock
        || prefix.rex2_m()
        || prefix.rex.is_some() && prefix.rex2.is_some()
        || bytes.get(prefix.cursor) != Some(&0xC9)
    {
        return None;
    }
    let expected_len = prefix.cursor.checked_add(1)?;
    if bytes.len() != expected_len || bytes.len() > 15 {
        return None;
    }
    let encoded_width = if prefix.operand_size_override && !prefix.rex_w() {
        X86LeaveWidth::W16
    } else {
        X86LeaveWidth::W64
    };
    let encoded_apx = prefix.rex2.is_some();
    let encoded_next_pc = op.guest_pc.checked_add(bytes.len() as u64)?;
    if block.ops.iter().enumerate().any(|(index, candidate)| {
        index != op_index
            && candidate.guest_pc >= op.guest_pc
            && candidate.guest_pc < encoded_next_pc
    }) {
        return None;
    }
    (*width == encoded_width && *requires_apx == encoded_apx && *next_pc == encoded_next_pc)
        .then_some(X86LeaveEncoding {
            width: encoded_width,
            requires_apx: encoded_apx,
            next_pc: encoded_next_pc,
        })
}

impl X86_64Lowerer {
    /// Continue only in 64-bit mode. A cached region is keyed by CS.L, but the
    /// dynamic guard also makes direct lowerer use and stale-region execution
    /// fail closed before LEAVE commits `RSP := RBP`.
    fn emit_x86_leave_long_mode_guard(&mut self, guest_pc: u64) -> Result<(), LowerError> {
        const EFER_LMA: i64 = 1 << 10;

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                crate::smir::lower::regalloc::PhysReg::Rax,
                X86_GUEST_EFER_OFFSET,
                DispSize::Auto,
                EFER_LMA,
                OpWidth::W64,
            );
        }
        let lma_disabled = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cs_l],0
        self.code.emit_u32(X86_GUEST_CS_L_OFFSET as u32);
        self.code.emit_u8(0);
        let enabled = self.emit_jcc_placeholder(X86Cond::Ne);

        self.patch_rel32_to_current(lma_disabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(guest_pc);

        self.patch_rel32_to_current(enabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }

    /// Deoptimize an active unaligned #AC case before LEAVE changes any guest
    /// register. Direct replay then raises the exact architectural fault.
    fn emit_x86_leave_alignment_guard(
        &mut self,
        guest_pc: u64,
        width: X86LeaveWidth,
    ) -> Result<(), LowerError> {
        const CR0_AM: i64 = 1 << 18;

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();

        let mut success = Vec::with_capacity(4);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                crate::smir::lower::regalloc::PhysReg::Rax,
                X86_GUEST_CR0_OFFSET,
                DispSize::Auto,
                CR0_AM,
                OpWidth::W64,
            );
        }
        success.push(self.emit_jcc_placeholder(X86Cond::E));
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],3
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(3);
        success.push(self.emit_jcc_placeholder(X86Cond::Ne));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                crate::smir::lower::regalloc::PhysReg::Rax,
                X86_GUEST_AC_FLAG_OFFSET,
                DispSize::Auto,
                1,
                OpWidth::W64,
            );
        }
        success.push(self.emit_jcc_placeholder(X86Cond::E));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                crate::smir::lower::regalloc::PhysReg::Rax,
                5 * 8,
                DispSize::Auto,
                i64::from(width.bytes() - 1),
                OpWidth::W64,
            );
        }
        success.push(self.emit_jcc_placeholder(X86Cond::E));

        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(guest_pc);

        for branch in success {
            self.patch_rel32_to_current(branch)?;
        }
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }

    /// Commit the helper-staged frame-pointer value without changing flags or
    /// any unrelated guest register. The caller's staged qword is at [RSP].
    fn emit_x86_leave_staged_rbp_commit(&mut self, width: X86LeaveWidth) -> Result<(), LowerError> {
        let width = match width {
            X86LeaveWidth::W16 => OpWidth::W16,
            X86LeaveWidth::W64 => OpWidth::W64,
        };
        self.code.emit_u8(0x50); // push guest RAX
        self.code.emit_u8(0x51); // push guest RCX; staged value is now [rsp+16]
        self.emit_load_state_ptr_rax();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rcx, PhysReg::Rsp, 16, width);
            emitter.emit_mov_mr(PhysReg::Rax, 5 * 8, PhysReg::Rcx, width);
            // Native RBP is the region frame pointer. Its saved guest word is
            // the value restored by the block epilogue.
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rcx, width);
        }
        self.code.emit_u8(0x59); // pop guest RCX
        self.code.emit_u8(0x58); // pop guest RAX
        Ok(())
    }

    /// Return true after lowering one exact LEAVE transaction. Guest RSP/RBP
    /// remain state-backed; no host `LEAVE` instruction is emitted.
    pub(crate) fn emit_x86_leave_if_present(
        &mut self,
        block: &SmirBlock,
        op_index: usize,
    ) -> Result<bool, LowerError> {
        if !matches!(block.ops[op_index].kind, OpKind::X86Leave(..)) {
            return Ok(false);
        }
        if !self.mem_helpers || !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "LEAVE requires JIT MMU helpers and precise deoptimization guards".into(),
            });
        }
        let encoding = x86_leave_encoding(block, op_index, &self.x86_instruction_bytes)
            .ok_or_else(|| LowerError::InvalidOperand {
                op: "X86Leave".into(),
                operand: "requires exact C9 provenance and matching payload".into(),
            })?;
        let guest_pc = block.ops[op_index].guest_pc;
        self.emit_x86_leave_long_mode_guard(guest_pc)?;
        if encoding.requires_apx {
            self.emit_x86_require_apx_guard(guest_pc)?;
        }

        self.emit_x86_leave_alignment_guard(guest_pc, encoding.width)?;
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
        // Stage the popped value in private host-stack space. The helper fault
        // path releases this frame before exiting, with GuestRegs untouched.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
        }
        self.emit_jit_mem_op(
            guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            &Address::Direct(rbp),
            encoding.width.mem_width(),
            SignExtend::Zero,
            32,
        )?;
        self.lower_state_backed_gpr_mov(rsp, &SrcOperand::Reg(rbp), OpWidth::W64)?;
        self.lower_state_backed_stack_gpr_alu(
            false,
            rsp,
            rsp,
            &SrcOperand::Imm(i64::from(encoding.width.bytes())),
            OpWidth::W64,
            FlagUpdate::None,
        )?;
        self.emit_x86_leave_staged_rbp_commit(encoding.width)?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        Ok(true)
    }
}
