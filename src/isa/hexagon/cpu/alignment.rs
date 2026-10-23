//! V68 section 5.1.3 alignment, before execution or MMIO routing.

use super::*;
use crate::error::MemoryAccessKind;
use crate::isa::hexagon::memory_access::{ScalarMemoryAccess, scalar_memory_access, width_bytes};

impl HexagonVcpu {
    pub(super) fn execute_aligned_insn(
        &mut self,
        insn: DecodedInsn,
        packet_pc: u32,
        immext: Option<u32>,
        new_r: &mut [Option<u32>; 32],
        new_p: &mut [Option<u8>; 4],
        branch: &mut Option<BranchTarget>,
    ) -> Result<Option<VcpuExit>> {
        let mut operand = scalar_memory_access(&insn);
        // A successful store-conditional necessarily uses the reservation's
        // address. A failed reservation performs no memory access in this CPU.
        if let DecodedInsn::StoreCond {
            base,
            width,
            success_pred: Some(_),
            ..
        } = insn
        {
            if self.lock_addr == Some(self.regs.r[base as usize]) {
                operand = Some(ScalarMemoryAccess {
                    address: AddrMode::Offset { base, offset: 0 },
                    width,
                    access: MemoryAccessKind::Write,
                    predicate: None,
                });
            }
        }
        if let Some(operand) = operand {
            let cancelled = operand
                .predicate
                .is_some_and(|predicate| !self.eval_pred(predicate, new_p));
            if !cancelled {
                let (address, _) = self.resolve_addr(operand.address);
                let alignment = width_bytes(operand.width);
                if address % alignment != 0 {
                    self.set_pc(packet_pc);
                    return Err(Error::AlignmentFault {
                        pc: packet_pc as u64,
                        address: address as u64,
                        alignment,
                        access: operand.access,
                    });
                }
            }
        }
        self.execute_insn(insn, packet_pc, immext, new_r, new_p, branch)
    }
}
