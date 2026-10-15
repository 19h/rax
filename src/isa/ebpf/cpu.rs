//! Direct eBPF (RFC 9669) and Solana SBPF (V0–V4) CPU state, interpreter, and
//! [`VCpu`] implementation.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vm_memory::GuestMemoryMmap;

use crate::error::{Error, Result};
use crate::isa::ebpf::decode::{
    AluWidth, AtomicKind, BinAluOp, EbpfDecodeError, EbpfDialect, EbpfInsn, EbpfMemWidth, EbpfOp,
    EbpfSrc, EndianOp, FRAME_PTR_REG, INSN_SIZE, JmpCond, SbpfVersion, decode_insn,
};
use crate::isa::ebpf::memory::{EbpfMemFault, EbpfMemory, MM_STACK_START};
use crate::smir::ir::types::Endian;
use crate::vm::vcpu::{CpuState, VCpu, VcpuExit};

/// Architectural register state of an eBPF / Solana SBPF processor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbpfRegisters {
    /// General-purpose registers `r0..r10` (`r10` is frame pointer `fp`).
    pub r: [u64; 11],
    /// Program counter in bytes.
    pub pc: u64,
}

impl Default for EbpfRegisters {
    fn default() -> Self {
        let mut r = [0u64; 11];
        // Default stack frame pointer at top of first 4 KiB frame in the stack region.
        r[FRAME_PTR_REG as usize] = MM_STACK_START + 4096;
        Self { r, pc: 0 }
    }
}

impl EbpfRegisters {
    /// Read register `r0..r10`.
    #[inline]
    pub fn gpr(&self, idx: u8) -> u64 {
        self.r[idx as usize]
    }

    /// Write register `r0..r10`.
    #[inline]
    pub fn set_gpr(&mut self, idx: u8, val: u64) {
        self.r[idx as usize] = val;
    }
}

/// Shadow call-stack frame saved across BPF-to-BPF local calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EbpfCallFrame {
    /// Saved callee-preserved registers `r6, r7, r8, r9`.
    pub caller_saved_regs: [u64; 4],
    /// Saved frame pointer `r10`.
    pub frame_pointer: u64,
    /// Return PC (byte address of instruction following `CALL`).
    pub return_pc: u64,
}

/// Runtime trap or fault raised during eBPF / Solana SBPF execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EbpfTrap {
    /// Instruction decode failure at `pc`.
    DecodeError { pc: u64, error: EbpfDecodeError },
    /// Division or remainder by zero (Solana SBPF `V0..V4`).
    DivideByZero { pc: u64 },
    /// Signed division or remainder overflow `MIN / -1` (Solana SBPF `V2` PQR).
    DivideOverflow { pc: u64 },
    /// Memory access violation or unmapped address.
    MemoryFault { pc: u64, fault: EbpfMemFault },
    /// Call stack exceeded `max_call_depth`.
    CallDepthExceeded { pc: u64 },
    /// Branch or call target outside executable text bounds or unaligned.
    CallOutsideTextSegment { pc: u64, target_pc: u64 },
    /// Unregistered syscall or internal function key.
    UnsupportedSyscall { pc: u64, key: u32 },
    /// A platform-specific LDDW subtype requires an unavailable resolver.
    UnsupportedPseudoLoad { pc: u64, src_type: u8 },
    /// Instruction count limit / compute budget exceeded.
    ExceededMaxInstructions { pc: u64 },
}

/// Outcome of a single instruction step or program run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EbpfExit {
    /// Top-level `exit` executed (`call_depth == 0`), returning `r0`.
    Exit { r0: u64 },
    /// External syscall invoked (`CALL_IMM` with `src == 0` or non-static syscall).
    Syscall { key: u32, next_pc: u64 },
    /// Architectural trap or fault occurred.
    Trap(EbpfTrap),
}

/// Configuration for an [`EbpfCpu`] instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EbpfConfig {
    /// Dialect (`Rfc9669` vs `Solana(SbpfVersion)`).
    pub dialect: EbpfDialect,
    /// Byte order for instruction decoding and memory loads/stores.
    pub endian: Endian,
    /// Maximum nested call depth (default `64`).
    pub max_call_depth: usize,
    /// Stack frame size in bytes (default `4096`).
    pub stack_frame_size: u64,
    /// Enable virtual address gaps between fixed stack frames in SBPF `V0`.
    pub enable_stack_frame_gaps: bool,
    /// Optional instruction count limit (compute unit budget).
    pub max_instructions: Option<u64>,
    /// Optional executable `[start, end)` byte range for PC bounds checking.
    pub text_range: Option<(u64, u64)>,
}

impl Default for EbpfConfig {
    fn default() -> Self {
        Self {
            dialect: EbpfDialect::Rfc9669,
            endian: Endian::Little,
            max_call_depth: 64,
            stack_frame_size: 4096,
            enable_stack_frame_gaps: true,
            max_instructions: None,
            text_range: None,
        }
    }
}

impl EbpfConfig {
    /// Create a configuration for the given Solana SBPF version.
    pub fn solana(version: SbpfVersion) -> Self {
        Self {
            dialect: EbpfDialect::Solana(version),
            ..Self::default()
        }
    }
}

/// Direct interpreter CPU for eBPF (RFC 9669) and Solana SBPF (V0–V4).
pub struct EbpfCpu {
    /// Architectural registers (`r0..r10` and `pc`).
    pub regs: EbpfRegisters,
    /// Configuration parameters.
    pub config: EbpfConfig,
    /// Shadow call stack for BPF-to-BPF calls.
    pub call_frames: Vec<EbpfCallFrame>,
    /// Number of instructions executed.
    pub insn_count: u64,
    /// Internal function registry mapping hash/key -> target byte PC (for SBPF V0–V2).
    pub function_registry: HashMap<u32, u64>,
    /// Registered syscall callbacks mapping key -> handler `(regs: &mut [u64; 11], mem: &mut dyn EbpfMemory) -> Result<u64, EbpfTrap>`.
    #[allow(clippy::type_complexity)]
    syscall_handlers: HashMap<
        u32,
        Box<
            dyn FnMut(&mut [u64; 11], &mut dyn EbpfMemory) -> std::result::Result<u64, EbpfTrap>
                + Send,
        >,
    >,
}

impl fmt::Debug for EbpfCpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EbpfCpu")
            .field("regs", &self.regs)
            .field("config", &self.config)
            .field("call_frames", &self.call_frames)
            .field("insn_count", &self.insn_count)
            .finish()
    }
}

use std::fmt;

impl Default for EbpfCpu {
    fn default() -> Self {
        Self::new(EbpfConfig::default())
    }
}

impl EbpfCpu {
    /// Create a new eBPF / Solana SBPF CPU with the given configuration.
    pub fn new(config: EbpfConfig) -> Self {
        let mut regs = EbpfRegisters::default();
        let stack_size = if config
            .dialect
            .sbpf_version()
            .is_some_and(|v| v.manual_stack_frame_bump())
        {
            config
                .stack_frame_size
                .saturating_mul(config.max_call_depth as u64)
        } else {
            config.stack_frame_size
        };
        regs.r[FRAME_PTR_REG as usize] = MM_STACK_START.saturating_add(stack_size);
        Self {
            regs,
            config,
            call_frames: Vec::new(),
            insn_count: 0,
            function_registry: HashMap::new(),
            syscall_handlers: HashMap::new(),
        }
    }

    /// Create a new standard IETF RFC 9669 little-endian eBPF CPU.
    pub fn rfc9669() -> Self {
        Self::new(EbpfConfig::default())
    }

    /// Create a new Solana SBPF CPU for the given SBPF version.
    pub fn solana(version: SbpfVersion) -> Self {
        Self::new(EbpfConfig {
            dialect: EbpfDialect::Solana(version),
            endian: Endian::Little,
            ..EbpfConfig::default()
        })
    }

    /// Register an internal function entry point (byte PC) by 32-bit key.
    pub fn register_function(&mut self, key: u32, target_pc: u64) {
        self.function_registry.insert(key, target_pc);
    }

    /// Register a host syscall callback by 32-bit key.
    pub fn register_syscall<F>(&mut self, key: u32, handler: F)
    where
        F: FnMut(&mut [u64; 11], &mut dyn EbpfMemory) -> std::result::Result<u64, EbpfTrap>
            + Send
            + 'static,
    {
        self.syscall_handlers.insert(key, Box::new(handler));
    }

    fn validate_target_pc(
        &self,
        fault_pc: u64,
        target_pc: u64,
    ) -> std::result::Result<(), EbpfTrap> {
        if target_pc % (INSN_SIZE as u64) != 0 {
            return Err(EbpfTrap::CallOutsideTextSegment {
                pc: fault_pc,
                target_pc,
            });
        }
        if let Some((start, end)) = self.config.text_range
            && (target_pc < start || target_pc.saturating_add(INSN_SIZE as u64) > end)
        {
            return Err(EbpfTrap::CallOutsideTextSegment {
                pc: fault_pc,
                target_pc,
            });
        }
        Ok(())
    }

    fn push_call_frame(
        &mut self,
        fault_pc: u64,
        return_pc: u64,
    ) -> std::result::Result<(), EbpfTrap> {
        if self.call_frames.len() + 1 >= self.config.max_call_depth {
            return Err(EbpfTrap::CallDepthExceeded { pc: fault_pc });
        }
        let frame = EbpfCallFrame {
            caller_saved_regs: [
                self.regs.r[6],
                self.regs.r[7],
                self.regs.r[8],
                self.regs.r[9],
            ],
            frame_pointer: self.regs.r[FRAME_PTR_REG as usize],
            return_pc,
        };
        self.call_frames.push(frame);

        match self.config.dialect {
            EbpfDialect::Solana(ver) => {
                if !ver.manual_stack_frame_bump() {
                    let mult = if ver.stack_frame_gaps() && self.config.enable_stack_frame_gaps {
                        2
                    } else {
                        1
                    };
                    let step = self.config.stack_frame_size.wrapping_mul(mult);
                    self.regs.r[FRAME_PTR_REG as usize] =
                        self.regs.r[FRAME_PTR_REG as usize].wrapping_add(step);
                }
            }
            EbpfDialect::Rfc9669 => {
                self.regs.r[FRAME_PTR_REG as usize] =
                    self.regs.r[FRAME_PTR_REG as usize].wrapping_sub(self.config.stack_frame_size);
            }
        }
        Ok(())
    }

    /// Fetch and decode the instruction at `self.regs.pc`.
    pub fn fetch_and_decode(
        &self,
        mem: &mut dyn EbpfMemory,
    ) -> std::result::Result<EbpfInsn, EbpfTrap> {
        let pc = self.regs.pc;
        if let Some((start, end)) = self.config.text_range
            && (pc < start || pc.saturating_add(INSN_SIZE as u64) > end)
        {
            return Err(EbpfTrap::CallOutsideTextSegment { pc, target_pc: pc });
        }
        let mut buf = [0u8; 16];
        mem.read_bytes(pc, &mut buf[..8])
            .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
        if buf[0] == 0x18 {
            if let Some((_, end)) = self.config.text_range
                && pc.saturating_add(16) > end
            {
                return Err(EbpfTrap::DecodeError {
                    pc,
                    error: EbpfDecodeError::UnexpectedEof {
                        needed: 16,
                        available: 8,
                    },
                });
            }
            mem.read_bytes(pc.wrapping_add(8), &mut buf[8..16])
                .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
            decode_insn(&buf[..16], self.config.dialect, self.config.endian)
                .map_err(|error| EbpfTrap::DecodeError { pc, error })
        } else {
            decode_insn(&buf[..8], self.config.dialect, self.config.endian)
                .map_err(|error| EbpfTrap::DecodeError { pc, error })
        }
    }

    /// Execute a single decoded instruction against `self.regs` and `mem`.
    pub fn execute_decoded(
        &mut self,
        insn: &EbpfInsn,
        mem: &mut dyn EbpfMemory,
    ) -> std::result::Result<Option<EbpfExit>, EbpfTrap> {
        let pc = self.regs.pc;
        let mut next_pc = pc.wrapping_add(u64::from(insn.size));
        let endian = self.config.endian;

        match insn.op {
            EbpfOp::LoadImm64 {
                dst,
                src_type,
                imm64,
            } => {
                if self.config.dialect == EbpfDialect::Rfc9669 && src_type != 0 {
                    return Err(EbpfTrap::UnsupportedPseudoLoad { pc, src_type });
                }
                self.regs.r[dst as usize] = imm64;
            }

            EbpfOp::LoadReg {
                dst,
                base,
                off,
                width,
                sign_extend,
            } => {
                let addr = (self.regs.r[base as usize] as i64).wrapping_add(i64::from(off)) as u64;
                let val = mem
                    .load(addr, width, sign_extend, endian)
                    .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                self.regs.r[dst as usize] = val;
            }

            EbpfOp::StoreImm {
                base,
                off,
                imm,
                width,
            } => {
                let addr = (self.regs.r[base as usize] as i64).wrapping_add(i64::from(off)) as u64;
                let val = imm as i64 as u64;
                mem.store(addr, val, width, endian)
                    .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
            }

            EbpfOp::StoreReg {
                base,
                off,
                src,
                width,
            } => {
                let addr = (self.regs.r[base as usize] as i64).wrapping_add(i64::from(off)) as u64;
                let val = self.regs.r[src as usize];
                mem.store(addr, val, width, endian)
                    .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
            }

            EbpfOp::Atomic {
                base,
                off,
                src,
                width,
                kind,
            } => {
                let addr = (self.regs.r[base as usize] as i64).wrapping_add(i64::from(off)) as u64;
                let old = mem
                    .load(addr, width, false, endian)
                    .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                let mask = match width {
                    EbpfMemWidth::B4 => 0xffff_ffffu64,
                    _ => u64::MAX,
                };
                let old_m = old & mask;
                let src_m = self.regs.r[src as usize] & mask;
                match kind {
                    AtomicKind::Add { fetch } => {
                        let new_m = old_m.wrapping_add(src_m) & mask;
                        mem.store(addr, new_m, width, endian)
                            .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                        if fetch {
                            self.regs.r[src as usize] = old_m;
                        }
                    }
                    AtomicKind::Or { fetch } => {
                        let new_m = (old_m | src_m) & mask;
                        mem.store(addr, new_m, width, endian)
                            .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                        if fetch {
                            self.regs.r[src as usize] = old_m;
                        }
                    }
                    AtomicKind::And { fetch } => {
                        let new_m = (old_m & src_m) & mask;
                        mem.store(addr, new_m, width, endian)
                            .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                        if fetch {
                            self.regs.r[src as usize] = old_m;
                        }
                    }
                    AtomicKind::Xor { fetch } => {
                        let new_m = (old_m ^ src_m) & mask;
                        mem.store(addr, new_m, width, endian)
                            .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                        if fetch {
                            self.regs.r[src as usize] = old_m;
                        }
                    }
                    AtomicKind::Xchg => {
                        mem.store(addr, src_m, width, endian)
                            .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                        self.regs.r[src as usize] = old_m;
                    }
                    AtomicKind::Cmpxchg => {
                        let exp_m = self.regs.r[0] & mask;
                        if old_m == exp_m {
                            mem.store(addr, src_m, width, endian)
                                .map_err(|fault| EbpfTrap::MemoryFault { pc, fault })?;
                        }
                        self.regs.r[0] = old_m;
                    }
                }
            }

            EbpfOp::BinAlu {
                width,
                op,
                dst,
                src,
            } => {
                let d_val = self.regs.r[dst as usize];
                let (s_sext64, s_zext64) = match src {
                    EbpfSrc::Imm(imm) => (imm as i64 as u64, imm as u32 as u64),
                    EbpfSrc::Reg(r) => {
                        let v = self.regs.r[r as usize];
                        (v, v)
                    }
                };

                let res = match (width, op) {
                    (AluWidth::W32, BinAluOp::Add { sign_extend_32 }) => {
                        let r32 = (d_val as i32).wrapping_add(s_sext64 as i32);
                        if sign_extend_32 {
                            r32 as i64 as u64
                        } else {
                            r32 as u32 as u64
                        }
                    }
                    (AluWidth::W64, BinAluOp::Add { .. }) => d_val.wrapping_add(s_sext64),

                    (
                        AluWidth::W32,
                        BinAluOp::Sub {
                            sign_extend_32,
                            swap_operands,
                        },
                    ) => {
                        let r32 = if swap_operands {
                            (s_sext64 as i32).wrapping_sub(d_val as i32)
                        } else {
                            (d_val as i32).wrapping_sub(s_sext64 as i32)
                        };
                        if sign_extend_32 {
                            r32 as i64 as u64
                        } else {
                            r32 as u32 as u64
                        }
                    }
                    (AluWidth::W64, BinAluOp::Sub { swap_operands, .. }) => {
                        if swap_operands {
                            s_sext64.wrapping_sub(d_val)
                        } else {
                            d_val.wrapping_sub(s_sext64)
                        }
                    }

                    (AluWidth::W32, BinAluOp::Mul { sign_extend_32 }) => {
                        let r32 = (d_val as i32).wrapping_mul(s_sext64 as i32);
                        if sign_extend_32 {
                            r32 as i64 as u64
                        } else {
                            r32 as u32 as u64
                        }
                    }
                    (AluWidth::W64, BinAluOp::Mul { .. }) => d_val.wrapping_mul(s_sext64),

                    (_, BinAluOp::Uhmul64) => {
                        ((d_val as u128).wrapping_mul(s_zext64 as u128) >> 64) as u64
                    }
                    (_, BinAluOp::Shmul64) => {
                        ((d_val as i64 as i128).wrapping_mul(s_sext64 as i64 as i128) >> 64) as u64
                    }

                    (AluWidth::W32, BinAluOp::Udiv { trap_on_zero, .. }) => {
                        let a = d_val as u32;
                        let b = s_sext64 as u32;
                        if b == 0 {
                            if trap_on_zero {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            0
                        } else {
                            u64::from(a / b)
                        }
                    }
                    (
                        AluWidth::W64,
                        BinAluOp::Udiv {
                            trap_on_zero,
                            imm_zero_extended_64,
                        },
                    ) => {
                        let b = if imm_zero_extended_64 {
                            s_zext64
                        } else {
                            s_sext64
                        };
                        if b == 0 {
                            if trap_on_zero {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            0
                        } else {
                            d_val / b
                        }
                    }

                    (AluWidth::W32, BinAluOp::Urem { trap_on_zero, .. }) => {
                        let a = d_val as u32;
                        let b = s_sext64 as u32;
                        if b == 0 {
                            if trap_on_zero {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            u64::from(a)
                        } else {
                            u64::from(a % b)
                        }
                    }
                    (
                        AluWidth::W64,
                        BinAluOp::Urem {
                            trap_on_zero,
                            imm_zero_extended_64,
                        },
                    ) => {
                        let b = if imm_zero_extended_64 {
                            s_zext64
                        } else {
                            s_sext64
                        };
                        if b == 0 {
                            if trap_on_zero {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            d_val
                        } else {
                            d_val % b
                        }
                    }

                    (
                        AluWidth::W32,
                        BinAluOp::Sdiv {
                            trap_on_zero_or_overflow,
                        },
                    ) => {
                        let a = d_val as i32;
                        let b = s_sext64 as i32;
                        if b == 0 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            0
                        } else if a == i32::MIN && b == -1 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideOverflow { pc });
                            }
                            u64::from(i32::MIN as u32)
                        } else {
                            u64::from((a / b) as u32)
                        }
                    }
                    (
                        AluWidth::W64,
                        BinAluOp::Sdiv {
                            trap_on_zero_or_overflow,
                        },
                    ) => {
                        let a = d_val as i64;
                        let b = s_sext64 as i64;
                        if b == 0 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            0
                        } else if a == i64::MIN && b == -1 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideOverflow { pc });
                            }
                            i64::MIN as u64
                        } else {
                            (a / b) as u64
                        }
                    }

                    (
                        AluWidth::W32,
                        BinAluOp::Srem {
                            trap_on_zero_or_overflow,
                        },
                    ) => {
                        let a = d_val as i32;
                        let b = s_sext64 as i32;
                        if b == 0 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            u64::from(a as u32)
                        } else if a == i32::MIN && b == -1 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideOverflow { pc });
                            }
                            0
                        } else {
                            u64::from((a % b) as u32)
                        }
                    }
                    (
                        AluWidth::W64,
                        BinAluOp::Srem {
                            trap_on_zero_or_overflow,
                        },
                    ) => {
                        let a = d_val as i64;
                        let b = s_sext64 as i64;
                        if b == 0 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideByZero { pc });
                            }
                            a as u64
                        } else if a == i64::MIN && b == -1 {
                            if trap_on_zero_or_overflow {
                                return Err(EbpfTrap::DivideOverflow { pc });
                            }
                            0
                        } else {
                            (a % b) as u64
                        }
                    }

                    (AluWidth::W32, BinAluOp::Or) => u64::from((d_val as u32) | (s_sext64 as u32)),
                    (AluWidth::W64, BinAluOp::Or) => d_val | s_sext64,

                    (AluWidth::W32, BinAluOp::And) => u64::from((d_val as u32) & (s_sext64 as u32)),
                    (AluWidth::W64, BinAluOp::And) => d_val & s_sext64,

                    (AluWidth::W32, BinAluOp::Xor) => u64::from((d_val as u32) ^ (s_sext64 as u32)),
                    (AluWidth::W64, BinAluOp::Xor) => d_val ^ s_sext64,

                    (AluWidth::W32, BinAluOp::Lsh) => {
                        u64::from((d_val as u32).wrapping_shl(s_sext64 as u32))
                    }
                    (AluWidth::W64, BinAluOp::Lsh) => d_val.wrapping_shl(s_sext64 as u32),

                    (AluWidth::W32, BinAluOp::Rsh) => {
                        u64::from((d_val as u32).wrapping_shr(s_sext64 as u32))
                    }
                    (AluWidth::W64, BinAluOp::Rsh) => d_val.wrapping_shr(s_sext64 as u32),

                    (AluWidth::W32, BinAluOp::Arsh) => {
                        u64::from((d_val as i32).wrapping_shr(s_sext64 as u32) as u32)
                    }
                    (AluWidth::W64, BinAluOp::Arsh) => {
                        (d_val as i64).wrapping_shr(s_sext64 as u32) as u64
                    }

                    (
                        width,
                        BinAluOp::Mov {
                            sign_extend_from_bits,
                            sign_extend_to_64,
                        },
                    ) => match (width, sign_extend_from_bits, sign_extend_to_64) {
                        (AluWidth::W32, Some(8), false) => u64::from(s_sext64 as i8 as i32 as u32),
                        (AluWidth::W32, Some(16), false) => {
                            u64::from(s_sext64 as i16 as i32 as u32)
                        }
                        (_, Some(8), true) => s_sext64 as i8 as i64 as u64,
                        (_, Some(16), true) => s_sext64 as i16 as i64 as u64,
                        (_, Some(32), true) => s_sext64 as i32 as i64 as u64,
                        (AluWidth::W32, _, _) => u64::from(s_sext64 as u32),
                        (AluWidth::W64, _, _) => s_sext64,
                    },

                    (_, BinAluOp::Hor64Imm) => d_val | (s_zext64 << 32),
                };

                self.regs.r[dst as usize] = res;
            }

            EbpfOp::Neg { width, dst } => {
                let d_val = self.regs.r[dst as usize];
                self.regs.r[dst as usize] = match width {
                    AluWidth::W32 => u64::from((d_val as i32).wrapping_neg() as u32),
                    AluWidth::W64 => (d_val as i64).wrapping_neg() as u64,
                };
            }

            EbpfOp::Endian { dst, op } => {
                let d_val = self.regs.r[dst as usize];
                let swap = match (endian, op) {
                    (Endian::Little, EndianOp::ToLe(_)) | (Endian::Big, EndianOp::ToBe(_)) => false,
                    (Endian::Little, EndianOp::ToBe(_))
                    | (Endian::Big, EndianOp::ToLe(_))
                    | (_, EndianOp::Bswap(_)) => true,
                };
                let bits = match op {
                    EndianOp::ToLe(b) | EndianOp::ToBe(b) | EndianOp::Bswap(b) => b,
                };
                self.regs.r[dst as usize] = match (bits, swap) {
                    (16, false) => u64::from(d_val as u16),
                    (16, true) => u64::from((d_val as u16).swap_bytes()),
                    (32, false) => u64::from(d_val as u32),
                    (32, true) => u64::from((d_val as u32).swap_bytes()),
                    (_, false) => d_val,
                    (_, true) => d_val.swap_bytes(),
                };
            }

            EbpfOp::Jump { offset_insns } => {
                let delta_bytes = i64::from(offset_insns).wrapping_mul(INSN_SIZE as i64);
                let target_pc = (next_pc as i64).wrapping_add(delta_bytes) as u64;
                self.validate_target_pc(pc, target_pc)?;
                next_pc = target_pc;
            }

            EbpfOp::JumpCond {
                width,
                cond,
                dst,
                src,
                offset_insns,
            } => {
                let d_val = self.regs.r[dst as usize];
                let s_val = match src {
                    EbpfSrc::Imm(imm) => imm as i64 as u64,
                    EbpfSrc::Reg(r) => self.regs.r[r as usize],
                };
                let taken = match (width, cond) {
                    (AluWidth::W64, JmpCond::Jeq) => d_val == s_val,
                    (AluWidth::W64, JmpCond::Jgt) => d_val > s_val,
                    (AluWidth::W64, JmpCond::Jge) => d_val >= s_val,
                    (AluWidth::W64, JmpCond::Jset) => (d_val & s_val) != 0,
                    (AluWidth::W64, JmpCond::Jne) => d_val != s_val,
                    (AluWidth::W64, JmpCond::Jsgt) => (d_val as i64) > (s_val as i64),
                    (AluWidth::W64, JmpCond::Jsge) => (d_val as i64) >= (s_val as i64),
                    (AluWidth::W64, JmpCond::Jlt) => d_val < s_val,
                    (AluWidth::W64, JmpCond::Jle) => d_val <= s_val,
                    (AluWidth::W64, JmpCond::Jslt) => (d_val as i64) < (s_val as i64),
                    (AluWidth::W64, JmpCond::Jsle) => (d_val as i64) <= (s_val as i64),

                    (AluWidth::W32, JmpCond::Jeq) => (d_val as u32) == (s_val as u32),
                    (AluWidth::W32, JmpCond::Jgt) => (d_val as u32) > (s_val as u32),
                    (AluWidth::W32, JmpCond::Jge) => (d_val as u32) >= (s_val as u32),
                    (AluWidth::W32, JmpCond::Jset) => ((d_val as u32) & (s_val as u32)) != 0,
                    (AluWidth::W32, JmpCond::Jne) => (d_val as u32) != (s_val as u32),
                    (AluWidth::W32, JmpCond::Jsgt) => (d_val as i32) > (s_val as i32),
                    (AluWidth::W32, JmpCond::Jsge) => (d_val as i32) >= (s_val as i32),
                    (AluWidth::W32, JmpCond::Jlt) => (d_val as u32) < (s_val as u32),
                    (AluWidth::W32, JmpCond::Jle) => (d_val as u32) <= (s_val as u32),
                    (AluWidth::W32, JmpCond::Jslt) => (d_val as i32) < (s_val as i32),
                    (AluWidth::W32, JmpCond::Jsle) => (d_val as i32) <= (s_val as i32),
                };
                if taken {
                    let delta_bytes = i64::from(offset_insns).wrapping_mul(INSN_SIZE as i64);
                    let target_pc = (next_pc as i64).wrapping_add(delta_bytes) as u64;
                    self.validate_target_pc(pc, target_pc)?;
                    next_pc = target_pc;
                }
            }

            EbpfOp::CallImm { src_kind, imm } => {
                let key = imm as u32;
                let static_syscalls = self
                    .config
                    .dialect
                    .sbpf_version()
                    .is_none_or(|v| v.static_syscalls());

                if src_kind == 1 && static_syscalls {
                    // PC-relative local function call: target_pc = next_pc + imm * 8
                    let delta_bytes = i64::from(imm).wrapping_mul(INSN_SIZE as i64);
                    let target_pc = (next_pc as i64).wrapping_add(delta_bytes) as u64;
                    self.validate_target_pc(pc, target_pc)?;
                    self.push_call_frame(pc, next_pc)?;
                    next_pc = target_pc;
                } else if !static_syscalls
                    && let Some(&target_pc) = self.function_registry.get(&key)
                {
                    // Legacy Solana SBPF V0–V2 internal function call by hash key
                    self.validate_target_pc(pc, target_pc)?;
                    self.push_call_frame(pc, next_pc)?;
                    next_pc = target_pc;
                } else if let Some(handler) = self.syscall_handlers.get_mut(&key) {
                    let ret = handler(&mut self.regs.r, mem)?;
                    self.regs.r[0] = ret;
                } else {
                    self.regs.pc = next_pc;
                    return Ok(Some(EbpfExit::Syscall { key, next_pc }));
                }
            }

            EbpfOp::CallReg { target_reg } => {
                let target_pc = self.regs.r[target_reg as usize];
                self.validate_target_pc(pc, target_pc)?;
                self.push_call_frame(pc, next_pc)?;
                next_pc = target_pc;
            }

            EbpfOp::Exit => {
                if let Some(frame) = self.call_frames.pop() {
                    self.regs.r[6..10].copy_from_slice(&frame.caller_saved_regs);
                    self.regs.r[FRAME_PTR_REG as usize] = frame.frame_pointer;
                    next_pc = frame.return_pc;
                } else {
                    self.regs.pc = next_pc;
                    return Ok(Some(EbpfExit::Exit { r0: self.regs.r[0] }));
                }
            }
        }

        self.regs.pc = next_pc;
        Ok(None)
    }

    /// Fetch, decode, and execute a single instruction.
    pub fn step(
        &mut self,
        mem: &mut dyn EbpfMemory,
    ) -> std::result::Result<Option<EbpfExit>, EbpfTrap> {
        let pc = self.regs.pc;
        if let Some(max) = self.config.max_instructions
            && self.insn_count >= max
        {
            return Err(EbpfTrap::ExceededMaxInstructions { pc });
        }
        let insn = self.fetch_and_decode(mem)?;
        self.insn_count = self.insn_count.wrapping_add(1);
        match self.execute_decoded(&insn, mem) {
            Ok(exit) => Ok(exit),
            Err(trap) => Err(trap),
        }
    }

    /// Run until an [`EbpfExit`] (program exit, unhandled syscall, or trap) occurs.
    pub fn run(&mut self, mem: &mut dyn EbpfMemory) -> EbpfExit {
        loop {
            match self.step(mem) {
                Ok(Some(exit)) => return exit,
                Ok(None) => continue,
                Err(trap) => return EbpfExit::Trap(trap),
            }
        }
    }
}

/// Architecture-neutral [`VCpu`] adapter for [`EbpfCpu`].
pub struct EbpfVcpu {
    id: u32,
    cpu: EbpfCpu,
    mem: Arc<GuestMemoryMmap>,
    halted: bool,
}

const RUN_BATCH_INSNS: u64 = 100_000;

impl EbpfVcpu {
    /// Create a new eBPF / Solana SBPF vCPU backed by `mem`.
    pub fn new(id: u32, mem: Arc<GuestMemoryMmap>, config: EbpfConfig) -> Self {
        Self {
            id,
            cpu: EbpfCpu::new(config),
            mem,
            halted: false,
        }
    }

    /// Borrow the underlying [`EbpfCpu`].
    pub fn cpu(&self) -> &EbpfCpu {
        &self.cpu
    }

    /// Mutably borrow the underlying [`EbpfCpu`].
    pub fn cpu_mut(&mut self) -> &mut EbpfCpu {
        &mut self.cpu
    }

    /// Return this vCPU's numeric identifier.
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl VCpu for EbpfVcpu {
    fn id(&self) -> u32 {
        self.id
    }

    fn run(&mut self) -> Result<VcpuExit> {
        if self.halted {
            return Ok(VcpuExit::Hlt);
        }
        for _ in 0..RUN_BATCH_INSNS {
            if let Some(exit) = self.step_insn()? {
                return Ok(exit);
            }
        }
        Ok(VcpuExit::Hlt)
    }

    fn step_insn(&mut self) -> Result<Option<VcpuExit>> {
        if self.halted {
            return Ok(Some(VcpuExit::Hlt));
        }
        let mut mem_ref: &GuestMemoryMmap = &self.mem;
        match self.cpu.step(&mut mem_ref) {
            Ok(None) => Ok(None),
            Ok(Some(EbpfExit::Exit { .. })) => {
                self.halted = true;
                Ok(Some(VcpuExit::Shutdown))
            }
            Ok(Some(EbpfExit::Syscall { .. })) => Ok(Some(VcpuExit::SystemCall)),
            Ok(Some(EbpfExit::Trap(trap))) | Err(trap) => {
                self.halted = true;
                Err(Error::Emulator(format!("eBPF trap: {trap:?}")))
            }
        }
    }

    fn supports_stepping(&self) -> bool {
        true
    }

    fn current_pc(&self) -> u64 {
        self.cpu.regs.pc
    }

    fn wake(&mut self) {
        self.halted = false;
    }

    fn instruction_count(&self) -> u64 {
        self.cpu.insn_count
    }

    fn get_state(&self) -> Result<CpuState> {
        Ok(CpuState::ebpf(self.cpu.regs.clone()))
    }

    fn set_state(&mut self, state: &CpuState) -> Result<()> {
        let CpuState::Ebpf(ebpf_state) = state else {
            return Err(Error::Emulator(
                "expected ebpf state for ebpf vCPU".to_string(),
            ));
        };
        self.cpu.regs = ebpf_state.regs.clone();
        Ok(())
    }

    fn complete_io_in(&mut self, _data: &[u8]) {}
}
