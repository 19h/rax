use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use super::decode::{
    decode, AddrMode, CmpKind, DecodedInsn, MemSign, MemWidth, ShiftKind,
};
use crate::config::{Endianness, HexagonIsa};
use crate::cpu::{CpuState, HexagonRegisters, VCpu, VcpuExit};
use crate::error::{Error, Result};

const SERIAL_MMIO_BASE: u64 = 0xf000_0000;
const SERIAL_MMIO_LEN: u64 = 8;
const MAX_RUN_ITERATIONS: u64 = 100_000;

struct MmioPending {
    dst: u8,
    size: u8,
    signed: bool,
}

struct BranchTarget {
    target: u32,
    is_call: bool,
}

pub struct HexagonVcpu {
    id: u32,
    regs: HexagonRegisters,
    mem: Arc<GuestMemoryMmap>,
    halted: bool,
    pending_mmio: Option<MmioPending>,
    immext: Option<u32>,
    _isa: HexagonIsa,
    endian: Endianness,
}

impl HexagonVcpu {
    pub fn new(id: u32, mem: Arc<GuestMemoryMmap>, isa: HexagonIsa, endian: Endianness) -> Self {
        HexagonVcpu {
            id,
            regs: HexagonRegisters::default(),
            mem,
            halted: false,
            pending_mmio: None,
            immext: None,
            _isa: isa,
            endian,
        }
    }

    fn read_u8(&self, addr: u32) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.mem
            .read_slice(&mut buf, GuestAddress(addr as u64))
            .map_err(Error::from)?;
        Ok(buf[0])
    }

    fn read_u16(&self, addr: u32) -> Result<u16> {
        let mut buf = [0u8; 2];
        self.mem
            .read_slice(&mut buf, GuestAddress(addr as u64))
            .map_err(Error::from)?;
        Ok(match self.endian {
            Endianness::Little => u16::from_le_bytes(buf),
            Endianness::Big => u16::from_be_bytes(buf),
        })
    }

    fn read_u32(&self, addr: u32) -> Result<u32> {
        let mut buf = [0u8; 4];
        self.mem
            .read_slice(&mut buf, GuestAddress(addr as u64))
            .map_err(Error::from)?;
        Ok(match self.endian {
            Endianness::Little => u32::from_le_bytes(buf),
            Endianness::Big => u32::from_be_bytes(buf),
        })
    }

    fn write_u8(&self, addr: u32, value: u8) -> Result<()> {
        self.mem
            .write_slice(&[value], GuestAddress(addr as u64))
            .map_err(Error::from)?;
        Ok(())
    }

    fn write_u16(&self, addr: u32, value: u16) -> Result<()> {
        let bytes = match self.endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        };
        self.mem
            .write_slice(&bytes, GuestAddress(addr as u64))
            .map_err(Error::from)?;
        Ok(())
    }

    fn write_u32(&self, addr: u32, value: u32) -> Result<()> {
        let bytes = match self.endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        };
        self.mem
            .write_slice(&bytes, GuestAddress(addr as u64))
            .map_err(Error::from)?;
        Ok(())
    }

    fn fetch_word(&self, pc: u32) -> Result<u32> {
        self.read_u32(pc)
    }

    fn is_mmio(addr: u32) -> bool {
        let addr = addr as u64;
        addr >= SERIAL_MMIO_BASE && addr < SERIAL_MMIO_BASE + SERIAL_MMIO_LEN
    }

    fn set_pc(&mut self, pc: u32) {
        self.regs.set_pc(pc);
    }

    fn load_mem(&self, addr: u32, width: MemWidth, sign: MemSign) -> Result<u32> {
        match width {
            MemWidth::Byte => {
                let val = self.read_u8(addr)?;
                Ok(match sign {
                    MemSign::Signed => (val as i8 as i32) as u32,
                    MemSign::Unsigned => val as u32,
                })
            }
            MemWidth::Half => {
                let val = self.read_u16(addr)?;
                Ok(match sign {
                    MemSign::Signed => (val as i16 as i32) as u32,
                    MemSign::Unsigned => val as u32,
                })
            }
            MemWidth::Word => self.read_u32(addr),
            MemWidth::Double => Err(Error::Emulator(
                "doubleword load not supported yet".to_string(),
            )),
        }
    }

    fn store_mem(&self, addr: u32, width: MemWidth, value: u32) -> Result<()> {
        match width {
            MemWidth::Byte => self.write_u8(addr, value as u8),
            MemWidth::Half => self.write_u16(addr, value as u16),
            MemWidth::Word => self.write_u32(addr, value),
            MemWidth::Double => Err(Error::Emulator(
                "doubleword store not supported yet".to_string(),
            )),
        }
    }

    fn commit_packet(&mut self, new_r: &[Option<u32>; 32], new_p: &[Option<bool>; 4]) {
        for (idx, val) in new_r.iter().enumerate() {
            if let Some(value) = val {
                self.regs.r[idx] = *value;
            }
        }
        for (idx, val) in new_p.iter().enumerate() {
            if let Some(value) = val {
                self.regs.set_predicate(idx, *value);
            }
        }
    }

    fn step_packet(&mut self) -> Result<Option<VcpuExit>> {
        let packet_pc = self.regs.pc();
        let mut pc = packet_pc;
        self.immext = None;

        let mut new_r = [None; 32];
        let mut new_p = [None; 4];
        let mut branch: Option<BranchTarget> = None;
        let mut inst_index = 0usize;
        let mut first_parse: Option<u32> = None;
        let mut second_parse: Option<u32> = None;

        loop {
            let word = self.fetch_word(pc)?;
            let parse = (word >> 14) & 0x3;
            if parse == 0 {
                return Err(Error::Emulator(
                    "duplex instruction encoding not supported".to_string(),
                ));
            }

            if inst_index == 0 {
                first_parse = Some(parse);
            } else if inst_index == 1 {
                second_parse = Some(parse);
            }

            let decoded = decode(word, self.immext);
            self.immext = None;

            match decoded.insn {
                DecodedInsn::ImmExt { value } => {
                    self.immext = Some(value);
                    pc = pc.wrapping_add(4);
                    inst_index += 1;
                    if parse == 0x3 {
                        self.commit_packet(&new_r, &new_p);
                        self.set_pc(pc);
                        return Ok(None);
                    }
                    continue;
                }
                DecodedInsn::Add { dst, src1, src2 } => {
                    let val = self.regs.r[src1 as usize]
                        .wrapping_add(self.regs.r[src2 as usize]);
                    new_r[dst as usize] = Some(val);
                }
                DecodedInsn::Sub { dst, src1, src2 } => {
                    let val = self.regs.r[src1 as usize]
                        .wrapping_sub(self.regs.r[src2 as usize]);
                    new_r[dst as usize] = Some(val);
                }
                DecodedInsn::And { dst, src1, src2 } => {
                    new_r[dst as usize] =
                        Some(self.regs.r[src1 as usize] & self.regs.r[src2 as usize]);
                }
                DecodedInsn::Or { dst, src1, src2 } => {
                    new_r[dst as usize] =
                        Some(self.regs.r[src1 as usize] | self.regs.r[src2 as usize]);
                }
                DecodedInsn::Xor { dst, src1, src2 } => {
                    new_r[dst as usize] =
                        Some(self.regs.r[src1 as usize] ^ self.regs.r[src2 as usize]);
                }
                DecodedInsn::AddImm { dst, src, imm } => {
                    let val = self.regs.r[src as usize].wrapping_add(imm as u32);
                    new_r[dst as usize] = Some(val);
                }
                DecodedInsn::Mov { dst, src } => {
                    new_r[dst as usize] = Some(self.regs.r[src as usize]);
                }
                DecodedInsn::MovImm { dst, imm } => {
                    new_r[dst as usize] = Some(imm as u32);
                }
                DecodedInsn::Load {
                    dst,
                    addr,
                    width,
                    sign,
                } => {
                    let (addr, update) = match addr {
                        AddrMode::Offset { base, offset } => {
                            let base_val = self.regs.r[base as usize];
                            (base_val.wrapping_add(offset as u32), None)
                        }
                        AddrMode::PostIncImm { base, offset } => {
                            let base_val = self.regs.r[base as usize];
                            let new_base = base_val.wrapping_add(offset as u32);
                            (base_val, Some((base, new_base)))
                        }
                        AddrMode::GpOffset { offset } => {
                            let gp = self.regs.control(11);
                            (gp.wrapping_add(offset as u32), None)
                        }
                        AddrMode::Abs { addr } => (addr, None),
                    };

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
                                ))
                            }
                        };
                        let signed = matches!(sign, MemSign::Signed) && size < 4;
                        self.pending_mmio = Some(MmioPending { dst, size, signed });
                        self.commit_packet(&new_r, &new_p);
                        self.set_pc(pc.wrapping_add(4));
                        return Ok(Some(VcpuExit::MmioRead {
                            addr: addr as u64,
                            size,
                        }));
                    }

                    let val = self.load_mem(addr, width, sign)?;
                    new_r[dst as usize] = Some(val);
                }
                DecodedInsn::Store { src, addr, width } => {
                    let (addr, update) = match addr {
                        AddrMode::Offset { base, offset } => {
                            let base_val = self.regs.r[base as usize];
                            (base_val.wrapping_add(offset as u32), None)
                        }
                        AddrMode::PostIncImm { base, offset } => {
                            let base_val = self.regs.r[base as usize];
                            let new_base = base_val.wrapping_add(offset as u32);
                            (base_val, Some((base, new_base)))
                        }
                        AddrMode::GpOffset { offset } => {
                            let gp = self.regs.control(11);
                            (gp.wrapping_add(offset as u32), None)
                        }
                        AddrMode::Abs { addr } => (addr, None),
                    };

                    if let Some((reg, value)) = update {
                        new_r[reg as usize] = Some(value);
                    }

                    let val = self.regs.r[src as usize];
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
                                ))
                            }
                        };
                        self.commit_packet(&new_r, &new_p);
                        self.set_pc(pc.wrapping_add(4));
                        return Ok(Some(VcpuExit::MmioWrite {
                            addr: addr as u64,
                            data,
                        }));
                    }
                    self.store_mem(addr, width, val)?;
                }
                DecodedInsn::Jump { offset } => {
                    let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                    if branch.is_some() {
                        return Err(Error::Emulator(
                            "multiple branches in packet".to_string(),
                        ));
                    }
                    branch = Some(BranchTarget {
                        target,
                        is_call: false,
                    });
                }
                DecodedInsn::JumpCond {
                    offset,
                    pred,
                    sense,
                    pred_new,
                } => {
                    let pred_val = if pred_new {
                        new_p[pred as usize].unwrap_or(self.regs.p[pred as usize])
                    } else {
                        self.regs.p[pred as usize]
                    };
                    let take = if sense { pred_val } else { !pred_val };
                    if take {
                        let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                        if branch.is_some() {
                            return Err(Error::Emulator(
                                "multiple branches in packet".to_string(),
                            ));
                        }
                        branch = Some(BranchTarget {
                            target,
                            is_call: false,
                        });
                    }
                }
                DecodedInsn::JumpReg { src } => {
                    let target = self.regs.r[src as usize] & !0x3;
                    if branch.is_some() {
                        return Err(Error::Emulator(
                            "multiple branches in packet".to_string(),
                        ));
                    }
                    branch = Some(BranchTarget {
                        target,
                        is_call: false,
                    });
                }
                DecodedInsn::JumpRegCond {
                    src,
                    pred,
                    sense,
                    pred_new,
                } => {
                    let pred_val = if pred_new {
                        new_p[pred as usize].unwrap_or(self.regs.p[pred as usize])
                    } else {
                        self.regs.p[pred as usize]
                    };
                    let take = if sense { pred_val } else { !pred_val };
                    if take {
                        let target = self.regs.r[src as usize] & !0x3;
                        if branch.is_some() {
                            return Err(Error::Emulator(
                                "multiple branches in packet".to_string(),
                            ));
                        }
                        branch = Some(BranchTarget {
                            target,
                            is_call: false,
                        });
                    }
                }
                DecodedInsn::Call { offset } => {
                    let target = packet_pc.wrapping_add(offset as u32) & !0x3;
                    if branch.is_some() {
                        return Err(Error::Emulator(
                            "multiple branches in packet".to_string(),
                        ));
                    }
                    branch = Some(BranchTarget {
                        target,
                        is_call: true,
                    });
                }
                DecodedInsn::CallReg { src } => {
                    let target = self.regs.r[src as usize] & !0x3;
                    if branch.is_some() {
                        return Err(Error::Emulator(
                            "multiple branches in packet".to_string(),
                        ));
                    }
                    branch = Some(BranchTarget {
                        target,
                        is_call: true,
                    });
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
                    };
                    new_p[pred as usize] = Some(result);
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
                        }
                    };
                    new_p[pred as usize] = Some(result);
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
                    let shamt = (self.regs.r[amt as usize] & 0x1f) as u32;
                    let result = match kind {
                        ShiftKind::Lsl => val.wrapping_shl(shamt),
                        ShiftKind::Lsr => val.wrapping_shr(shamt),
                        ShiftKind::Asr => ((val as i32) >> shamt) as u32,
                    };
                    new_r[dst as usize] = Some(result);
                }
                DecodedInsn::TfrCrR { dst, src } => {
                    new_r[dst as usize] = Some(self.regs.control(src as usize));
                }
                DecodedInsn::TfrRrCr { dst, src } => {
                    let val = self.regs.r[src as usize];
                    self.regs.set_control(dst as usize, val);
                }
                DecodedInsn::LoopStartReg {
                    loop_id,
                    start_offset,
                    count_reg,
                } => {
                    let count = self.regs.r[count_reg as usize];
                    if loop_id == 0 {
                        self.regs.c[0] = packet_pc.wrapping_add(start_offset as u32) & !0x3;
                        self.regs.c[1] = count;
                    } else {
                        self.regs.c[2] = packet_pc.wrapping_add(start_offset as u32) & !0x3;
                        self.regs.c[3] = count;
                    }
                }
                DecodedInsn::LoopStartImm {
                    loop_id,
                    start_offset,
                    count,
                } => {
                    if loop_id == 0 {
                        self.regs.c[0] = packet_pc.wrapping_add(start_offset as u32) & !0x3;
                        self.regs.c[1] = count;
                    } else {
                        self.regs.c[2] = packet_pc.wrapping_add(start_offset as u32) & !0x3;
                        self.regs.c[3] = count;
                    }
                }
                DecodedInsn::Trap0 => {
                    self.commit_packet(&new_r, &new_p);
                    self.set_pc(pc.wrapping_add(4));
                    return Ok(Some(VcpuExit::Shutdown));
                }
                DecodedInsn::Unknown(word) => {
                    return Err(Error::Emulator(format!(
                        "unknown hexagon instruction 0x{word:08x} at pc=0x{pc:08x}"
                    )));
                }
            }

            pc = pc.wrapping_add(4);
            inst_index += 1;
            if parse == 0x3 {
                break;
            }
        }

        self.commit_packet(&new_r, &new_p);
        let packet_end = pc;

        let (loop0_end, loop1_end) = match (first_parse, second_parse) {
            (Some(first), Some(second)) => {
                let loop0 = first == 0b10 && second != 0;
                let loop1 = second == 0b10 && first != 0;
                (loop0, loop1)
            }
            _ => (false, false),
        };

        if let Some(branch) = branch {
            if branch.is_call {
                self.regs.r[31] = packet_end;
            }
            self.set_pc(branch.target);
        } else if loop0_end && self.regs.c[1] > 1 {
            self.regs.c[1] = self.regs.c[1].wrapping_sub(1);
            self.set_pc(self.regs.c[0]);
        } else if loop1_end && self.regs.c[3] > 1 {
            self.regs.c[3] = self.regs.c[3].wrapping_sub(1);
            self.set_pc(self.regs.c[2]);
        } else {
            self.set_pc(packet_end);
        }

        Ok(None)
    }
}

impl VCpu for HexagonVcpu {
    fn run(&mut self) -> Result<VcpuExit> {
        if self.halted {
            return Ok(VcpuExit::Hlt);
        }
        let mut iterations = 0u64;
        loop {
            iterations += 1;
            if iterations > MAX_RUN_ITERATIONS {
                return Err(Error::Emulator(format!(
                    "exceeded {} iterations at pc=0x{:08x}",
                    MAX_RUN_ITERATIONS,
                    self.regs.pc()
                )));
            }

            if let Some(exit) = self.step_packet()? {
                return Ok(exit);
            }
        }
    }

    fn get_state(&self) -> Result<CpuState> {
        Ok(CpuState::hexagon(self.regs.clone()))
    }

    fn set_state(&mut self, state: &CpuState) -> Result<()> {
        let state = match state {
            CpuState::Hexagon(state) => state,
            _ => {
                return Err(Error::Emulator(
                    "expected hexagon state for hexagon vCPU".to_string(),
                ))
            }
        };
        self.regs = state.regs.clone();
        Ok(())
    }

    fn complete_io_in(&mut self, data: &[u8]) {
        if let Some(pending) = self.pending_mmio.take() {
            let val = match pending.size {
                1 if data.len() >= 1 => {
                    let raw = data[0] as u32;
                    if pending.signed {
                        (raw as i8 as i32) as u32
                    } else {
                        raw
                    }
                }
                2 if data.len() >= 2 => {
                    let raw = match self.endian {
                        Endianness::Little => u16::from_le_bytes([data[0], data[1]]) as u32,
                        Endianness::Big => u16::from_be_bytes([data[0], data[1]]) as u32,
                    };
                    if pending.signed {
                        (raw as i16 as i32) as u32
                    } else {
                        raw
                    }
                }
                4 if data.len() >= 4 => match self.endian {
                    Endianness::Little => {
                        u32::from_le_bytes([data[0], data[1], data[2], data[3]])
                    }
                    Endianness::Big => u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
                },
                _ => return,
            };
            self.regs.r[pending.dst as usize] = val;
        }
    }

    fn id(&self) -> u32 {
        self.id
    }
}
