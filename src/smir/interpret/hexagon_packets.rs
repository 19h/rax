//! Explicit Hexagon packet boundaries, rollback and backend write publication.

use super::*;
use crate::smir::ir::context::VRegFile;
use crate::smir::ir::flags::FlagState;
use crate::smir::ir::memory::{ExclusiveMonitor, PacketCommitError, PacketWrite};

struct PacketSnapshot {
    registers: ArchRegState,
    temporaries: VRegFile,
    flags: FlagState,
    reservation: ExclusiveMonitor,
    pc: GuestAddr,
    instructions: u64,
    cycles: u64,
}

impl PacketSnapshot {
    fn capture(context: &SmirContext) -> Self {
        Self {
            registers: context.arch_regs.clone(),
            temporaries: context.vregs.clone(),
            flags: context.flags.clone(),
            reservation: context.exclusive_monitor.clone(),
            pc: context.pc,
            instructions: context.insn_count,
            cycles: context.cycle_count,
        }
    }

    fn restore(self, context: &mut SmirContext) {
        context.arch_regs = self.registers;
        context.vregs = self.temporaries;
        context.flags = self.flags;
        context.exclusive_monitor = self.reservation;
        context.pc = self.pc;
        context.insn_count = self.instructions;
        context.cycle_count = self.cycles;
        context.exit_reason = None;
    }
}

struct PacketMemory<'a> {
    inner: &'a mut dyn SmirMemory,
    active: bool,
    writes: Vec<PacketWrite>,
    fences: Vec<FenceKind>,
}

impl PacketMemory<'_> {
    fn commit(&mut self) -> Result<(), PacketCommitError> {
        self.inner.write_packet(&self.writes)?;
        for fence in self.fences.drain(..) {
            self.inner.fence(fence);
        }
        self.writes.clear();
        self.active = false;
        Ok(())
    }
}

impl SmirMemory for PacketMemory<'_> {
    fn read(&mut self, addr: GuestAddr, data: &mut [u8]) -> Result<(), MemoryError> {
        self.inner.read(addr, data)?;
        for write in &self.writes {
            for (index, byte) in data.iter_mut().enumerate() {
                let location = u128::from(addr) + index as u128;
                if let Some(offset) = location.checked_sub(u128::from(write.addr))
                    && let Ok(offset) = usize::try_from(offset)
                    && let Some(value) = write.data.get(offset)
                {
                    *byte = *value;
                }
            }
        }
        Ok(())
    }

    fn write(&mut self, addr: GuestAddr, data: &[u8]) -> Result<(), MemoryError> {
        if self.active {
            self.inner.probe_write(addr, data.len())?;
            self.writes.push(PacketWrite {
                addr,
                data: data.to_vec(),
            });
            Ok(())
        } else {
            self.inner.write(addr, data)
        }
    }

    fn probe(&self, addr: GuestAddr, size: usize, write: bool) -> Result<(), MemoryError> {
        self.inner.probe(addr, size, write)
    }

    // These operations are rejected before dispatch. Hexagon LL/SC uses the
    // rollback-capable context reservation and ordinary staged reads/writes.
    fn atomic_load(
        &mut self,
        _: GuestAddr,
        _: MemWidth,
        _: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }
    fn atomic_store(
        &mut self,
        _: GuestAddr,
        _: u64,
        _: MemWidth,
        _: MemoryOrder,
    ) -> Result<(), MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }
    fn compare_and_swap(
        &mut self,
        _: GuestAddr,
        _: u64,
        _: u64,
        _: MemWidth,
        _: MemoryOrder,
        _: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }
    fn atomic_rmw(
        &mut self,
        _: GuestAddr,
        _: AtomicOp,
        _: u64,
        _: MemWidth,
        _: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }
    fn load_exclusive(&mut self, _: GuestAddr, _: MemWidth) -> Result<u64, MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }
    fn store_exclusive(&mut self, _: GuestAddr, _: u64, _: MemWidth) -> Result<bool, MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }
    fn clear_exclusive(&mut self) {}
    fn fence(&mut self, fence: FenceKind) {
        if self.active {
            self.fences.push(fence);
        } else {
            self.inner.fence(fence);
        }
    }
}

fn memory_exit(error: MemoryError, write: bool, pc: GuestAddr) -> ExitReason {
    let addr = match error {
        MemoryError::PageFault { addr, .. }
        | MemoryError::AccessViolation { addr, .. }
        | MemoryError::Alignment { addr, .. }
        | MemoryError::Mmio { addr, .. }
        | MemoryError::OutOfBounds { addr } => addr,
        MemoryError::ExclusiveFailed => pc,
    };
    let write = match error {
        MemoryError::PageFault { write, .. } | MemoryError::AccessViolation { write, .. } => write,
        _ => write,
    };
    ExitReason::MemoryFault { addr, write }
}

impl SmirInterpreter {
    pub(super) fn execute_hexagon_packets(
        &self,
        context: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        block: &SmirBlock,
        limit: u64,
    ) -> BlockResult {
        // Reject incomplete, nested or unmatched packets before executing any
        // operation. Unmarked instrumentation may surround complete packets.
        let mut active = None;
        for op in &block.ops {
            match op.kind {
                OpKind::HexagonPacketBegin if active.is_none() => active = Some(op.guest_pc),
                OpKind::HexagonPacketCommit if active.is_some() => active = None,
                OpKind::HexagonPacketBegin | OpKind::HexagonPacketCommit => {
                    return BlockResult::Exit(ExitReason::Unsupported {
                        addr: op.guest_pc,
                        operation: "malformed Hexagon packet boundaries",
                    });
                }
                _ => {}
            }
        }
        if let Some(addr) = active {
            return BlockResult::Exit(ExitReason::Unsupported {
                addr,
                operation: "incomplete Hexagon packet",
            });
        }
        let mut memory = PacketMemory {
            inner: memory,
            active: false,
            writes: Vec::new(),
            fences: Vec::new(),
        };
        let mut snapshot: Option<PacketSnapshot> = None;
        let mut retired = 0;
        for op in &block.ops {
            if let OpKind::HexagonPacketBegin = op.kind {
                context.pc = op.guest_pc;
                context.arch_regs.set_pc(op.guest_pc);
                if context.debug.has_breakpoint(op.guest_pc) {
                    return BlockResult::Exit(ExitReason::Breakpoint { addr: op.guest_pc });
                }
                if context.insn_count >= limit {
                    return BlockResult::Exit(ExitReason::InsnLimit);
                }
                if context.debug.single_step && retired != 0 {
                    return BlockResult::Exit(ExitReason::SingleStep);
                }
                snapshot = Some(PacketSnapshot::capture(context));
                memory.active = true;
                continue;
            }
            if let OpKind::HexagonPacketCommit = op.kind {
                let clears_reservation = memory.writes.iter().any(|write| {
                    context
                        .exclusive_monitor
                        .should_clear_for_store(write.addr, write.data.len())
                });
                if let Err(error) = memory.commit() {
                    let initial = snapshot.take().unwrap();
                    let pc = initial.pc;
                    initial.restore(context);
                    return BlockResult::Exit(match error {
                        PacketCommitError::Unsupported => ExitReason::Unsupported {
                            addr: pc,
                            operation: "backend cannot publish a Hexagon packet atomically",
                        },
                        PacketCommitError::Memory(error) => memory_exit(error, true, pc),
                    });
                }
                snapshot = None;
                if clears_reservation {
                    context.exclusive_monitor.clear();
                }
                context.pc = u64::from((op.guest_pc as u32).wrapping_add(4));
                context.arch_regs.set_pc(context.pc);
                context.insn_count = context.insn_count.saturating_add(1);
                retired += 1;
                continue;
            }
            if snapshot.is_some()
                && matches!(
                    op.kind,
                    OpKind::AtomicLoad { .. }
                        | OpKind::AtomicStore { .. }
                        | OpKind::AtomicRmw { .. }
                        | OpKind::Cas { .. }
                        | OpKind::CasPair { .. }
                        | OpKind::AtomicCmpXadd { .. }
                )
            {
                let initial = snapshot.take().unwrap();
                let pc = initial.pc;
                initial.restore(context);
                return BlockResult::Exit(ExitReason::Unsupported {
                    addr: pc,
                    operation: "non-Hexagon atomic operation inside a Hexagon packet",
                });
            }
            if let Err(error) = self.execute_op(context, &mut memory, op) {
                let pc = if let Some(initial) = snapshot.take() {
                    let pc = initial.pc;
                    initial.restore(context);
                    pc
                } else {
                    op.guest_pc
                };
                return BlockResult::Exit(memory_exit(error, op.kind.writes_memory(), pc));
            }
            if let Some(mut reason) = context.exit_reason.take() {
                if let Some(initial) = snapshot.take() {
                    if let ExitReason::AlignmentFault { pc, .. } = &mut reason {
                        *pc = initial.pc;
                    }
                    initial.restore(context);
                }
                return BlockResult::Exit(reason);
            }
        }
        let result = self.execute_terminator(context, memory.inner, &block.terminator);
        if let BlockResult::Continue(addr) = result {
            let addr = u64::from(addr as u32);
            context.pc = addr;
            context.arch_regs.set_pc(addr);
            BlockResult::Continue(addr)
        } else {
            result
        }
    }
}
