//! Decoded scalar, control-flow, and vector instruction execution.

use super::*;
use crate::isa::hexagon::semantics::merge_predicate;

impl HexagonVcpu {
    pub(super) fn execute_insn(
        &mut self,
        insn: DecodedInsn,
        packet_pc: u32,
        immext: Option<u32>,
        new_r: &mut [Option<u32>; 32],
        new_p: &mut [Option<u8>; 4],
        branch: &mut Option<BranchTarget>,
    ) -> Result<Option<VcpuExit>> {
        match insn {
            DecodedInsn::ImmExt { .. } => {
                return Err(Error::Emulator("unexpected immext in execute".to_string()));
            }
            DecodedInsn::Add { dst, src1, src2 } => {
                let val = self.regs.r[src1 as usize].wrapping_add(self.regs.r[src2 as usize]);
                new_r[dst as usize] = Some(val);
            }
            DecodedInsn::Sub { dst, src1, src2 } => {
                let val = self.regs.r[src1 as usize].wrapping_sub(self.regs.r[src2 as usize]);
                new_r[dst as usize] = Some(val);
            }
            DecodedInsn::And { dst, src1, src2 } => {
                new_r[dst as usize] = Some(self.regs.r[src1 as usize] & self.regs.r[src2 as usize]);
            }
            DecodedInsn::AndImm { dst, src, imm } => {
                new_r[dst as usize] = Some(self.regs.r[src as usize] & imm);
            }
            DecodedInsn::OrImm { dst, src, imm } => {
                new_r[dst as usize] = Some(self.regs.r[src as usize] | imm);
            }
            DecodedInsn::Or { dst, src1, src2 } => {
                new_r[dst as usize] = Some(self.regs.r[src1 as usize] | self.regs.r[src2 as usize]);
            }
            DecodedInsn::Xor { dst, src1, src2 } => {
                new_r[dst as usize] = Some(self.regs.r[src1 as usize] ^ self.regs.r[src2 as usize]);
            }
            DecodedInsn::AddImm { dst, src, imm } => {
                let val = self.regs.r[src as usize].wrapping_add(imm as u32);
                new_r[dst as usize] = Some(val);
            }
            // SubImmRev: Rd = #s - Rs (reversed subtract)
            DecodedInsn::SubImmRev { dst, src, imm } => {
                let val = (imm as u32).wrapping_sub(self.regs.r[src as usize]);
                new_r[dst as usize] = Some(val);
            }
            DecodedInsn::Mov { dst, src } => {
                new_r[dst as usize] = Some(self.regs.r[src as usize]);
            }
            DecodedInsn::MovImm { dst, imm } => {
                new_r[dst as usize] = Some(imm as u32);
            }
            DecodedInsn::ClearCond { dst, pred } => {
                if self.eval_pred(pred, new_p) {
                    new_r[dst as usize] = Some(0);
                }
            }
            DecodedInsn::Extend { dst, src, kind } => {
                let val = self.regs.r[src as usize];
                let result = match kind {
                    ExtendKind::Sxt8 => (val as i8 as i32) as u32,
                    ExtendKind::Sxt16 => (val as i16 as i32) as u32,
                    ExtendKind::Zxt8 => (val & 0xff) as u32,
                    ExtendKind::Zxt16 => (val & 0xffff) as u32,
                };
                new_r[dst as usize] = Some(result);
            }
            DecodedInsn::Combine { dst, high, low } => {
                let odd = dst.wrapping_add(1);
                if odd >= 32 {
                    return Err(Error::Emulator(
                        "invalid register pair for combine".to_string(),
                    ));
                }
                let low_val = match low {
                    CombineOperand::Reg(reg) => self.regs.r[reg as usize],
                    CombineOperand::Imm(val) => val,
                };
                let high_val = match high {
                    CombineOperand::Reg(reg) => self.regs.r[reg as usize],
                    CombineOperand::Imm(val) => val,
                };
                new_r[dst as usize] = Some(low_val);
                new_r[odd as usize] = Some(high_val);
            }
            DecodedInsn::Load {
                dst,
                addr,
                width,
                sign,
                pred,
            } => {
                if let Some(cond) = pred {
                    if !self.eval_pred(cond, new_p) {
                        return Ok(None);
                    }
                }

                let (addr, update) = self.resolve_addr(addr);

                if let Some((reg, value)) = update {
                    new_r[reg as usize] = Some(value);
                }

                if Self::is_mmio(addr) {
                    let size = match width {
                        MemWidth::Byte => 1,
                        MemWidth::Half => 2,
                        MemWidth::Word => 4,
                        MemWidth::Double => {
                            return Err(Error::Emulator(
                                "doubleword mmio load not supported".to_string(),
                            ));
                        }
                    };
                    let signed = matches!(sign, MemSign::Signed) && size < 4;
                    self.pending_mmio = Some(MmioPending { dst, size, signed });
                    return Ok(Some(VcpuExit::MmioRead {
                        addr: addr as u64,
                        size,
                    }));
                }

                if width == MemWidth::Double {
                    let even = dst & !1;
                    let odd = even.wrapping_add(1);
                    if odd >= 32 {
                        return Err(Error::Emulator(
                            "invalid register pair for doubleword load".to_string(),
                        ));
                    }
                    let val = self.read_u64(addr)?;
                    new_r[even as usize] = Some(val as u32);
                    new_r[odd as usize] = Some((val >> 32) as u32);
                } else {
                    let val = self.load_mem(addr, width, sign)?;
                    new_r[dst as usize] = Some(val);
                }
            }
            DecodedInsn::LoadAlign {
                dst_pair,
                addr,
                width,
                pred,
            } => {
                // Predicate CANCEL: no Ryy write and no post-increment.
                if let Some(cond) = pred {
                    if !self.eval_pred(cond, new_p) {
                        return Ok(None);
                    }
                }
                let (addr, update) = self.resolve_addr(addr);
                if let Some((reg, value)) = update {
                    new_r[reg as usize] = Some(value);
                }
                let even = dst_pair & !1;
                let odd = even.wrapping_add(1);
                if odd >= 32 {
                    return Err(Error::Emulator(
                        "invalid register pair for loadalign".to_string(),
                    ));
                }
                // Current Ryy (use the in-packet value if a producer set it).
                let lo = self.read_reg_with_new(even, new_r) as u64;
                let hi = self.read_reg_with_new(odd, new_r) as u64;
                let old = lo | (hi << 32);
                // `w` = access width in bits (8 byte / 16 half). Shift Ryy right
                // by w and insert the freshly-loaded value at the top:
                //   Ryy = (Ryy u>> w) | (zxt(load) << (64-w)).
                let (w, loaded) = match width {
                    MemWidth::Byte => (8u32, self.read_u8(addr)? as u64),
                    MemWidth::Half => (16u32, self.read_u16(addr)? as u64),
                    _ => {
                        return Err(Error::Emulator(
                            "loadalign supports only byte/half".to_string(),
                        ));
                    }
                };
                let result = (old >> w) | (loaded << (64 - w));
                new_r[even as usize] = Some(result as u32);
                new_r[odd as usize] = Some((result >> 32) as u32);
            }
            DecodedInsn::LoadUnpack {
                dst,
                addr,
                count,
                sign,
                pred,
            } => {
                // Predicate CANCEL (none of the bsw/bzw forms are predicated
                // today, but honour the field for forward-compatibility).
                if let Some(cond) = pred {
                    if !self.eval_pred(cond, new_p) {
                        return Ok(None);
                    }
                }
                let (addr, update) = self.resolve_addr(addr);
                if let Some((reg, value)) = update {
                    new_r[reg as usize] = Some(value);
                }
                // Load `count` contiguous bytes; unpack each into a halfword
                // lane, sign- (membh) or zero-extended (memubh).
                let mut result: u64 = 0;
                for i in 0..count as u32 {
                    let b = self.read_u8(addr.wrapping_add(i))?;
                    let lane = match sign {
                        MemSign::Signed => (b as i8 as i16) as u16,
                        MemSign::Unsigned => b as u16,
                    };
                    result |= (lane as u64) << (16 * i);
                }
                if count == 2 {
                    new_r[dst as usize] = Some(result as u32);
                } else {
                    let even = dst & !1;
                    let odd = even.wrapping_add(1);
                    if odd >= 32 {
                        return Err(Error::Emulator(
                            "invalid register pair for loadunpack".to_string(),
                        ));
                    }
                    new_r[even as usize] = Some(result as u32);
                    new_r[odd as usize] = Some((result >> 32) as u32);
                }
            }
            DecodedInsn::Store {
                src,
                addr,
                width,
                pred,
                src_new,
                high_half,
            } => {
                if let Some(cond) = pred {
                    if !self.eval_pred(cond, new_p) {
                        // Cancelled store: no memory write AND no post-increment.
                        return Ok(None);
                    }
                }

                let (addr, update) = self.resolve_addr(addr);

                if let Some((reg, value)) = update {
                    new_r[reg as usize] = Some(value);
                }

                if width == MemWidth::Double {
                    let even = src & !1;
                    let odd = even.wrapping_add(1);
                    if odd >= 32 {
                        return Err(Error::Emulator(
                            "invalid register pair for doubleword store".to_string(),
                        ));
                    }
                    if Self::is_mmio(addr) {
                        return Err(Error::Emulator(
                            "doubleword mmio store not supported".to_string(),
                        ));
                    }
                    let even_val = if src_new {
                        self.read_reg_with_new(even, new_r)
                    } else {
                        self.regs.r[even as usize]
                    };
                    let odd_val = if src_new {
                        self.read_reg_with_new(odd, new_r)
                    } else {
                        self.regs.r[odd as usize]
                    };
                    let combined = ((odd_val as u64) << 32) | even_val as u64;
                    self.write_u64(addr, combined)?;
                } else {
                    let mut val = if src_new {
                        self.read_reg_with_new(src, new_r)
                    } else {
                        self.regs.r[src as usize]
                    };
                    // `storerf` high-half store: the stored halfword is Rt[31:16].
                    if high_half {
                        val >>= 16;
                    }

                    if Self::is_mmio(addr) {
                        let data = match width {
                            MemWidth::Byte => vec![val as u8],
                            MemWidth::Half => match self.endian {
                                Endianness::Little => (val as u16).to_le_bytes().to_vec(),
                                Endianness::Big => (val as u16).to_be_bytes().to_vec(),
                            },
                            MemWidth::Word => match self.endian {
                                Endianness::Little => val.to_le_bytes().to_vec(),
                                Endianness::Big => val.to_be_bytes().to_vec(),
                            },
                            MemWidth::Double => {
                                return Err(Error::Emulator(
                                    "doubleword mmio store not supported".to_string(),
                                ));
                            }
                        };
                        return Ok(Some(VcpuExit::MmioWrite {
                            addr: addr as u64,
                            data,
                        }));
                    }
                    self.store_mem(addr, width, val)?;
                }
            }
            DecodedInsn::StoreImm {
                value,
                addr,
                width,
                pred,
            } => {
                if let Some(cond) = pred {
                    if !self.eval_pred(cond, new_p) {
                        return Ok(None);
                    }
                }
                let addr = match addr {
                    AddrMode::Offset { base, offset } => {
                        let base_val = self.regs.r[base as usize];
                        base_val.wrapping_add(offset as u32)
                    }
                    AddrMode::GpOffset { offset } => {
                        let gp = self.regs.control(11) & !0x3f; // GP low 6 bits are hardwired zero
                        gp.wrapping_add(offset as u32)
                    }
                    AddrMode::Abs { addr } => addr,
                    AddrMode::RegScaled { base, index, shift } => {
                        let base_val = self.regs.r[base as usize];
                        let idx = self.regs.r[index as usize].wrapping_shl(shift as u32);
                        base_val.wrapping_add(idx)
                    }
                    AddrMode::PostIncImm { .. }
                    | AddrMode::PostIncReg { .. }
                    | AddrMode::PostIncBrev { .. }
                    | AddrMode::PostIncCircImm { .. }
                    | AddrMode::PostIncCircReg { .. }
                    | AddrMode::AbsSet { .. }
                    | AddrMode::IndexAbs { .. } => {
                        return Err(Error::Emulator(
                            "post-increment store immediate not supported".to_string(),
                        ));
                    }
                };

                if Self::is_mmio(addr) {
                    let data = match width {
                        MemWidth::Byte => vec![value as u8],
                        MemWidth::Half => match self.endian {
                            Endianness::Little => (value as u16).to_le_bytes().to_vec(),
                            Endianness::Big => (value as u16).to_be_bytes().to_vec(),
                        },
                        MemWidth::Word => match self.endian {
                            Endianness::Little => value.to_le_bytes().to_vec(),
                            Endianness::Big => value.to_be_bytes().to_vec(),
                        },
                        MemWidth::Double => {
                            return Err(Error::Emulator(
                                "doubleword mmio store immediate not supported".to_string(),
                            ));
                        }
                    };
                    return Ok(Some(VcpuExit::MmioWrite {
                        addr: addr as u64,
                        data,
                    }));
                }
                self.store_mem(addr, width, value)?;
            }
            DecodedInsn::AllocFrame { base, size } => {
                let sp = self.regs.r[base as usize];
                let ea = sp.wrapping_sub(8);
                let fp = self.regs.r[30];
                let lr = self.regs.r[31];
                let value = ((lr as u64) << 32) | fp as u64;
                self.write_u64(ea, value)?;
                let new_sp = ea.wrapping_sub(size);
                new_r[base as usize] = Some(new_sp);
                new_r[30] = Some(ea);
            }
            DecodedInsn::DeallocFrame {
                base,
                dst,
                update_lr_fp,
            } => {
                let addr = self.regs.r[base as usize];
                let value = self.read_u64(addr)?;
                let fp = (value & 0xffff_ffff) as u32;
                let lr = (value >> 32) as u32;
                if let Some(dst) = dst {
                    let idx = dst as usize;
                    if idx < 32 {
                        new_r[idx] = Some(fp);
                    }
                    if idx + 1 < 32 {
                        new_r[idx + 1] = Some(lr);
                    }
                }
                if update_lr_fp {
                    new_r[30] = Some(fp);
                    new_r[31] = Some(lr);
                }
                new_r[base as usize] = Some(addr.wrapping_add(8));
            }
            DecodedInsn::DeallocReturn {
                base,
                dst,
                pred,
                update_lr_fp,
            } => {
                if let Some(cond) = pred {
                    if !self.eval_pred(cond, new_p) {
                        return Ok(None);
                    }
                }
                let addr = self.regs.r[base as usize];
                let value = self.read_u64(addr)?;
                let fp = (value & 0xffff_ffff) as u32;
                let lr = (value >> 32) as u32;
                if let Some(dst) = dst {
                    let idx = dst as usize;
                    if idx < 32 {
                        new_r[idx] = Some(fp);
                    }
                    if idx + 1 < 32 {
                        new_r[idx + 1] = Some(lr);
                    }
                }
                if update_lr_fp {
                    new_r[30] = Some(fp);
                    new_r[31] = Some(lr);
                }
                new_r[base as usize] = Some(addr.wrapping_add(8));
                self.set_branch(branch, lr & !0x3, false)?;
            }
            DecodedInsn::Jump { offset } => {
                let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                self.set_branch(branch, target, false)?;
            }
            DecodedInsn::JumpCond {
                offset,
                pred,
                sense,
                pred_new,
            } => {
                let cond = PredCond {
                    pred,
                    sense,
                    pred_new,
                };
                if self.eval_pred(cond, new_p) {
                    let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                    self.set_branch(branch, target, false)?;
                }
            }
            DecodedInsn::JumpReg { src } => {
                let target = self.regs.r[src as usize] & !0x3;
                self.set_branch(branch, target, false)?;
            }
            DecodedInsn::JumpRegCond {
                src,
                pred,
                sense,
                pred_new,
            } => {
                let cond = PredCond {
                    pred,
                    sense,
                    pred_new,
                };
                if self.eval_pred(cond, new_p) {
                    let target = self.regs.r[src as usize] & !0x3;
                    self.set_branch(branch, target, false)?;
                }
            }
            DecodedInsn::JumpRegZero { src, kind, offset } => {
                // `if (cmp.<kind>(Rs,#0)) jump #r13:2`: a DIRECT PC-relative jump
                // conditioned on a register compare-to-zero.
                let a = self.regs.r[src as usize] as i32;
                let take = match kind {
                    CmpKind::Eq => a == 0,
                    CmpKind::Ne => a != 0,
                    CmpKind::Gte => a >= 0,
                    CmpKind::Lte => a <= 0,
                    _ => false,
                };
                if take {
                    let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                    self.set_branch(branch, target, false)?;
                }
            }
            DecodedInsn::JumpSet { dst, value, offset } => {
                let val = match value {
                    crate::isa::hexagon::decode::JumpSetSrc::Reg(reg) => self.regs.r[reg as usize],
                    crate::isa::hexagon::decode::JumpSetSrc::Imm(imm) => imm,
                };
                new_r[dst as usize] = Some(val);
                let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                self.set_branch(branch, target, false)?;
            }
            DecodedInsn::Nop => {}
            DecodedInsn::CompoundCmpJump {
                kind,
                src1,
                src2,
                write_pred,
                sense,
                new_value: _,
                offset,
            } => {
                use crate::isa::hexagon::decode::CmpJumpKind;
                // `src1` is already resolved to a real GPR by resolve_new_value
                // for the `_jumpnv` forms; read it as an in-flight value so a
                // same-packet producer is seen.
                let a = self.read_reg_with_new(src1, new_r);
                let result = match kind {
                    CmpJumpKind::Eq => a == self.regs.r[src2 as usize],
                    CmpJumpKind::Gt => (a as i32) > (self.regs.r[src2 as usize] as i32),
                    CmpJumpKind::Gtu => a > self.regs.r[src2 as usize],
                    CmpJumpKind::Lt => (a as i32) < (self.regs.r[src2 as usize] as i32),
                    CmpJumpKind::Ltu => a < self.regs.r[src2 as usize],
                    CmpJumpKind::EqImm(imm) => (a as i32) == imm,
                    CmpJumpKind::GtImm(imm) => (a as i32) > imm,
                    CmpJumpKind::GtuImm(imm) => a > imm,
                    CmpJumpKind::EqN1 => (a as i32) == -1,
                    CmpJumpKind::GtN1 => (a as i32) > -1,
                    CmpJumpKind::TstBit0 => a & 1 != 0,
                };
                // The predicate-writing (`_jump`) forms write P0/P1 with the
                // compare result; the `_jumpnv` forms write no predicate.
                let branch_result = if let Some(p) = write_pred {
                    merge_predicate(new_p, p, if result { 0xff } else { 0 });
                    new_p[p as usize].is_some_and(|value| value & 1 != 0)
                } else {
                    result
                };
                // Predicate-writing forms consume the aggregate P0/P1.new;
                // new-value register comparisons branch on their own result.
                if branch_result == sense {
                    let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                    self.set_branch(branch, target, false)?;
                }
            }
            DecodedInsn::Call { offset, pred } => {
                let take = pred.map_or(true, |cond| self.eval_pred(cond, new_p));
                if take {
                    let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                    self.set_branch(branch, target, true)?;
                }
            }
            DecodedInsn::CallReg { src, pred } => {
                let take = pred.map_or(true, |cond| self.eval_pred(cond, new_p));
                if take {
                    let target = self.regs.r[src as usize] & !0x3;
                    self.set_branch(branch, target, true)?;
                }
            }
            DecodedInsn::Cmp {
                pred,
                src1,
                src2,
                kind,
            } => {
                let a = self.regs.r[src1 as usize];
                let b = self.regs.r[src2 as usize];
                let result = match kind {
                    CmpKind::Eq => a == b,
                    CmpKind::Gt => (a as i32) > (b as i32),
                    CmpKind::Gtu => a > b,
                    CmpKind::Ne => a != b,
                    CmpKind::Lte => (a as i32) <= (b as i32),
                    CmpKind::Lteu => a <= b,
                    CmpKind::Gte => (a as i32) >= (b as i32),
                };
                merge_predicate(new_p, pred, if result { 0xff } else { 0 });
            }
            DecodedInsn::CmpImm {
                pred,
                src,
                imm,
                kind,
                unsigned,
            } => {
                let a = self.regs.r[src as usize];
                let result = if unsigned {
                    let b = imm as u32;
                    match kind {
                        CmpKind::Eq => a == b,
                        CmpKind::Gt => (a as i32) > (b as i32),
                        CmpKind::Gtu => a > b,
                        CmpKind::Ne => a != b,
                        CmpKind::Lte => (a as i32) <= (b as i32),
                        CmpKind::Lteu => a <= b,
                        CmpKind::Gte => (a as i32) >= (b as i32),
                    }
                } else {
                    let b = imm as i32;
                    match kind {
                        CmpKind::Eq => (a as i32) == b,
                        CmpKind::Gt => (a as i32) > b,
                        CmpKind::Gtu => a > b as u32,
                        CmpKind::Ne => (a as i32) != b,
                        CmpKind::Lte => (a as i32) <= b,
                        CmpKind::Lteu => a <= b as u32,
                        CmpKind::Gte => (a as i32) >= b,
                    }
                };
                merge_predicate(new_p, pred, if result { 0xff } else { 0 });
            }
            DecodedInsn::Mul { dst, src1, src2 } => {
                let val = self.regs.r[src1 as usize].wrapping_mul(self.regs.r[src2 as usize]);
                new_r[dst as usize] = Some(val);
            }
            DecodedInsn::ShiftImm {
                dst,
                src,
                kind,
                amount,
            } => {
                let val = self.regs.r[src as usize];
                let shamt = (amount & 0x1f) as u32;
                let result = match kind {
                    ShiftKind::Lsl => val.wrapping_shl(shamt),
                    ShiftKind::Lsr => val.wrapping_shr(shamt),
                    ShiftKind::Asr => ((val as i32) >> shamt) as u32,
                };
                new_r[dst as usize] = Some(result);
            }
            DecodedInsn::ShiftReg {
                dst,
                src,
                amt,
                kind,
            } => {
                let val = self.regs.r[src as usize];
                let rt = self.regs.r[amt as usize];
                new_r[dst as usize] = Some(hex_reg_shift32(val, rt, kind));
            }
            DecodedInsn::TfrCrR { dst, src } => {
                new_r[dst as usize] = Some(self.regs.control(src as usize));
            }
            DecodedInsn::TfrRrCr { dst, src } => {
                let val = self.regs.r[src as usize];
                self.regs.set_control(dst as usize, val);
            }
            DecodedInsn::TfrCrRPair { dst, src } => {
                // Rdd = Css: read the even/odd control-register pair into a GPR pair.
                new_r[dst as usize] = Some(self.regs.control(src as usize));
                new_r[dst as usize + 1] = Some(self.regs.control(src as usize + 1));
            }
            DecodedInsn::TfrRrCrPair { dst, src } => {
                // Cdd = Rss: write a GPR pair into the control-register pair.
                let lo = self.regs.r[src as usize];
                let hi = self.regs.r[src as usize + 1];
                self.regs.set_control(dst as usize, lo);
                self.regs.set_control(dst as usize + 1, hi);
            }
            DecodedInsn::DcZero { base } => {
                // Zero the 32-byte cache line containing Rs (aligned down to 32).
                let ea = self.regs.r[base as usize] & !31;
                self.mem
                    .write_slice(&[0u8; 32], GuestAddress(ea as u64))
                    .map_err(|e| Error::Emulator(format!("dczeroa write: {e}")))?;
            }
            DecodedInsn::LoopStartReg {
                loop_id,
                start_offset,
                count_reg,
                lpcfg,
            } => {
                let count = self.regs.r[count_reg as usize];
                let sa = packet_pc.wrapping_add(start_offset as u32) & !0x3;
                if loop_id == 0 {
                    self.regs.c[0] = sa;
                    self.regs.c[1] = count;
                } else {
                    self.regs.c[2] = sa;
                    self.regs.c[3] = count;
                }
                if let Some(n) = lpcfg {
                    self.set_lpcfg(n as u32);
                    new_p[3] = Some(0);
                }
            }
            DecodedInsn::LoopStartImm {
                loop_id,
                start_offset,
                count,
                lpcfg,
            } => {
                let sa = packet_pc.wrapping_add(start_offset as u32) & !0x3;
                if loop_id == 0 {
                    self.regs.c[0] = sa;
                    self.regs.c[1] = count;
                } else {
                    self.regs.c[2] = sa;
                    self.regs.c[3] = count;
                }
                if let Some(n) = lpcfg {
                    self.set_lpcfg(n as u32);
                    new_p[3] = Some(0);
                }
            }
            DecodedInsn::Trap0 => {
                return Ok(Some(VcpuExit::Shutdown));
            }
            // Read-modify-write memory op: mem[Rs+#off] OP= (Rt | #imm | bit).
            DecodedInsn::MemOp {
                base,
                offset,
                width,
                op,
                src,
            } => {
                let ea = self.regs.r[base as usize].wrapping_add(offset as u32);
                // A read-modify-write memop on the serial/debug MMIO range would
                // need two device transactions (an MMIO read and an MMIO write)
                // that the single-exit MMIO model can't express, and these
                // output devices have no meaningful RMW semantics. Reject it
                // rather than silently read-modify-write the RAM that may back
                // the MMIO address (the load/store paths dispatch via is_mmio;
                // this path must not bypass that). (#170)
                if Self::is_mmio(ea) {
                    return Err(Error::Emulator(
                        "hexagon memop on MMIO address not supported".to_string(),
                    ));
                }
                let cur = self.load_mem(ea, width, MemSign::Unsigned)?;
                let srcval = match src {
                    MemOpSrc::Reg(reg) => self.regs.r[reg as usize],
                    MemOpSrc::Imm(value) => value,
                };
                let result = match op {
                    MemOpKind::Add => cur.wrapping_add(srcval),
                    MemOpKind::Sub => cur.wrapping_sub(srcval),
                    MemOpKind::And => cur & srcval,
                    MemOpKind::Or => cur | srcval,
                    MemOpKind::ClrBit => cur & !(1u32 << (srcval & 0x1f)),
                    MemOpKind::SetBit => cur | (1u32 << (srcval & 0x1f)),
                };
                self.store_mem(ea, width, result)?;
            }
            // HVX vector load: Vd = vmem(base + offset) (buffered to new_v).
            DecodedInsn::VLoad {
                dst,
                base,
                offset,
                post_inc,
                post_inc_mod,
                aligned,
                commit,
                pred,
            } => {
                // Scalar-predicated load: CANCEL (no register write, no
                // post-increment) when the predicate is false.
                let do_load = match pred {
                    Some(cond) => self.eval_pred(cond, new_p),
                    None => true,
                };
                if do_load {
                    let mut ea = self.regs.r[base as usize].wrapping_add(offset as u32);
                    if aligned {
                        ea &= !127;
                    }
                    // Read always (a `.tmp` load still faults on a bad address). A
                    // normal/`.cur` load buffers into `new_v` (committed at packet
                    // end); a `.tmp` load forwards into the scratch buffer so a
                    // same-packet consumer (e.g. vhist) sees it, but it is never
                    // committed architecturally.
                    let vec = self.read_vec(ea)?;
                    if commit {
                        self.new_v[dst as usize] = Some(vec);
                        self.last_v_produced = Some(dst);
                    } else {
                        self.new_v_tmp[dst as usize] = Some(vec);
                    }
                    // Post-increment: `pi` by the (vector-unit) immediate, `ppu` by
                    // the modifier register Mu (full byte value).
                    let inc = post_inc
                        .map(|i| i as u32)
                        .or_else(|| post_inc_mod.map(|m| self.modifier(m)));
                    if let Some(inc) = inc {
                        new_r[base as usize] = Some(self.regs.r[base as usize].wrapping_add(inc));
                    }
                }
            }
            // HVX vector store: vmem(base + offset) = Vs.
            DecodedInsn::VStore {
                src,
                base,
                offset,
                post_inc,
                post_inc_mod,
                aligned,
                pred,
                qmask,
                from_gather,
                srls,
            } => {
                // Scalar-predicated store: CANCEL (no write, no post-increment)
                // when the predicate is false. A byte-masked (qmask) store always
                // executes; it only masks which bytes change.
                let do_store = match pred {
                    Some(cond) => self.eval_pred(cond, new_p),
                    None => true,
                };
                if do_store {
                    let mut ea = self.regs.r[base as usize].wrapping_add(offset as u32);
                    if aligned {
                        ea &= !127;
                    }
                    // `srls` (scatter_release) is a barrier with no vector source:
                    // it performs no memory write (only the post-increment, below).
                    let mut bytes = if srls {
                        None
                    } else if from_gather {
                        // New-value vector store `vmem(...) = V.new`. Normally the
                        // source is the vector produced earlier in this packet
                        // (`new_v[last producer]`). For the gather idiom
                        // (`vmem=vtmp.new`, no architectural vector producer), it
                        // is the gather scratch: only the gathered (valid) bytes
                        // are written, the rest of memory is preserved (the gather
                        // temp is seeded from this memory on hardware).
                        if let Some(p) = self.last_v_produced {
                            let v = self.new_v[p as usize].unwrap_or(self.regs.v[p as usize]);
                            Some(vec_to_bytes(&v))
                        } else {
                            let (data, valid) =
                                self.gather_tmp.unwrap_or(([0u8; 128], [false; 128]));
                            let cur = vec_to_bytes(&self.read_vec(ea)?);
                            let mut b = cur;
                            for i in 0..128 {
                                if valid[i] {
                                    b[i] = data[i];
                                }
                            }
                            Some(b)
                        }
                    } else {
                        Some(vec_to_bytes(&self.regs.v[src as usize]))
                    };
                    if let (Some(bytes), Some((q, sense))) = (bytes.as_mut(), qmask) {
                        // Read-modify-write: keep existing bytes where the Q mask
                        // doesn't select. The Qv operand reads the OLD predicate
                        // (it is not a `.new` operand).
                        let qv = self.regs.q[q as usize];
                        let cur = vec_to_bytes(&self.read_vec(ea)?);
                        for i in 0..128 {
                            let bit = (qv[i / 32] >> (i % 32)) & 1 != 0;
                            if bit != sense {
                                bytes[i] = cur[i];
                            }
                        }
                    }
                    if let Some(bytes) = bytes {
                        self.write_vec(ea, &bytes_to_vec(&bytes))?;
                    }
                    // Post-increment: `pi` by the (vector-unit) immediate, `ppu` by
                    // the modifier register Mu. The `srls` scatter-release barrier
                    // is a pure ordering hint and does NOT write back the base, so
                    // its `_pi`/`_ppu` forms perform no post-increment (matching
                    // qemu-hexagon).
                    let inc = if srls {
                        None
                    } else {
                        post_inc
                            .map(|i| i as u32)
                            .or_else(|| post_inc_mod.map(|m| self.modifier(m)))
                    };
                    if let Some(inc) = inc {
                        new_r[base as usize] = Some(self.regs.r[base as usize].wrapping_add(inc));
                    }
                }
            }
            // HVX V65 scatter: store/accumulate elements into the region.
            DecodedInsn::VScatter {
                base,
                modsel,
                offsets,
                data,
                esz,
                off_pair,
                add,
                pred,
            } => {
                self.exec_scatter(base, modsel, offsets, data, esz, off_pair, add, pred)?;
            }
            // HVX V65 gather: build the gathered vector into the per-packet scratch
            // (`gather_tmp`), consumed by the paired `vmem=vtmp.new` store. The
            // gather is also pre-run at packet start (the assembler encodes it
            // AFTER its consuming store); re-running here is idempotent.
            DecodedInsn::VGather {
                base,
                modsel,
                offsets,
                esz,
                off_pair,
                pred,
            } => {
                self.gather_tmp =
                    Some(self.exec_gather(base, modsel, offsets, esz, off_pair, pred)?);
            }
            // Absolute value: Rd = |Rs|, with optional saturation
            DecodedInsn::Abs { dst, src, sat } => {
                let val = self.regs.r[src as usize] as i32;
                let result = if sat {
                    // Saturating absolute value
                    // Special case: 0x80000000 saturates to 0x7fffffff
                    if val == i32::MIN {
                        // Set overflow flag in USR (C8)
                        self.regs.c[8] |= 1; // OVF bit
                        i32::MAX as u32
                    } else {
                        val.abs() as u32
                    }
                } else {
                    // Non-saturating: wraps on MIN_VALUE
                    val.wrapping_abs() as u32
                };
                new_r[dst as usize] = Some(result);
            }
            // Saturating negation: Rd = sat(-Rs)
            DecodedInsn::NegSat { dst, src } => {
                let val = self.regs.r[src as usize] as i32;
                // Special case: -0x80000000 saturates to 0x7fffffff
                let result = if val == i32::MIN {
                    // Set overflow flag in USR (C8)
                    self.regs.c[8] |= 1; // OVF bit
                    i32::MAX as u32
                } else {
                    (-val) as u32
                };
                new_r[dst as usize] = Some(result);
            }
            // Signed maximum: Rd = max(Rs, Rt)
            DecodedInsn::Max { dst, src1, src2 } => {
                let a = self.regs.r[src1 as usize] as i32;
                let b = self.regs.r[src2 as usize] as i32;
                let result = if a > b { a } else { b };
                new_r[dst as usize] = Some(result as u32);
            }
            // Unsigned maximum: Rd = maxu(Rs, Rt)
            DecodedInsn::Maxu { dst, src1, src2 } => {
                let a = self.regs.r[src1 as usize];
                let b = self.regs.r[src2 as usize];
                let result = if a > b { a } else { b };
                new_r[dst as usize] = Some(result);
            }
            // Signed minimum: Rd = min(Rs, Rt)
            DecodedInsn::Min { dst, src1, src2 } => {
                let a = self.regs.r[src1 as usize] as i32;
                let b = self.regs.r[src2 as usize] as i32;
                let result = if a < b { a } else { b };
                new_r[dst as usize] = Some(result as u32);
            }
            // Unsigned minimum: Rd = minu(Rs, Rt)
            DecodedInsn::Minu { dst, src1, src2 } => {
                let a = self.regs.r[src1 as usize];
                let b = self.regs.r[src2 as usize];
                let result = if a < b { a } else { b };
                new_r[dst as usize] = Some(result);
            }
            DecodedInsn::StoreNew { .. } => {
                // New-value stores are resolved to a regular Store by the packet
                // driver before reaching here; seeing one is a logic error.
                return Err(Error::Emulator("unresolved new-value store".to_string()));
            }
            DecodedInsn::LoadLocked { dst, base, width } => {
                // Plain word/dword load at `Rs+0` that also sets the LL
                // reservation on the accessed address.
                let addr = self.regs.r[base as usize];
                if Self::is_mmio(addr) {
                    return Err(Error::Emulator(
                        "atomic load from mmio not supported".to_string(),
                    ));
                }
                if width == MemWidth::Double {
                    let even = dst & !1;
                    let odd = even.wrapping_add(1);
                    if odd >= 32 {
                        return Err(Error::Emulator(
                            "invalid register pair for doubleword load".to_string(),
                        ));
                    }
                    let val = self.read_u64(addr)?;
                    new_r[even as usize] = Some(val as u32);
                    new_r[odd as usize] = Some((val >> 32) as u32);
                } else {
                    new_r[dst as usize] = Some(self.read_u32(addr)?);
                }
                self.lock_addr = Some(addr);
            }
            DecodedInsn::StoreCond {
                src,
                base,
                width,
                success_pred,
            } => {
                // Store-conditional / store-release at `Rs+0`.
                let addr = self.regs.r[base as usize];
                if Self::is_mmio(addr) {
                    return Err(Error::Emulator(
                        "atomic store to mmio not supported".to_string(),
                    ));
                }
                // A `_locked` store-conditional succeeds only when the LL
                // reservation matches this address (a bare SC fails). Release
                // stores (`success_pred == None`) always store unconditionally.
                let succeed = match success_pred {
                    Some(_) => self.lock_addr == Some(addr),
                    None => true,
                };
                // Any store-conditional attempt clears the reservation.
                if success_pred.is_some() {
                    self.lock_addr = None;
                }
                if succeed {
                    if width == MemWidth::Double {
                        let even = src & !1;
                        let odd = even.wrapping_add(1);
                        if odd >= 32 {
                            return Err(Error::Emulator(
                                "invalid register pair for doubleword store".to_string(),
                            ));
                        }
                        let combined = ((self.regs.r[odd as usize] as u64) << 32)
                            | self.regs.r[even as usize] as u64;
                        self.write_u64(addr, combined)?;
                    } else {
                        self.store_mem(addr, width, self.regs.r[src as usize])?;
                    }
                }
                // The `_locked` forms set Pd to the success result (TRUE on a held
                // reservation, FALSE otherwise); release stores set no predicate.
                if let Some(pd) = success_pred {
                    new_p[pd as usize] = Some(if succeed { 0xff } else { 0 });
                }
            }
            DecodedInsn::Vsplice {
                dst,
                src_low,
                src_high,
                amount,
            } => {
                // Rdd = Rtt << N*8 | zxt(N*8)(Rss): low N bytes from Rss, high
                // (8-N) bytes from Rtt. N is #u3 or Pu&7 (range 0..=7, so the
                // shift is always < 64 and well-defined).
                let n = match amount {
                    SpliceAmount::Imm(v) => (v & 0x7) as u32,
                    SpliceAmount::Pred(p) => (self.regs.p[p as usize] & 0x7) as u32,
                };
                let bits = n * 8;
                let rss_even = src_low & !1;
                let rss_odd = rss_even.wrapping_add(1);
                let rtt_even = src_high & !1;
                let rtt_odd = rtt_even.wrapping_add(1);
                let rss = ((self.regs.r[rss_odd as usize] as u64) << 32)
                    | self.regs.r[rss_even as usize] as u64;
                let rtt = ((self.regs.r[rtt_odd as usize] as u64) << 32)
                    | self.regs.r[rtt_even as usize] as u64;
                let low_mask = if bits == 0 { 0 } else { (1u64 << bits) - 1 };
                let result = (rtt << bits) | (rss & low_mask);
                let even = dst & !1;
                let odd = even.wrapping_add(1);
                new_r[even as usize] = Some(result as u32);
                new_r[odd as usize] = Some((result >> 32) as u32);
            }
            DecodedInsn::Unknown(word) => {
                // Fall through to the direct opcode-dispatch semantic layer,
                // which handles instructions not modelled by the DecodedInsn IR.
                if !self.try_sem(word, immext, new_r, new_p)? {
                    return Err(Error::Emulator(format!(
                        "unknown hexagon instruction 0x{word:08x}"
                    )));
                }
            }
        }

        Ok(None)
    }
}
