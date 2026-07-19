//! AArch64 memory and instruction-synchronization lowering.

use crate::smir::ir::types::FenceKind;
use crate::smir::lower::{LowerError, aarch64::Aarch64Lowerer};

impl Aarch64Lowerer {
    pub(crate) fn lower_fence(&mut self, kind: FenceKind) -> Result<(), LowerError> {
        match kind {
            FenceKind::ISync => self.emit(0xD503_3FDF), // ISB SY
            FenceKind::DSync | FenceKind::Full => self.emit(0xD503_3F9F), // DSB SY
            FenceKind::LoadLoad
            | FenceKind::LoadStore
            | FenceKind::StoreLoad
            | FenceKind::StoreStore => self.emit(0xD503_3FBF), // DMB SY
            FenceKind::InstructionSerialize => {
                // DSB SY completes prior memory accesses; ISB then flushes the
                // pipeline so subsequent fetch observes the completed state.
                self.emit(0xD503_3F9F);
                self.emit(0xD503_3FDF);
            }
        }
        Ok(())
    }
}
