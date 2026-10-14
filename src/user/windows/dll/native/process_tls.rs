//! Installed build29683's compact TLS replacement packet and WoW64 capture.
//!
//! Native controls are recorded under `docs/specifications/windows/native-process-tls`.
//! All addresses and thread changes stay in the emulated process.

use super::query::probe_write;
use crate::error::MemoryAccessKind;
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::layout;
use crate::user::windows::memory::{Mem, MemFault};
use crate::user::windows::nt::status::*;

const HEADER_BYTES: u64 = 16;

enum Packet {
    Native { address: u64 },
    Wow { address: u64, bytes: Vec<u8> },
}

impl Packet {
    fn read32(&self, c: &Ctx, offset: usize) -> Result<u32, ApiErr> {
        match self {
            Self::Native { address } => Ok(c.mem().u32(address + offset as u64)?),
            Self::Wow { bytes, .. } => Ok(u32::from_le_bytes(
                bytes[offset..offset + 4].try_into().unwrap(),
            )),
        }
    }

    fn pointer(&self, c: &Ctx, offset: usize) -> Result<u64, ApiErr> {
        match self {
            Self::Native { address } => Ok(c.read_ptr(address + offset as u64)?),
            Self::Wow { .. } => Ok(u64::from(self.read32(c, offset)?)),
        }
    }

    fn write32(&mut self, c: &Ctx, offset: usize, value: u32) -> Result<(), ApiErr> {
        match self {
            Self::Native { address } => c.mem().w32(*address + offset as u64, value)?,
            Self::Wow { bytes, .. } => {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        Ok(())
    }

    fn write_pointer(&mut self, c: &Ctx, offset: usize, value: u64) -> Result<(), ApiErr> {
        match self {
            Self::Native { address } => c.write_ptr(*address + offset as u64, value)?,
            Self::Wow { .. } => self.write32(c, offset, value as u32)?,
        }
        Ok(())
    }

    fn publish_wow(&self, c: &Ctx) -> Result<(), ApiErr> {
        if let Self::Wow { address, bytes } = self {
            c.mem().wr(*address, bytes)?;
        }
        Ok(())
    }
}

fn allocation(bytes: usize) -> Result<Vec<u8>, u32> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(bytes)
        .map_err(|_| STATUS_NO_MEMORY)?;
    result.resize(bytes, 0);
    Ok(result)
}

fn probe_read(c: &Ctx, address: u64, bytes: usize) -> Result<(), MemFault> {
    c.mem()
        .probe(address, bytes, MemoryAccessKind::Read)
        .map_err(|fault| MemFault {
            addr: fault.address,
            write: false,
        })
}

pub(super) fn information(c: &mut Ctx, process: u64, input: u64, length: u32) -> ApiResult {
    let wow = c.arch() == WinArch::X86;
    if !wow {
        // The caller alignment precedes process validation; a zero length
        // bypasses alignment, but still validates the current-process token.
        if length != 0 && input % 8 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        if process != c.arch().ptr(u64::MAX) {
            return Flow::ret(STATUS_INVALID_PARAMETER.into());
        }
        // The kernel first requires a structurally possible compact packet;
        // header ThreadDataCount is checked only after the caller probe.
        if length < 40 || (length - 16) % 24 != 0 {
            return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
        }
        if input
            .checked_add(u64::from(length))
            .is_none_or(|end| end > c.p.vm.high())
        {
            return Flow::ret(STATUS_ACCESS_VIOLATION.into());
        }
        probe_write(c, input, length as usize)?;
    }

    let flags = c.mem().u32(input)?;
    let operation = c.mem().u32(input + 4)?;
    let count = c.mem().u32(input + 8)?;
    let index = c.mem().u32(input + 12)?;
    let stride = c.psize() * 3;
    let bytes = HEADER_BYTES + u64::from(count) * stride;

    let mut packet = if wow {
        if operation > 1 {
            return Flow::ret(STATUS_INVALID_PARAMETER.into());
        }
        // WoW64 allocates a native-width temporary packet before reading its
        // entries. Use the personality's existing no-paging-file budget,
        // without mapping a host scratch address or allocating unbounded data.
        let capture = HEADER_BYTES + u64::from(count) * 24;
        let charge = (capture + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if charge > c.p.vm.commit_limit() - c.p.vm.committed_bytes()
            || charge > c.p.vm.high() - c.p.vm.low()
        {
            return Flow::ret(STATUS_NO_MEMORY.into());
        }
        probe_read(c, input, bytes as usize)?;
        let mut data = match allocation(bytes as usize) {
            Ok(data) => data,
            Err(status) => return Flow::ret(status.into()),
        };
        c.mem().rd(input, &mut data)?;
        Packet::Wow {
            address: input,
            bytes: data,
        }
    } else {
        Packet::Native { address: input }
    };

    if process != c.arch().ptr(u64::MAX) {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if count == 0 || operation > 1 || flags > 1 || (!wow && u64::from(length) != bytes) {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    if !wow && flags != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    for ordinal in 0..count {
        let offset = (HEADER_BYTES + u64::from(ordinal) * stride) as usize;
        if packet.read32(c, offset)? != 0 {
            return Flow::ret(STATUS_INVALID_PARAMETER.into());
        }
    }

    let status = replace(c, &mut packet, operation, count, index)?;
    if status == STATUS_SUCCESS {
        // Native errors retain completed native caller stores. WoW64 instead
        // leaves its original packet intact on a kernel failure; a later copy
        // fault retains the TLS state already changed in the kernel phase.
        packet.publish_wow(c)?;
    }
    Flow::ret(status.into())
}

/// O(N + T*log(T) + K*P) time and O(T + P) temporary bytes for N packet entries,
/// T process threads, K selected threads and P copied prefix bytes. WoW64
/// additionally retains O(N) converted caller bytes until successful copyback.
fn replace(
    c: &mut Ctx,
    packet: &mut Packet,
    operation: u32,
    count: u32,
    index: u32,
) -> Result<u32, ApiErr> {
    // Guest thread identifiers increase with creation. Include the extracted
    // scheduler owner in the same creation-order enumeration as its peers.
    let mut threads: Vec<_> = c.p.threads.values().map(|t| (t.tid, t.teb)).collect();
    threads.push((c.t.tid, c.t.teb));
    threads.sort_unstable_by_key(|(tid, _)| *tid);
    threads.dedup_by_key(|(tid, _)| *tid);
    let ptr = c.psize();
    let mut ordinal = 0u32;
    for (tid, teb) in threads {
        if ordinal == count {
            break;
        }
        let tls_pointer = teb + layout::offsets(c.arch()).teb_tls_pointer;
        let old = c.read_ptr(tls_pointer)?;
        if old == 0 {
            continue;
        }
        let entry = (HEADER_BYTES + u64::from(ordinal) * ptr * 3) as usize;
        let next = packet.pointer(c, entry + ptr as usize)?;
        if operation == 0 {
            let Some(slot) = old.checked_add(u64::from(index) * ptr) else {
                return Ok(STATUS_ACCESS_VIOLATION);
            };
            let previous = c.read_ptr(slot)?;
            probe_write(c, slot, ptr as usize)?;
            packet.write32(c, entry, 2)?;
            c.write_ptr(slot, next)?;
            packet.write_pointer(c, entry + ptr as usize, previous)?;
        } else {
            let previous = if old == tls_pointer { 0 } else { old };
            let copy_bytes = if previous == 0 {
                0
            } else {
                u64::from(index) * ptr
            };
            if copy_bytes != 0 && next % ptr != 0 {
                return Ok(STATUS_DATATYPE_MISALIGNMENT);
            }
            if copy_bytes != 0 {
                probe_read(c, previous, copy_bytes as usize)?;
                probe_write(c, next, copy_bytes as usize)?;
            }
            let mut prefix = match allocation(copy_bytes as usize) {
                Ok(data) => data,
                Err(status) => return Ok(status),
            };
            c.mem().rd(previous, &mut prefix)?;
            packet.write32(c, entry, 2)?;
            c.mem().wr(next, &prefix)?;
            c.write_ptr(tls_pointer, next)?;
            packet.write_pointer(c, entry + ptr as usize, previous)?;
            packet.write_pointer(c, entry + ptr as usize * 2, tid.into())?;
        }
        // Index replacement can also alias the TEB's pointer itself. Keep
        // introspection and subsequent loader operations coherent with memory.
        let published = c.read_ptr(tls_pointer)?;
        if tid == c.t.tid {
            c.t.tls_array = published;
        } else if let Some(thread) = c.p.threads.get_mut(&tid) {
            thread.tls_array = published;
        }
        ordinal += 1;
    }
    Ok(STATUS_SUCCESS)
}
