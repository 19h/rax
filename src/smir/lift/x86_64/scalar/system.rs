//! System, I/O, group, and miscellaneous scalar lifting.

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
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
    /// Lift legacy LDMXCSR/STMXCSR and LFENCE/MFENCE/SFENCE (0F AE).
    pub(crate) fn lift_fence_0f(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let group = (modrm.byte >> 3) & 7;
        if let Some(result) =
            self.lift_group15_profile_form(bytes, prefix, &modrm, group, pc, ctx)?
        {
            return Ok(result);
        }
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if modrm.is_memory && matches!(group, 4 | 5 | 6) && !prefix.operand_size_override {
            if prefix.rep_prefix.is_some() {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            let kind = if group == 5 {
                OpKind::X86XRstor {
                    addr,
                    rex_w: prefix.rex_w(),
                    supervisor: false,
                    src_low: self.gpr(0),
                    src_high: self.gpr(2),
                }
            } else {
                OpKind::X86XSave {
                    addr,
                    rex_w: prefix.rex_w(),
                    kind: if group == 6 {
                        X86XSaveKind::XSaveOpt
                    } else {
                        X86XSaveKind::XSave
                    },
                    src_low: self.gpr(0),
                    src_high: self.gpr(2),
                }
            };
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        if modrm.is_memory
            && matches!(group, 4 | 5 | 6)
            && (prefix.rep_prefix.is_some() || prefix.operand_size_override)
            && !(group == 6 && prefix.operand_size_override && prefix.rep_prefix.is_none())
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        if matches!(group, 0 | 1) {
            if !modrm.is_memory {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
                });
            }
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if group == 0 {
                    OpKind::X86FxSave {
                        addr,
                        rex_w: prefix.rex_w(),
                    }
                } else {
                    OpKind::X86FxRstor {
                        addr,
                        rex_w: prefix.rex_w(),
                    }
                },
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        if modrm.is_memory && matches!(group, 2 | 3) {
            if prefix.rep_prefix.is_some() || prefix.operand_size_override {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if group == 2 {
                    OpKind::X86LoadMxcsr { addr }
                } else {
                    OpKind::X86StoreMxcsr { addr }
                },
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        if modrm.is_memory && ((group == 7) || (group == 6 && prefix.operand_size_override)) {
            if prefix.rep_prefix.is_some() {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let kind = match (group, prefix.operand_size_override) {
                (7, false) => X86CacheControlKind::Clflush,
                (7, true) => X86CacheControlKind::Clflushopt,
                (6, true) => X86CacheControlKind::Clwb,
                _ => unreachable!(),
            };
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CacheControl { addr, kind },
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        let no_mandatory_prefix =
            !modrm.is_memory && prefix.rep_prefix.is_none() && !prefix.operand_size_override;
        let kind = match modrm.byte {
            // Intel specifies that the r/m field is ignored for all three
            // fences, making each complete eight-value ModR/M range valid.
            0xE8..=0xEF if no_mandatory_prefix => FenceKind::LoadLoad,
            0xF0..=0xF7 if no_mandatory_prefix => FenceKind::Full,
            0xF8..=0xFF if no_mandatory_prefix => FenceKind::StoreStore,
            // Preserve the existing accepted redundant-prefix behavior for the
            // canonical LFENCE and SFENCE encodings. Group /6 mandatory-prefix
            // forms are WAITPKG instructions and must not become MFENCE.
            0xE8 => FenceKind::LoadLoad,
            0xF8 => FenceKind::StoreStore,
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("0F AE /{}", (modrm.byte >> 3) & 7),
                });
            }
        };
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, OpKind::Fence { kind })],
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift Group-9 CMPXCHG8B/16B, compacted XSAVE-family, random-source, and
    /// processor-ID forms (0F C7).
    pub(crate) fn lift_xsave_group9_0fc7(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let group = (modrm.byte >> 3) & 7;
        match group {
            1 => {
                if !modrm.is_memory {
                    return Err(LiftError::InvalidEncoding {
                        addr: pc,
                        bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
                    });
                }
                let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
                let (addr, mut ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86Cmpxchg8b16b {
                        addr,
                        wide: prefix.rex_w(),
                        locked: prefix.lock,
                        compare_lo: self.gpr(0),
                        compare_hi: self.gpr(2),
                        new_lo: self.gpr(3),
                        new_hi: self.gpr(1),
                        dst_lo: self.gpr(0),
                        dst_hi: self.gpr(2),
                    },
                ));
                Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ))
            }
            3..=5 => {
                if prefix.lock
                    || prefix.rep_prefix.is_some()
                    || prefix.operand_size_override
                    || !modrm.is_memory
                {
                    return Err(LiftError::InvalidEncoding {
                        addr: pc,
                        bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
                    });
                }
                let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
                let (addr, mut ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                let kind = match group {
                    3 => OpKind::X86XRstor {
                        addr,
                        rex_w: prefix.rex_w(),
                        supervisor: true,
                        src_low: self.gpr(0),
                        src_high: self.gpr(2),
                    },
                    4 | 5 => OpKind::X86XSave {
                        addr,
                        rex_w: prefix.rex_w(),
                        kind: if group == 4 {
                            X86XSaveKind::XSaveC
                        } else {
                            X86XSaveKind::XSaveS
                        },
                        src_low: self.gpr(0),
                        src_high: self.gpr(2),
                    },
                    _ => unreachable!(),
                };
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
                Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ))
            }
            6 | 7 => {
                if prefix.lock || modrm.is_memory {
                    return Err(LiftError::InvalidEncoding {
                        addr: pc,
                        bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
                    });
                }
                if group == 6 && prefix.rep_prefix == Some(0xF3) {
                    // SENDUIPI requires User Interrupts. The configured x86
                    // interpreter does not expose UINTR and therefore raises
                    // #UD for this encoding; preserve that architectural exit
                    // explicitly instead of treating it as a lifting gap.
                    return Ok(LiftResult {
                        ops: vec![],
                        bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                        control_flow: ControlFlow::Trap {
                            kind: TrapKind::InvalidOpcode,
                        },
                        branch_targets: vec![],
                    });
                }
                let kind = if group == 7 && prefix.rep_prefix == Some(0xF3) {
                    OpKind::X86ReadPid {
                        dst: self.gpr(modrm.rm),
                    }
                } else {
                    OpKind::X86Random {
                        dst: self.gpr(modrm.rm),
                        width: if prefix.rex_w() {
                            OpWidth::W64
                        } else if prefix.operand_size_override {
                            OpWidth::W16
                        } else {
                            OpWidth::W32
                        },
                        seed: group == 7,
                    }
                };
                Ok(LiftResult::fallthrough(
                    vec![SmirOp::new(OpId(0), pc, kind)],
                    prefix.cursor + modrm.bytes_consumed,
                ))
            }
            0 | 2 => Ok(LiftResult {
                // Intel SDM Table A-6 leaves the /0 and /2 Group 9 columns
                // unassigned. Decode the complete ModR/M address form above,
                // but do not evaluate it or emit any operation before #UD.
                ops: Vec::new(),
                bytes_consumed: prefix.cursor + modrm.bytes_consumed,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: Vec::new(),
            }),
            _ => unreachable!("Group 9 selector is masked to three bits"),
        }
    }

    pub(crate) fn lift_bf16_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let two_source = prefix.pp == X86SsePrefix::Repne;
        if prefix.w
            || prefix.l_bits == 3
            || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
            || (two_source && prefix.encoding != VecEncodingKind::Evex)
            || (!two_source && (prefix.vvvv != 0 || prefix.v_high))
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && (prefix.encoding != VecEncodingKind::Evex || !modrm.is_memory) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let memory_src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 4 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if !two_source {
                if let Some(mask) = mask {
                    self.append_evex_masked_vector_source(
                        addr,
                        VecElementType::F32,
                        prefix.width,
                        prefix.b,
                        mask,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else if prefix.b {
                    self.append_broadcast_memory_source(
                        addr,
                        VecElementType::F32,
                        prefix.width,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else {
                    let loaded = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: loaded,
                            addr,
                            width: prefix.width,
                        },
                    ));
                    loaded
                }
            } else if prefix.b {
                // Intel specifies no memory-fault suppression for this form.
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::F32,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let output_width = match prefix.width {
            VecWidth::V128 | VecWidth::V256 => VecWidth::V128,
            VecWidth::V512 => VecWidth::V256,
            VecWidth::V64 => unreachable!("x86 vector prefixes cannot encode 64-bit VL"),
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            if two_source {
                prefix.width
            } else {
                output_width
            },
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCvtFP32ToBF16 {
                dst,
                src1: if two_source {
                    self.vec_reg(
                        prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                        prefix.width,
                    )
                } else {
                    memory_src
                },
                src2: two_source.then_some(memory_src),
                mask,
                width: prefix.width,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_rao_int_0f38(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op = match prefix.rep_prefix {
            Some(0xF2) => AtomicOp::Or,
            Some(0xF3) => AtomicOp::Xor,
            Some(_) => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            None if prefix.operand_size_override => AtomicOp::And,
            None => AtomicOp::Add,
        };

        let width = if prefix.rex_w() {
            MemWidth::B8
        } else {
            MemWidth::B4
        };
        self.lift_rao_int_modrm(bytes, prefix, pc, ctx, op, width)
    }

    pub(crate) fn lift_rao_int_modrm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        op: AtomicOp,
        width: MemWidth,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let x86_addr = modrm.addr.as_ref().unwrap();
        let (addr, mut ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
        let old = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::AtomicRmw {
                dst: old,
                addr,
                src: self.gpr(modrm.reg),
                op,
                width,
                order: MemoryOrder::SeqCst,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift Group 4 INC/DEC r/m8 (FE /0, /1).
    pub(crate) fn lift_group4(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let group = (modrm.byte >> 3) & 0x07;
        if group > 1 || (prefix.lock && !modrm.is_memory) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let operand;
        let mut store_addr = None;
        let mut high_byte_base = None;

        if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            operand = ctx.alloc_vreg();

            if prefix.lock {
                let one = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: one,
                        src: SrcOperand::Imm(1),
                        width: OpWidth::W8,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::AtomicRmw {
                        dst: operand,
                        addr,
                        src: one,
                        op: if group == 0 {
                            AtomicOp::Add
                        } else {
                            AtomicOp::Sub
                        },
                        width: MemWidth::B1,
                        order: MemoryOrder::SeqCst,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: operand,
                        addr: addr.clone(),
                        width: MemWidth::B1,
                        sign: SignExtend::Zero,
                    },
                ));
                store_addr = Some(addr);
            }
        } else {
            if !prefix.has_rex() && (4..=7).contains(&(modrm.rm & 7)) {
                // Legacy byte-register codes 4..7 name AH/CH/DH/BH. Extract
                // bits 15:8 into a temporary, operate on its low byte, then
                // merge the result back after the flag-producing operation.
                let base = self.gpr((modrm.rm & 7) - 4);
                operand = ctx.alloc_vreg();
                high_byte_base = Some(base);
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: operand,
                        src: base,
                        amount: SrcOperand::Imm(8),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            } else {
                operand = self.gpr(modrm.rm);
            }
        }

        if let Some(addr) = store_addr {
            // Delay the architectural flag update until after the potentially
            // faulting store. This preserves precise-exception semantics.
            let result = ctx.alloc_vreg();
            let update_no_flags = if group == 0 {
                OpKind::Inc {
                    dst: result,
                    src: operand,
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                }
            } else {
                OpKind::Dec {
                    dst: result,
                    src: operand,
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                }
            };
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, update_no_flags));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: MemWidth::B1,
                },
            ));
            let flags_result = ctx.alloc_vreg();
            let update_flags = if group == 0 {
                OpKind::Inc {
                    dst: flags_result,
                    src: operand,
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                }
            } else {
                OpKind::Dec {
                    dst: flags_result,
                    src: operand,
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                }
            };
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, update_flags));
        } else {
            let update = if group == 0 {
                OpKind::Inc {
                    dst: operand,
                    src: operand,
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                }
            } else {
                OpKind::Dec {
                    dst: operand,
                    src: operand,
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                }
            };
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, update));

            if let Some(base) = high_byte_base {
                let byte = ctx.alloc_vreg();
                let shifted = ctx.alloc_vreg();
                let preserved = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: byte,
                        src1: operand,
                        src2: SrcOperand::Imm(0xFF),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shl {
                        dst: shifted,
                        src: byte,
                        amount: SrcOperand::Imm(8),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: preserved,
                        src1: base,
                        src2: SrcOperand::Imm(!0xFF00u64 as i64),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Or {
                        dst: base,
                        src1: preserved,
                        src2: SrcOperand::Reg(shifted),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift group 3 instructions (F6/F7)
    pub(crate) fn lift_group3(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = opcode == 0xF6;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let group = (modrm.byte >> 3) & 0x07;
        let imm_size = if group == 0 {
            if is_8bit {
                1
            } else if op_size == 2 {
                2
            } else {
                4
            }
        } else {
            0
        };

        if bytes.len() < modrm.bytes_consumed + imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: modrm.bytes_consumed + imm_size,
            });
        }

        let imm = if imm_size == 0 {
            0
        } else if imm_size == 1 {
            bytes[modrm.bytes_consumed] as i8 as i64
        } else if imm_size == 2 {
            i16::from_le_bytes([bytes[modrm.bytes_consumed], bytes[modrm.bytes_consumed + 1]])
                as i64
        } else {
            i32::from_le_bytes([
                bytes[modrm.bytes_consumed],
                bytes[modrm.bytes_consumed + 1],
                bytes[modrm.bytes_consumed + 2],
                bytes[modrm.bytes_consumed + 3],
            ]) as i64
        };

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
        let mut high_dst = None;

        if prefix.lock {
            if !modrm.is_memory || (group != 2 && group != 3) {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let atomic_source = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: atomic_source,
                    src: SrcOperand::Imm(if group == 2 { -1 } else { 0 }),
                    width,
                },
            ));
            let old = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::AtomicRmw {
                    dst: old,
                    addr,
                    src: atomic_source,
                    op: if group == 2 {
                        AtomicOp::Nand
                    } else {
                        AtomicOp::Neg
                    },
                    width: mem_width,
                    order: MemoryOrder::SeqCst,
                },
            ));
            if group == 3 {
                let flag_result = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Neg {
                        dst: flag_result,
                        src: old,
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
            }
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        let (operand, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else if is_8bit {
            high_dst = self.high_byte_base(modrm.rm, prefix);
            (
                self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                None,
            )
        } else {
            (self.gpr(modrm.rm), None)
        };

        let mut writeback_value = operand;

        match group {
            0 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Test {
                        src1: operand,
                        src2: SrcOperand::Imm(imm),
                        width,
                    },
                ));
            }
            2 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Not {
                        dst: operand,
                        src: operand,
                        width,
                    },
                ));
            }
            3 => {
                writeback_value = if addr.is_some() {
                    ctx.alloc_vreg()
                } else {
                    operand
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Neg {
                        dst: writeback_value,
                        src: operand,
                        width,
                        flags: if addr.is_some() {
                            FlagUpdate::None
                        } else {
                            FlagUpdate::All
                        },
                    },
                ));
            }
            4 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::MulU {
                        dst_lo: self.gpr(0),
                        dst_hi: (width != OpWidth::W8).then_some(self.gpr(2)),
                        src1: self.gpr(0),
                        src2: SrcOperand::Reg(operand),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
            }
            5 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::MulS {
                        dst_lo: self.gpr(0),
                        dst_hi: (width != OpWidth::W8).then_some(self.gpr(2)),
                        src1: self.gpr(0),
                        src2: SrcOperand::Reg(operand),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
            }
            6 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::DivU {
                        quot: self.gpr(0),
                        rem: (width != OpWidth::W8).then_some(self.gpr(2)),
                        src1: self.gpr(0),
                        src2: SrcOperand::Reg(operand),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
            }
            7 => {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::DivS {
                        quot: self.gpr(0),
                        rem: (width != OpWidth::W8).then_some(self.gpr(2)),
                        src1: self.gpr(0),
                        src2: SrcOperand::Reg(operand),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
            }
            _ => {
                if self.strict {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("group3 {}", group),
                    });
                }
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, OpKind::Nop));
            }
        }

        if matches!(group, 2 | 3) {
            if let Some(base) = high_dst {
                self.merge_high_byte(base, writeback_value, pc, ctx, &mut ops);
            }
            if let Some(addr) = addr {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: writeback_value,
                        addr,
                        width: mem_width,
                    },
                ));
                if group == 3 {
                    let flag_result = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Neg {
                            dst: flag_result,
                            src: operand,
                            width,
                            flags: FlagUpdate::All,
                        },
                    ));
                }
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + imm_size,
        ))
    }

    /// Lift PUSHFQ/PUSHFW (9C) and POPFQ/POPFW (9D) in 64-bit mode.
    pub(crate) fn lift_stack_flags(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }
        let (stack_bytes, mem_width) = if prefix.operand_size_override {
            (2, MemWidth::B2)
        } else {
            (8, MemWidth::B8)
        };
        let mut ops = Vec::new();

        match opcode {
            0x9C => {
                let flags = ctx.alloc_vreg();
                ops.push(SmirOp::new(OpId(0), pc, OpKind::ReadFlags { dst: flags }));
                ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::Sub {
                        dst: self.rsp(),
                        src1: self.rsp(),
                        src2: SrcOperand::Imm(stack_bytes),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(2),
                    pc,
                    OpKind::Store {
                        src: flags,
                        addr: Address::Direct(self.rsp()),
                        width: mem_width,
                    },
                ));
            }
            0x9D => {
                let popped = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::Load {
                        dst: popped,
                        addr: Address::Direct(self.rsp()),
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::Add {
                        dst: self.rsp(),
                        src1: self.rsp(),
                        src2: SrcOperand::Imm(stack_bytes),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));

                // SMIR models CF/PF/AF/ZF/SF/DF/OF/AC. Apply POPF's
                // reserved/control-bit filtering to that representable subset
                // and force bit 1 set.
                const SMIR_RFLAGS_MASK: i64 = 0x4_0CD5;
                let masked = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(2),
                    pc,
                    OpKind::And {
                        dst: masked,
                        src1: popped,
                        src2: SrcOperand::Imm(SMIR_RFLAGS_MASK),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));

                let new_flags = if prefix.operand_size_override {
                    let old_flags = ctx.alloc_vreg();
                    let preserved = ctx.alloc_vreg();
                    let merged = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(3),
                        pc,
                        OpKind::ReadFlags { dst: old_flags },
                    ));
                    ops.push(SmirOp::new(
                        OpId(4),
                        pc,
                        OpKind::And {
                            dst: preserved,
                            src1: old_flags,
                            src2: SrcOperand::Imm(!0xFFFF),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(5),
                        pc,
                        OpKind::Or {
                            dst: merged,
                            src1: preserved,
                            src2: SrcOperand::Reg(masked),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    merged
                } else {
                    masked
                };

                let with_reserved = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Or {
                        dst: with_reserved,
                        src1: new_flags,
                        src2: SrcOperand::Imm(2),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::WriteFlags { src: with_reserved },
                ));
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![opcode],
                });
            }
        }

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift SAHF (9E) and LAHF (9F).
    pub(crate) fn lift_ah_flags(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }
        const STATUS_MASK: i64 = 0xD5;
        let mut ops = Vec::new();

        match opcode {
            0x9E => {
                let old_flags = ctx.alloc_vreg();
                let ah = ctx.alloc_vreg();
                let status = ctx.alloc_vreg();
                let preserved = ctx.alloc_vreg();
                let merged = ctx.alloc_vreg();

                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::ReadFlags { dst: old_flags },
                ));
                ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::Shr {
                        dst: ah,
                        src: self.gpr(0),
                        amount: SrcOperand::Imm(8),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(2),
                    pc,
                    OpKind::And {
                        dst: status,
                        src1: ah,
                        src2: SrcOperand::Imm(STATUS_MASK),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(3),
                    pc,
                    OpKind::And {
                        dst: preserved,
                        src1: old_flags,
                        src2: SrcOperand::Imm(!STATUS_MASK),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(4),
                    pc,
                    OpKind::Or {
                        dst: merged,
                        src1: preserved,
                        src2: SrcOperand::Reg(status),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(OpId(5), pc, OpKind::WriteFlags { src: merged }));
            }
            0x9F => {
                let flags = ctx.alloc_vreg();
                let status = ctx.alloc_vreg();
                let shifted = ctx.alloc_vreg();
                let cleared_rax = ctx.alloc_vreg();

                ops.push(SmirOp::new(OpId(0), pc, OpKind::ReadFlags { dst: flags }));
                ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::And {
                        dst: status,
                        src1: flags,
                        src2: SrcOperand::Imm(STATUS_MASK),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(2),
                    pc,
                    OpKind::Or {
                        dst: status,
                        src1: status,
                        src2: SrcOperand::Imm(2),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(3),
                    pc,
                    OpKind::Shl {
                        dst: shifted,
                        src: status,
                        amount: SrcOperand::Imm(8),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(4),
                    pc,
                    OpKind::And {
                        dst: cleared_rax,
                        src1: self.gpr(0),
                        src2: SrcOperand::Imm(!0xFF00),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(5),
                    pc,
                    OpKind::Or {
                        dst: self.gpr(0),
                        src1: cleared_rax,
                        src2: SrcOperand::Reg(shifted),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![opcode],
                });
            }
        }

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift IN imm8 or DX (E4/E5/EC/ED)
    pub(crate) fn lift_in(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let (port, width, imm_len) = match opcode {
            0xE4 => {
                if bytes.is_empty() {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: 0,
                        need: 1,
                    });
                }
                (VReg::Imm(bytes[0] as i8 as i64), MemWidth::B1, 1)
            }
            0xE5 => {
                if bytes.is_empty() {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: 0,
                        need: 1,
                    });
                }
                let width = if prefix.operand_size_override {
                    MemWidth::B2
                } else {
                    MemWidth::B4
                };
                (VReg::Imm(bytes[0] as i8 as i64), width, 1)
            }
            0xEC => (self.gpr(2), MemWidth::B1, 0),
            0xED => {
                let width = if prefix.operand_size_override {
                    MemWidth::B2
                } else {
                    MemWidth::B4
                };
                (self.gpr(2), width, 0)
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };

        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::IoIn {
                dst: self.gpr(0),
                port,
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_len))
    }

    /// Lift OUT imm8 or DX (E6/E7/EE/EF)
    pub(crate) fn lift_out(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let (port, width, imm_len) = match opcode {
            0xE6 => {
                if bytes.is_empty() {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: 0,
                        need: 1,
                    });
                }
                (VReg::Imm(bytes[0] as i8 as i64), MemWidth::B1, 1)
            }
            0xE7 => {
                if bytes.is_empty() {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: 0,
                        need: 1,
                    });
                }
                let width = if prefix.operand_size_override {
                    MemWidth::B2
                } else {
                    MemWidth::B4
                };
                (VReg::Imm(bytes[0] as i8 as i64), width, 1)
            }
            0xEE => (self.gpr(2), MemWidth::B1, 0),
            0xEF => {
                let width = if prefix.operand_size_override {
                    MemWidth::B2
                } else {
                    MemWidth::B4
                };
                (self.gpr(2), width, 0)
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };

        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::IoOut {
                port,
                value: self.gpr(0),
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_len))
    }

    /// Lift NOP (90)
    pub(crate) fn lift_nop(&self, prefix: &X86Prefix, pc: u64) -> Result<LiftResult, LiftError> {
        Ok(LiftResult::fallthrough(
            self.rex2_apx_guard_ops(prefix, pc),
            prefix.cursor,
        ))
    }
}
