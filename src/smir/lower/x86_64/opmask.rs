//! Native lowering for VEX-encoded AVX-512 opmask instructions.

use crate::smir::ir::ops::{
    OpKind, SmirOp, X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource,
    X86OpmaskOp, X86OpmaskShiftKind, X86OpmaskTestKind, X86SsePrefix, X86VecMap,
};
use crate::smir::ir::types::{ArchReg, OpWidth, SignExtend, VReg, X86Reg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

use super::{X86_64Lowerer, X86Emitter};

pub(crate) fn x86_opmask_native_shape_valid(op: &X86OpmaskOp) -> bool {
    let k = |reg: VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::K(index))) if index < 8);
    let gpr = |reg: VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index < 16));
    let width = |width: OpWidth| {
        matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        )
    };

    match op {
        X86OpmaskOp::MoveToMask { dst, src, width: w } => {
            k(*dst)
                && width(*w)
                && match src {
                    X86OpmaskMoveSource::Mask(src) => k(*src),
                    X86OpmaskMoveSource::Gpr(src) => gpr(*src),
                    X86OpmaskMoveSource::Memory(_) => true,
                }
        }
        X86OpmaskOp::MoveFromMask { dst, src, width: w } => {
            k(*src)
                && width(*w)
                && match dst {
                    X86OpmaskMoveDestination::Gpr(dst) => gpr(*dst),
                    X86OpmaskMoveDestination::Memory(_) => true,
                }
        }
        X86OpmaskOp::Not {
            dst, src, width: w, ..
        }
        | X86OpmaskOp::Shift {
            dst, src, width: w, ..
        } => k(*dst) && k(*src) && width(*w),
        X86OpmaskOp::Binary {
            dst,
            src1,
            src2,
            width: w,
            ..
        } => k(*dst) && k(*src1) && k(*src2) && width(*w),
        X86OpmaskOp::Unpack {
            dst,
            src1,
            src2,
            width,
        } => {
            k(*dst)
                && k(*src1)
                && k(*src2)
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        }
        X86OpmaskOp::Test {
            src1,
            src2,
            width: w,
            ..
        } => k(*src1) && k(*src2) && width(*w),
    }
}

pub(crate) fn x86_opmask_needs_avx512dq(op: &X86OpmaskOp) -> bool {
    op.width() == OpWidth::W8
        || matches!(
            op,
            X86OpmaskOp::Binary {
                kind: X86OpmaskBinaryKind::Add,
                width: OpWidth::W16,
                ..
            } | X86OpmaskOp::Test {
                kind: X86OpmaskTestKind::And,
                width: OpWidth::W16,
                ..
            }
        )
}

impl X86_64Lowerer {
    pub(crate) fn lower_opmask(&mut self, op: &SmirOp) -> Result<bool, LowerError> {
        let OpKind::X86Opmask(opmask) = &op.kind else {
            return Ok(false);
        };
        if !x86_opmask_native_shape_valid(opmask) {
            return Err(LowerError::InvalidOperand {
                op: "X86Opmask".to_string(),
                operand: format!("invalid architectural operand shape: {opmask:?}"),
            });
        }

        match opmask {
            X86OpmaskOp::MoveToMask { dst, src, width } => {
                let dst = Self::opmask_index(*dst).unwrap();
                match src {
                    X86OpmaskMoveSource::Mask(src) => self.emit_opmask_rr(
                        X86VecMap::Map0F,
                        Self::opmask_logic_prefix(*width),
                        Self::opmask_w(*width),
                        false,
                        0,
                        0x90,
                        dst,
                        Self::opmask_index(*src).unwrap(),
                    ),
                    X86OpmaskMoveSource::Gpr(src) => {
                        self.emit_opmask_gpr_to_mask(dst, *src, *width)?;
                    }
                    X86OpmaskMoveSource::Memory(addr) => {
                        self.emit_opmask_memory(op.guest_pc, true, dst, addr, *width)?;
                    }
                }
            }
            X86OpmaskOp::MoveFromMask { dst, src, width } => {
                let src = Self::opmask_index(*src).unwrap();
                match dst {
                    X86OpmaskMoveDestination::Gpr(dst) => {
                        self.emit_opmask_mask_to_gpr(src, *dst, *width)?;
                    }
                    X86OpmaskMoveDestination::Memory(addr) => {
                        self.emit_opmask_memory(op.guest_pc, false, src, addr, *width)?;
                    }
                }
            }
            X86OpmaskOp::Not { dst, src, width } => self.emit_opmask_rr(
                X86VecMap::Map0F,
                Self::opmask_logic_prefix(*width),
                Self::opmask_w(*width),
                false,
                0,
                0x44,
                Self::opmask_index(*dst).unwrap(),
                Self::opmask_index(*src).unwrap(),
            ),
            X86OpmaskOp::Binary {
                kind,
                dst,
                src1,
                src2,
                width,
            } => {
                let opcode = match kind {
                    X86OpmaskBinaryKind::And => 0x41,
                    X86OpmaskBinaryKind::AndNot => 0x42,
                    X86OpmaskBinaryKind::Or => 0x45,
                    X86OpmaskBinaryKind::Xnor => 0x46,
                    X86OpmaskBinaryKind::Xor => 0x47,
                    X86OpmaskBinaryKind::Add => 0x4A,
                };
                self.emit_opmask_rr(
                    X86VecMap::Map0F,
                    Self::opmask_logic_prefix(*width),
                    Self::opmask_w(*width),
                    true,
                    Self::opmask_index(*src1).unwrap(),
                    opcode,
                    Self::opmask_index(*dst).unwrap(),
                    Self::opmask_index(*src2).unwrap(),
                );
            }
            X86OpmaskOp::Unpack {
                dst,
                src1,
                src2,
                width,
            } => {
                let (pp, w) = match width {
                    OpWidth::W16 => (X86SsePrefix::OpSize, false),
                    OpWidth::W32 => (X86SsePrefix::None, false),
                    OpWidth::W64 => (X86SsePrefix::None, true),
                    _ => unreachable!("validated KUNPCK width"),
                };
                self.emit_opmask_rr(
                    X86VecMap::Map0F,
                    pp,
                    w,
                    true,
                    Self::opmask_index(*src1).unwrap(),
                    0x4B,
                    Self::opmask_index(*dst).unwrap(),
                    Self::opmask_index(*src2).unwrap(),
                );
            }
            X86OpmaskOp::Shift {
                kind,
                dst,
                src,
                width,
                count,
            } => {
                let opcode = match (kind, width) {
                    (X86OpmaskShiftKind::Right, OpWidth::W8 | OpWidth::W16) => 0x30,
                    (X86OpmaskShiftKind::Right, OpWidth::W32 | OpWidth::W64) => 0x31,
                    (X86OpmaskShiftKind::Left, OpWidth::W8 | OpWidth::W16) => 0x32,
                    (X86OpmaskShiftKind::Left, OpWidth::W32 | OpWidth::W64) => 0x33,
                    _ => unreachable!("validated KSHIFT width"),
                };
                self.emit_opmask_rr(
                    X86VecMap::Map0F3A,
                    X86SsePrefix::OpSize,
                    matches!(width, OpWidth::W16 | OpWidth::W64),
                    false,
                    0,
                    opcode,
                    Self::opmask_index(*dst).unwrap(),
                    Self::opmask_index(*src).unwrap(),
                );
                self.code.emit_u8(*count);
            }
            X86OpmaskOp::Test {
                kind,
                src1,
                src2,
                width,
            } => self.emit_opmask_rr(
                X86VecMap::Map0F,
                Self::opmask_logic_prefix(*width),
                Self::opmask_w(*width),
                false,
                0,
                if *kind == X86OpmaskTestKind::And {
                    0x99
                } else {
                    0x98
                },
                Self::opmask_index(*src1).unwrap(),
                Self::opmask_index(*src2).unwrap(),
            ),
        }

        Ok(true)
    }

    fn emit_opmask_gpr_to_mask(
        &mut self,
        dst: u8,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let index = Self::x86_gpr_index(src).unwrap();
        if matches!(index, 4 | 5) {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rax);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rdx,
                    PhysReg::Rax,
                    i32::from(index) * 8,
                    if width == OpWidth::W64 {
                        OpWidth::W64
                    } else {
                        OpWidth::W32
                    },
                );
            }
            self.emit_opmask_rr(
                X86VecMap::Map0F,
                Self::opmask_gpr_prefix(width),
                width == OpWidth::W64,
                false,
                0,
                0x92,
                dst,
                PhysReg::Rdx.encoding(),
            );
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_pop(PhysReg::Rdx);
                emitter.emit_pop(PhysReg::Rax);
            }
            return Ok(());
        }

        self.emit_opmask_rr(
            X86VecMap::Map0F,
            Self::opmask_gpr_prefix(width),
            width == OpWidth::W64,
            false,
            0,
            0x92,
            dst,
            index,
        );
        Ok(())
    }

    fn emit_opmask_mask_to_gpr(
        &mut self,
        src: u8,
        dst: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let index = Self::x86_gpr_index(dst).unwrap();
        if matches!(index, 4 | 5) {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rax);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.emit_opmask_mask_to_gpr_rr(src, PhysReg::Rdx.encoding(), width);
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(
                    PhysReg::Rax,
                    i32::from(index) * 8,
                    PhysReg::Rdx,
                    OpWidth::W64,
                );
                if index == 5 {
                    emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
                }
                emitter.emit_pop(PhysReg::Rdx);
                emitter.emit_pop(PhysReg::Rax);
            }
            return Ok(());
        }

        self.emit_opmask_mask_to_gpr_rr(src, index, width);
        Ok(())
    }

    pub(crate) fn emit_opmask_mask_to_rax64(&mut self, src: u8) {
        debug_assert!(src <= 7, "architectural opmask index");
        self.emit_opmask_mask_to_gpr_rr(src, 0, OpWidth::W64);
    }

    fn emit_opmask_mask_to_gpr_rr(&mut self, src: u8, dst: u8, width: OpWidth) {
        self.emit_opmask_vex_prefix(
            X86VecMap::Map0F,
            Self::opmask_gpr_prefix(width),
            width == OpWidth::W64,
            false,
            0,
            dst >= 8,
            false,
        );
        self.code.emit_u8(0x93);
        self.code.emit_u8(0xC0 | ((dst & 7) << 3) | (src & 7));
    }

    fn emit_opmask_memory(
        &mut self,
        guest_pc: u64,
        is_load: bool,
        mask: u8,
        addr: &crate::smir::ir::types::Address,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if !self.mem_helpers || !self.preserve_vector_mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "KMOV memory form requires vector-preserving JIT memory helpers".to_string(),
            });
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        if is_load {
            self.emit_jit_mem_op(
                guest_pc,
                true,
                None,
                Some(16),
                None,
                None,
                None,
                addr,
                width.to_mem_width(),
                SignExtend::Zero,
                16,
            )?;
            self.emit_opmask_rsp_mem(true, mask, width);
        } else {
            self.emit_opmask_rsp_mem(false, mask, width);
            self.emit_jit_mem_op(
                guest_pc,
                false,
                None,
                None,
                None,
                None,
                Some(16),
                addr,
                width.to_mem_width(),
                SignExtend::Zero,
                16,
            )?;
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(())
    }

    fn emit_opmask_rsp_mem(&mut self, is_load: bool, mask: u8, width: OpWidth) {
        self.emit_opmask_vex_prefix(
            X86VecMap::Map0F,
            Self::opmask_logic_prefix(width),
            Self::opmask_w(width),
            false,
            0,
            false,
            false,
        );
        self.code.emit_u8(if is_load { 0x90 } else { 0x91 });
        self.code.emit_u8((mask << 3) | 0x04);
        self.code.emit_u8(0x24);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_opmask_rr(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        w: bool,
        l: bool,
        vvvv: u8,
        opcode: u8,
        reg: u8,
        rm: u8,
    ) {
        self.emit_opmask_vex_prefix(map, pp, w, l, vvvv, false, rm >= 8);
        self.code.emit_u8(opcode);
        self.code.emit_u8(0xC0 | ((reg & 7) << 3) | (rm & 7));
    }

    fn emit_opmask_vex_prefix(
        &mut self,
        map: X86VecMap,
        pp: X86SsePrefix,
        w: bool,
        l: bool,
        vvvv: u8,
        r: bool,
        b: bool,
    ) {
        let map = match map {
            X86VecMap::Map0F => 1,
            X86VecMap::Map0F3A => 3,
            _ => unreachable!("opmask uses VEX map 1 or 3"),
        };
        let pp = match pp {
            X86SsePrefix::None => 0,
            X86SsePrefix::OpSize => 1,
            X86SsePrefix::Rep => 2,
            X86SsePrefix::Repne => 3,
        };
        self.code.emit_u8(0xC4);
        // VEX.X is always inverted 1. VEX.R extends only the opcode-93 GPR
        // destination; VEX.B extends only the opcode-92 GPR source.
        self.code
            .emit_u8((u8::from(!r) << 7) | 0x40 | (u8::from(!b) << 5) | map);
        self.code
            .emit_u8((u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp);
    }

    fn opmask_index(reg: VReg) -> Option<u8> {
        match reg {
            VReg::Arch(ArchReg::X86(X86Reg::K(index @ 0..=7))) => Some(index),
            _ => None,
        }
    }

    fn opmask_logic_prefix(width: OpWidth) -> X86SsePrefix {
        if matches!(width, OpWidth::W8 | OpWidth::W32) {
            X86SsePrefix::OpSize
        } else {
            X86SsePrefix::None
        }
    }

    fn opmask_gpr_prefix(width: OpWidth) -> X86SsePrefix {
        match width {
            OpWidth::W8 => X86SsePrefix::OpSize,
            OpWidth::W16 => X86SsePrefix::None,
            OpWidth::W32 | OpWidth::W64 => X86SsePrefix::Repne,
            OpWidth::W128 => unreachable!("validated KMOV width"),
        }
    }

    fn opmask_w(width: OpWidth) -> bool {
        matches!(width, OpWidth::W32 | OpWidth::W64)
    }
}
