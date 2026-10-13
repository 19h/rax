//! A user-mode vCPU and a matching SMIR context that start every sweep case
//! from the same architectural state.
//!
//! User mode runs the guest at CPL 3, so privileged opcodes raise #GP instead
//! of changing control state, unmapped accesses report their exact linear
//! address, and exceptions come back as vectors. Every GPR except RSP holds a
//! distinct unmapped address, so an implicit access through a register is
//! never mistaken for the RIP-relative operand.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use super::super::{FpuState, LazyFlags, X86_64Vcpu};
use crate::error::{Error, GuestMemoryFault, MemoryAccessKind, MemoryFaultKind};
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::CallTarget;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::types::BlockId;
use crate::smir::ir::{SmirBlock, Terminator, TrapKind};
use crate::smir::lift::{ControlFlow, LiftResult};
use crate::vm::memory::FlatTranslation;
use crate::vm::vcpu::{Registers, SystemRegisters, VcpuExit};

/// The code page; the instruction under test starts at its first byte.
pub(super) const CODE: u64 = 0x40_0000;
/// The stack page (RSP starts inside it).
pub(super) const STACK: u64 = 0x7fff_f000;
/// Where every RIP-relative operand points: page aligned, unmapped unless a
/// case maps it explicitly.
pub(super) const TARGET: u64 = 0x4000_0000;

const R: u8 = 1;
const W: u8 = 2;
const X: u8 = 4;

/// Linear pages -> guest frames 0x0000 (code), 0x1000 (stack), 0x2000
/// (target, only while `target_mapped`).
#[derive(Default)]
struct Space {
    target_mapped: AtomicBool,
}

impl FlatTranslation for Space {
    fn translate(&self, linear: u64, access: MemoryAccessKind) -> Result<u64, GuestMemoryFault> {
        let (frame, perms) = match linear & !0xFFF {
            CODE => (0x0000, R | X),
            STACK => (0x1000, R | W),
            TARGET if self.target_mapped.load(Ordering::Relaxed) => (0x2000, R | W),
            _ => return Err(GuestMemoryFault::unmapped(linear, 1, access)),
        };
        let need = match access {
            MemoryAccessKind::Read => R,
            MemoryAccessKind::Write => W,
            MemoryAccessKind::Fetch => X,
        };
        if perms & need == 0 {
            return Err(GuestMemoryFault {
                address: linear,
                size: 1,
                access,
                kind: MemoryFaultKind::Permission,
            });
        }
        Ok(frame | (linear & 0xFFF))
    }
}

/// How one interpreter step ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Interp {
    /// An access to an unmapped linear address.
    Fault(u64),
    /// An exception or software interrupt with this vector.
    Event(u8),
    /// The instruction retired; RIP afterwards.
    Retired(u64),
    /// Any other exit or error, described.
    Other(String),
}

/// How the lifted instruction ended in the SMIR interpreter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Smir {
    /// An access to an address outside the stack page.
    Fault(u64),
    /// Control transfers through memory at this address (evaluated
    /// statically: a block terminator is not executed).
    BranchThrough(u64),
    /// Completed without leaving the stack page.
    NoAccess,
    /// Any other exit, described.
    Other(String),
}

struct Baseline {
    regs: Registers,
    sregs: SystemRegisters,
    fpu: FpuState,
    lazy_flags: LazyFlags,
    mxcsr: u32,
    xcr0: u64,
    pkru: u32,
}

pub(super) struct Machine {
    vcpu: X86_64Vcpu,
    mem: Arc<GuestMemoryMmap>,
    space: Arc<Space>,
    baseline: Baseline,
}

/// The poison value of GPR `index`: canonical, unmapped, and distinct.
fn poison(index: usize) -> u64 {
    0x5000_0000 + 0x1_0000 * index as u64
}

/// The value every case starts with in GPR `index`.
pub(super) fn initial_gpr(index: u8) -> u64 {
    if index == 4 {
        STACK + 0xF00
    } else {
        poison(usize::from(index))
    }
}

impl Machine {
    pub(super) fn new() -> Self {
        let mem =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x3000)]).unwrap());
        let space = Arc::new(Space::default());
        let mut vcpu = X86_64Vcpu::new(0, mem.clone());
        vcpu.set_xeon_phi_avx512_enabled(true);
        vcpu.set_vp2intersect_enabled(true);
        vcpu.set_sse4a_enabled(true);
        vcpu.set_tbm_enabled(true);
        vcpu.set_xop_enabled(true);
        vcpu.set_avx10_media_enabled(true);
        vcpu.set_avx10_vminmax_enabled(true);
        vcpu.set_avx10_sat_convert_enabled(true);
        vcpu.set_apx_enabled(true);
        vcpu.enable_user_mode(space.clone());
        // x87, SSE, AVX, the AVX-512 state, and APX_F (bit 19).
        vcpu.set_xcr0(0xE7 | (1 << 19)).unwrap();
        for index in 0..32 {
            vcpu.set_reg(index, initial_gpr(index), 8);
        }
        vcpu.regs.rip = CODE;
        let baseline = Baseline {
            regs: vcpu.regs.clone(),
            sregs: vcpu.sregs.clone(),
            fpu: vcpu.fpu.clone(),
            lazy_flags: vcpu.lazy_flags,
            mxcsr: vcpu.mxcsr,
            xcr0: vcpu.xcr0,
            pkru: vcpu.pkru,
        };
        Machine {
            vcpu,
            mem,
            space,
            baseline,
        }
    }

    fn restore(&mut self, code: &[u8], target_mapped: bool) {
        let b = &self.baseline;
        self.vcpu.regs = b.regs.clone();
        self.vcpu.sregs = b.sregs.clone();
        self.vcpu.fpu = b.fpu.clone();
        self.vcpu.lazy_flags = b.lazy_flags;
        self.vcpu.mxcsr = b.mxcsr;
        self.vcpu.xcr0 = b.xcr0;
        self.vcpu.pkru = b.pkru;
        self.vcpu.halted = false;
        self.vcpu.interrupt_inhibit = false;
        self.vcpu.io_pending = None;
        self.vcpu.take_user_trap();
        self.space
            .target_mapped
            .store(target_mapped, Ordering::Relaxed);
        if target_mapped {
            self.mem
                .write_slice(&[0; 0x1000], GuestAddress(0x2000))
                .unwrap();
        }
        self.mem.write_slice(code, GuestAddress(0)).unwrap();
    }

    fn outcome(&mut self, result: crate::error::Result<Option<VcpuExit>>) -> Interp {
        match result {
            Ok(None) => Interp::Retired(self.vcpu.regs.rip),
            Ok(Some(exit)) => Interp::Other(format!("{exit:?}")),
            Err(Error::GuestAccess(fault)) => Interp::Fault(fault.address),
            Err(Error::GuestEvent { vector }) => Interp::Event(vector),
            Err(error) => Interp::Other(error.to_string()),
        }
    }

    /// One instruction of `code` at `CODE`, with #GP and #PF delivered as
    /// an embedder sees them (`step_with_faults`).
    pub(super) fn step(&mut self, code: &[u8], target_mapped: bool) -> Interp {
        self.restore(code, target_mapped);
        let result = self.vcpu.step_with_faults();
        self.outcome(result)
    }

    /// `code` at `CODE` through the threaded interpreter, until it stops.
    pub(super) fn threaded(&mut self, code: &[u8]) -> Interp {
        self.restore(code, false);
        let result = self.vcpu.run_threaded().map(Some);
        self.outcome(result)
    }

    /// The GPR `index` after the last step.
    pub(super) fn gpr(&self, index: u8) -> u64 {
        self.vcpu.get_reg(index, 8)
    }

    fn smir_context(&self) -> SmirContext {
        let mut ctx = SmirContext::new_x86_64();
        ctx.pc = CODE;
        ctx.flags.materialized = MaterializedFlags::from_rflags(self.baseline.regs.rflags);
        ctx.flags.lazy = None;
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!()
        };
        for index in 0..32 {
            x86.gpr[index] = initial_gpr(index as u8);
        }
        let sregs = &self.baseline.sregs;
        x86.rip = CODE;
        x86.rflags = self.baseline.regs.rflags;
        x86.cr0 = sregs.cr0;
        x86.cr4 = sregs.cr4;
        x86.efer = sregs.efer;
        x86.cs_l = true;
        x86.cs_selector = sregs.cs.selector;
        x86.ss_selector = sregs.ss.selector;
        x86.cpl = 3;
        x86.xcr0 = self.baseline.xcr0;
        x86.mxcsr = self.baseline.mxcsr;
        x86.pkru = self.baseline.pkru;
        x86.xeon_phi_avx512 = true;
        x86.vp2intersect = true;
        x86.sse4a = true;
        x86.tbm = true;
        x86.xop = true;
        x86.apx_enabled = true;
        ctx
    }

    /// Runs the lifted instruction with only the stack page present.
    pub(super) fn smir(&self, lifted: &LiftResult) -> Smir {
        let ctx = self.smir_context();
        let through = match &lifted.control_flow {
            ControlFlow::IndirectBranchMem { addr }
            | ControlFlow::Call {
                target: CallTarget::IndirectMem(addr),
            } => Some((addr, false)),
            ControlFlow::Call {
                target: CallTarget::X86IndirectMemAddr32(addr),
            } => Some((addr, true)),
            _ => None,
        };
        if let Some((addr, addr32)) = through {
            let at = SmirInterpreter::new().compute_address(&ctx, addr);
            return Smir::BranchThrough(if addr32 { at & 0xFFFF_FFFF } else { at });
        }
        let mut ctx = ctx;
        let mut block = SmirBlock::new(BlockId(0), CODE);
        block.ops = lifted.ops.clone();
        block.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut memory = FlatMemory::with_base(STACK, 0x1000);
        match SmirInterpreter::new().execute_block(&mut ctx, &mut memory, &block) {
            BlockResult::Exit(ExitReason::MemoryFault { addr, .. }) => Smir::Fault(addr),
            BlockResult::Exit(ExitReason::Halt) | BlockResult::Continue(_) => Smir::NoAccess,
            BlockResult::Exit(other) => Smir::Other(format!("{other:?}")),
        }
    }
}

/// Is `address` inside the region a poisoned GPR points into, i.e. an
/// implicit access through a register rather than the RIP-relative operand?
pub(super) fn is_poison(address: u64) -> bool {
    (poison(0)..poison(32)).contains(&address)
}
