//! Backend contract for publishing an already validated packet write set.

use super::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::types::GuestAddr;

#[derive(Clone, Debug)]
pub struct PacketWrite {
    pub addr: GuestAddr,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub enum PacketCommitError {
    Unsupported,
    Memory(MemoryError),
}

impl From<MemoryError> for PacketCommitError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

impl FlatMemory {
    pub(super) fn publish_packet(
        &mut self,
        writes: &[PacketWrite],
    ) -> Result<(), PacketCommitError> {
        // Validate every destination before the first publication. The backing
        // Vec and its base are stable throughout this exclusive borrow.
        for write in writes {
            let start = self.offset(write.addr)?;
            self.end(start, write.addr, write.data.len())?;
        }
        for write in writes {
            // These bounds were validated against the same immutable allocation.
            self.write(write.addr, &write.data)
                .expect("validated flat packet write");
        }
        Ok(())
    }
}
