//! system.rs

use crate::isa::riscv::{
    Isa as RvIsa, Op as RvOp, Xlen as RvXlen, decode as rv_decode, rvc::decode_rvc as rv_decode_rvc,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, RvVectorState, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{SmirBlock, SmirFunction};
use crate::smir::lift::riscv::*;

use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl RiscVLifter {
    /// Fence instructions
    pub(crate) fn lift_fence(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let funct3 = Self::funct3(insn);

        let mut ops = Vec::new();

        match funct3 {
            0b000 => {
                // FENCE
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Fence {
                        kind: FenceKind::Full,
                    },
                ));
            }
            0b001 if self.extensions.zifencei => {
                // FENCE.I (instruction fence)
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::Fence {
                        kind: FenceKind::ISync,
                    },
                ));
            }
            0b010
                if self.extensions.zicboz
                    && Self::rd(insn) == 0
                    && ((insn >> 20) & 0xfff) == 0x004 =>
            {
                let base = self.get_x_reg(Self::rs1(insn), ctx);
                let aligned = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    ctx.next_op_id(),
                    addr,
                    OpKind::And {
                        dst: aligned,
                        src1: base,
                        src2: SrcOperand::Imm(!0x3f),
                        width: self.op_width(),
                        flags: FlagUpdate::None,
                    },
                ));

                for offset in (0..64).step_by(8) {
                    ops.push(SmirOp::new(
                        ctx.next_op_id(),
                        addr,
                        OpKind::Store {
                            src: VReg::Imm(0),
                            addr: Address::BaseOffset {
                                base: aligned,
                                offset,
                                disp_size: DispSize::Auto,
                            },
                            width: MemWidth::B8,
                        },
                    ));
                }
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: insn.to_le_bytes().to_vec(),
                });
            }
        }

        Ok((ops, ControlFlow::NextInsn))
    }

    /// System instructions (ECALL, EBREAK, CSR ops)
    pub(crate) fn lift_system(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = Self::rd(insn);
        let rs1_reg = Self::rs1(insn);
        let funct3 = Self::funct3(insn);
        let csr = ((insn >> 20) & 0xFFF) as u32;

        let mut ops = Vec::new();

        if funct3 == 0b100 && self.extensions.h {
            return self.lift_hypervisor_mem(insn, addr, ctx);
        }

        match funct3 {
            0b000 => {
                // ECALL, EBREAK, privileged instructions
                match insn {
                    0x00000073 => {
                        // ECALL
                        // System call - use a7 as syscall number
                        let syscall_num = self.get_x_reg(17, ctx); // a7
                        let args = (10..=16).map(|r| self.get_x_reg(r, ctx)).collect();
                        ops.push(SmirOp::new(
                            ctx.next_op_id(),
                            addr,
                            OpKind::Syscall {
                                num: syscall_num,
                                args,
                            },
                        ));
                        return Ok((ops, ControlFlow::NextInsn));
                    }
                    0x00100073 => {
                        // EBREAK
                        ops.push(SmirOp::new(ctx.next_op_id(), addr, OpKind::Breakpoint));
                        return Ok((ops, ControlFlow::NextInsn));
                    }
                    _ => {
                        return Err(LiftError::Unsupported {
                            addr,
                            mnemonic: "privileged instruction".to_string(),
                        });
                    }
                }
            }
            // CSR instructions (Zicsr). Lifted+verified for the application-
            // visible CSRs that SMIR models: fcsr (0x003, read+write) and the
            // read-only fflags/frm/vl/vtype/vlenb. Other CSRs (privileged state,
            // counters, and the fflags/frm/vector-fixedpoint *writes* that alias
            // fcsr/vcsr) are honest gaps. Read-old → rd, then conditionally write
            // the new value; the mask is applied in-IR because the vreg writeback
            // bypasses write_arch_reg's masking.
            0b001 | 0b010 | 0b011 | 0b101 | 0b110 | 0b111 => {
                if !self.extensions.zicsr
                    || (matches!(csr, 0x001..=0x003) && !self.extensions.f)
                    || (matches!(csr, 0xc20..=0xc22) && !self.extensions.v)
                {
                    return Err(LiftError::InvalidEncoding {
                        addr,
                        bytes: insn.to_le_bytes().to_vec(),
                    });
                }
                let is_imm = funct3 & 0b100 != 0;
                let op = funct3 & 0b011; // 1=rw, 2=rs, 3=rc
                let zimm = rs1_reg as i64; // 5-bit immediate (csrr*i forms)
                let writes = match op {
                    1 => true, // csrrw / csrrwi always write
                    _ => {
                        if is_imm {
                            zimm != 0
                        } else {
                            rs1_reg != 0
                        }
                    }
                };
                // fcsr-family CSRs are a (shift, field-mask) view of fcsr (0x003);
                // the read-only CSRs are read straight from their arch reg.
                let fcsr_field: Option<(i64, i64)> = match csr {
                    0x003 => Some((0, 0xff)), // fcsr
                    0x001 => Some((0, 0x1f)), // fflags
                    0x002 => Some((5, 0x7)),  // frm
                    _ => None,
                };
                let modeled_jvt = csr == 0x017 && self.extensions.zcmt;
                let modeled_ro = matches!(csr, 0xc20 | 0xc21 | 0xc22);
                let w = self.op_width();
                let mk = |ctx: &mut LiftContext, k: OpKind| SmirOp::new(ctx.next_op_id(), addr, k);
                if let Some((shift, mask)) = fcsr_field {
                    // Read the whole fcsr, extract the addressed field → rd.
                    let fcsr_cur = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
                    let old_full = ctx.alloc_vreg();
                    ops.push(mk(
                        ctx,
                        OpKind::Mov {
                            dst: old_full,
                            src: SrcOperand::Reg(fcsr_cur),
                            width: w,
                        },
                    ));
                    let shifted = if shift != 0 {
                        let s = ctx.alloc_vreg();
                        ops.push(mk(
                            ctx,
                            OpKind::Shr {
                                dst: s,
                                src: old_full,
                                amount: SrcOperand::Imm(shift),
                                width: w,
                                flags: FlagUpdate::None,
                            },
                        ));
                        s
                    } else {
                        old_full
                    };
                    let old_field = ctx.alloc_vreg();
                    ops.push(mk(
                        ctx,
                        OpKind::And {
                            dst: old_field,
                            src1: shifted,
                            src2: SrcOperand::Imm(mask),
                            width: w,
                            flags: FlagUpdate::None,
                        },
                    ));
                    // Snapshot rs1 BEFORE writing rd. Retaining an architectural
                    // VReg here is insufficient: if rd aliases rs1, the later
                    // CSR update would observe the newly written old-CSR value.
                    let src = if writes {
                        if is_imm {
                            Some(SrcOperand::Imm(zimm))
                        } else {
                            let snapshot = ctx.alloc_vreg();
                            let source = self.get_x_reg(rs1_reg, ctx);
                            ops.push(mk(
                                ctx,
                                OpKind::Mov {
                                    dst: snapshot,
                                    src: SrcOperand::Reg(source),
                                    width: w,
                                },
                            ));
                            Some(SrcOperand::Reg(snapshot))
                        }
                    } else {
                        None
                    };
                    if let Some(dst) = self.def_x_reg(rd, ctx) {
                        ops.push(mk(
                            ctx,
                            OpKind::Mov {
                                dst,
                                src: SrcOperand::Reg(old_field),
                                width: w,
                            },
                        ));
                    }
                    if let Some(src) = src {
                        // new_field (pre-mask) = src | (old|src) | (old&~src).
                        let nf = ctx.alloc_vreg();
                        match op {
                            1 => ops.push(mk(
                                ctx,
                                OpKind::Mov {
                                    dst: nf,
                                    src,
                                    width: w,
                                },
                            )),
                            2 => ops.push(mk(
                                ctx,
                                OpKind::Or {
                                    dst: nf,
                                    src1: old_field,
                                    src2: src,
                                    width: w,
                                    flags: FlagUpdate::None,
                                },
                            )),
                            _ => ops.push(mk(
                                ctx,
                                OpKind::AndNot {
                                    dst: nf,
                                    src1: old_field,
                                    src2: src,
                                    width: w,
                                    flags: FlagUpdate::None,
                                },
                            )),
                        }
                        let nfm = ctx.alloc_vreg();
                        ops.push(mk(
                            ctx,
                            OpKind::And {
                                dst: nfm,
                                src1: nf,
                                src2: SrcOperand::Imm(mask),
                                width: w,
                                flags: FlagUpdate::None,
                            },
                        ));
                        // new_fcsr = (old_full & ~(mask<<shift)) | (nfm << shift).
                        let cleared = ctx.alloc_vreg();
                        ops.push(mk(
                            ctx,
                            OpKind::AndNot {
                                dst: cleared,
                                src1: old_full,
                                src2: SrcOperand::Imm(mask << shift),
                                width: w,
                                flags: FlagUpdate::None,
                            },
                        ));
                        let placed = if shift != 0 {
                            let p = ctx.alloc_vreg();
                            ops.push(mk(
                                ctx,
                                OpKind::Shl {
                                    dst: p,
                                    src: nfm,
                                    amount: SrcOperand::Imm(shift),
                                    width: w,
                                    flags: FlagUpdate::None,
                                },
                            ));
                            p
                        } else {
                            nfm
                        };
                        let new_full = ctx.alloc_vreg();
                        ops.push(mk(
                            ctx,
                            OpKind::Or {
                                dst: new_full,
                                src1: cleared,
                                src2: SrcOperand::Reg(placed),
                                width: w,
                                flags: FlagUpdate::None,
                            },
                        ));
                        let csr_dst = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
                        ops.push(mk(
                            ctx,
                            OpKind::Mov {
                                dst: csr_dst,
                                src: SrcOperand::Reg(new_full),
                                width: w,
                            },
                        ));
                    }
                } else if modeled_jvt {
                    let current = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x017)));
                    let old = ctx.alloc_vreg();
                    ops.push(mk(
                        ctx,
                        OpKind::Mov {
                            dst: old,
                            src: SrcOperand::Reg(current),
                            width: w,
                        },
                    ));
                    let src = if writes {
                        if is_imm {
                            Some(SrcOperand::Imm(zimm))
                        } else {
                            let snapshot = ctx.alloc_vreg();
                            let source = self.get_x_reg(rs1_reg, ctx);
                            ops.push(mk(
                                ctx,
                                OpKind::Mov {
                                    dst: snapshot,
                                    src: SrcOperand::Reg(source),
                                    width: w,
                                },
                            ));
                            Some(SrcOperand::Reg(snapshot))
                        }
                    } else {
                        None
                    };
                    if let Some(dst) = self.def_x_reg(rd, ctx) {
                        ops.push(mk(
                            ctx,
                            OpKind::Mov {
                                dst,
                                src: SrcOperand::Reg(old),
                                width: w,
                            },
                        ));
                    }
                    if let Some(src) = src {
                        let new_value = ctx.alloc_vreg();
                        match op {
                            1 => ops.push(mk(
                                ctx,
                                OpKind::Mov {
                                    dst: new_value,
                                    src,
                                    width: w,
                                },
                            )),
                            2 => ops.push(mk(
                                ctx,
                                OpKind::Or {
                                    dst: new_value,
                                    src1: old,
                                    src2: src,
                                    width: w,
                                    flags: FlagUpdate::None,
                                },
                            )),
                            _ => ops.push(mk(
                                ctx,
                                OpKind::AndNot {
                                    dst: new_value,
                                    src1: old,
                                    src2: src,
                                    width: w,
                                    flags: FlagUpdate::None,
                                },
                            )),
                        }
                        let aligned = ctx.alloc_vreg();
                        ops.push(mk(
                            ctx,
                            OpKind::And {
                                dst: aligned,
                                src1: new_value,
                                src2: SrcOperand::Imm(!0x3fi64),
                                width: w,
                                flags: FlagUpdate::None,
                            },
                        ));
                        ops.push(mk(
                            ctx,
                            OpKind::Mov {
                                dst: current,
                                src: SrcOperand::Reg(aligned),
                                width: w,
                            },
                        ));
                    }
                } else if modeled_ro && !writes {
                    // Read-only CSR: rd = csr value (a write would trap on hardware).
                    let cur = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(csr as u16)));
                    if let Some(dst) = self.def_x_reg(rd, ctx) {
                        ops.push(mk(
                            ctx,
                            OpKind::Mov {
                                dst,
                                src: SrcOperand::Reg(cur),
                                width: w,
                            },
                        ));
                    }
                } else {
                    return Err(LiftError::Unsupported {
                        addr,
                        mnemonic: format!("csr {csr:#x}"),
                    });
                }
                return Ok((ops, ControlFlow::NextInsn));
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: insn.to_le_bytes().to_vec(),
                });
            }
        }
    }
}
