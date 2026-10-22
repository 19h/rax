//! Current-process reads using guest mappings only. Native build29683 observations
//! and the pinned NT signature are retained under
//! `docs/specifications/windows/native-read-virtual-memory/`.

use super::query::probe_write;
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::{Mem, MemFault, prot};
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

const USER_LIMIT: u64 = 0x7FFF_FFFF_0000;
const COPY_BLOCK: usize = 65536;
const FAULT_BLOCK: usize = 14 * PAGE_SIZE as usize;

pub(super) fn read(c: &mut Ctx) -> ApiResult {
    // Capture every argument before a destination can alias the caller's stack.
    let (handle, source, output, length, returned) =
        (c.ptr(0)?, c.ptr(1)?, c.ptr(2)?, c.ptr(3)?, c.ptr(4)?);
    if length != 0
        && let Some(Object::Process { pid, .. }) = c.p.objects.get(handle)
        && *pid != c.p.pid
        && c.p.objects.access(handle).unwrap_or(0) & 0x10 != 0
    {
        // Do not consume caller guards for an operation outside the guest
        // process model, and never forward a guest address to a host read API.
        return Err(c.unsupported("NtReadVirtualMemory for another process"));
    }
    for address in [source, output] {
        if address
            .checked_add(length)
            .is_none_or(|end| address >= USER_LIMIT || end > USER_LIMIT)
        {
            return Flow::ret(STATUS_ACCESS_VIOLATION.into());
        }
    }
    let wow = c.arch() == WinArch::X86;
    let returned = if wow && returned != 0 {
        // WoW64 first reads the optional ULONG destination. An inaccessible
        // read disables publication; a readable, read-only slot instead fails
        // its later store, after the data copy has happened.
        let mut slot = [0u8; 4];
        let available = source_bytes(c, returned, &mut slot);
        if available == slot.len() {
            returned
        } else {
            c.p.vm.take_guard(returned + available as u64);
            0
        }
    } else {
        if returned != 0
            && let Err(fault) = probe_write(c, returned, c.psize() as usize)
        {
            return Flow::ret(store_fault(c, fault).into());
        }
        returned
    };
    let (status, copied) = if length == 0 {
        // Native zero-size reads do not reference the process handle.
        (STATUS_SUCCESS, 0)
    } else {
        let status = process_status(c, handle);
        if status == STATUS_SUCCESS {
            copy(c, source, output, length)
        } else {
            (status, 0)
        }
    };
    if returned != 0
        && let Err(fault) = c.write_ptr(returned, copied)
    {
        return Flow::ret(store_fault(c, fault).into());
    }
    Flow::ret(status.into())
}

fn store_fault(c: &mut Ctx, fault: MemFault) -> u32 {
    if c.p.vm.take_guard(fault.addr) {
        STATUS_GUARD_PAGE_VIOLATION
    } else {
        STATUS_ACCESS_VIOLATION
    }
}

fn process_status(c: &Ctx, handle: u64) -> u32 {
    if handle == c.arch().ptr(u64::MAX) {
        return STATUS_SUCCESS;
    }
    if handle == c.arch().ptr(u64::MAX - 1) {
        return STATUS_OBJECT_TYPE_MISMATCH;
    }
    match c.p.objects.get(handle) {
        None => STATUS_INVALID_HANDLE,
        Some(Object::Process { .. }) => {
            if c.p.objects.access(handle).unwrap_or(0) & 0x10 != 0 {
                STATUS_SUCCESS
            } else {
                STATUS_ACCESS_DENIED
            }
        }
        Some(_) => STATUS_OBJECT_TYPE_MISMATCH,
    }
}

/// Fill bounded scratch using actual committed source pages. The kernel can
/// read execute-only source pages, but does not consume source guards. This
/// permission bypass never reads outside the validated guest mapping.
fn source_bytes(c: &Ctx, source: u64, bytes: &mut [u8]) -> usize {
    let mut done = 0;
    while done < bytes.len() {
        let address = source + done as u64;
        let size = (PAGE_SIZE - address % PAGE_SIZE) as usize;
        let size = size.min(bytes.len() - done);
        if c.p.vm.page_state(address).is_none_or(|state| {
            !state.committed
                || state.protect & prot::GUARD != 0
                || matches!(state.protect & 0xFF, 0 | prot::NOACCESS)
        }) || c
            .mem()
            .read_raw(address, &mut bytes[done..done + size])
            .is_err()
        {
            break;
        }
        done += size;
    }
    done
}

/// O(n + P log R) work, at most 64 KiB scratch, with P visited pages and R
/// allocation/page-state runs. Each failing source block remains unpublished.
fn copy(c: &mut Ctx, source: u64, output: u64, length: u64) -> (u32, u64) {
    // A faulting span uses the observed fourteen-page transfer blocks. A
    // fully accessible span instead snapshots up to 64 KiB before publication,
    // which is observable when source and destination overlap at that boundary.
    // This metadata walk does not consume guards or populate resident frames.
    let block = if accessible(c, source, length, false) && accessible(c, output, length, true) {
        COPY_BLOCK
    } else {
        FAULT_BLOCK
    };
    let mut bytes = [0u8; COPY_BLOCK];
    let mut done = 0;
    while done < length {
        let size = (length - done).min(block as u64) as usize;
        let available = source_bytes(c, source + done, &mut bytes[..size]);
        let publish = if available == size { size } else { 0 };
        if publish != 0 {
            let destination = output + done;
            if let Err(fault) = probe_write(c, destination, publish) {
                c.p.vm.take_guard(fault.addr);
                let prefix = fault.addr.saturating_sub(destination).min(publish as u64);
                let reached = partial_stores(
                    c,
                    source + done,
                    destination,
                    &bytes[..publish],
                    prefix as usize,
                );
                return (STATUS_PARTIAL_COPY, done + reached as u64);
            }
            if c.mem().wr(destination, &bytes[..publish]).is_err() {
                return (STATUS_PARTIAL_COPY, done);
            }
            done += publish as u64;
        }
        if available != size {
            return (STATUS_PARTIAL_COPY, done);
        }
    }
    (STATUS_SUCCESS, done)
}

/// The selected native copy issues wider stores than a byte loop. Preserve
/// completed stores without inventing a byte prefix when the first store
/// straddles an inaccessible page. Native captures exercise these boundaries.
fn partial_stores(c: &Ctx, source: u64, destination: u64, bytes: &[u8], prefix: usize) -> usize {
    let size = bytes.len();
    let store = |at: usize, width: usize| -> Result<(), usize> {
        // A straddling store reports the page boundary; a nonsequential store
        // starting beyond that boundary reports its own address. The latter
        // can exceed both the written bytes and the accessible prefix.
        if at + width > prefix {
            return Err(at.max(prefix));
        }
        c.mem()
            .wr(destination + at as u64, &bytes[at..at + width])
            .map_err(|_| at)
    };
    let sequence = |first: usize, count: usize| {
        (first..first + count)
            .step_by(8)
            .try_for_each(|at| store(at, 8))
    };
    let result = (|| -> Result<(), usize> {
        match size {
            0 => {}
            3 => return Err(prefix), // Neither crossing three-byte case publishes a store.
            1..=2 => {
                store(0, 1)?;
                store(size / 2, 1)?;
                store(size - 1, 1)?;
            }
            4..=7 => {
                store(0, 4)?;
                store(size - 4, 4)?;
            }
            8..=64 => {
                sequence(0, size / 8 * 8)?;
                if size % 8 != 0 {
                    store(size - 8, 8)?;
                }
            }
            65..=128 => {
                sequence(0, 32)?;
                sequence(size - 32, 32)?;
                sequence(32, 32)?;
                sequence(size - 64, 32)?;
            }
            _ => {
                // In the measured short-copy profile the loop origin is
                // aligned independently of the caller's source. At 512 bytes
                // and above, the profile retains its source byte alignment
                // and starts with a sixteen-byte leading store.
                let retain_alignment = size >= 512;
                sequence(0, if retain_alignment { 16 } else { 32 })?;
                let mut at = if retain_alignment {
                    16 - (source % 16) as usize
                } else {
                    16
                };
                while at < size - 64 {
                    sequence(at, 64)?;
                    at += 64;
                }
                sequence(size - 64, 64)?;
            }
        }
        Ok(())
    })();
    result.err().unwrap_or(prefix)
}

fn accessible(c: &Ctx, address: u64, length: u64, write: bool) -> bool {
    let mut at = address;
    let end = address + length; // checked before either metadata walk
    while at < end {
        let Some(state) = c.p.vm.page_state(at) else {
            return false;
        };
        if !state.committed
            || state.protect & prot::GUARD != 0
            || if write {
                !matches!(
                    state.protect & 0xFF,
                    prot::READWRITE
                        | prot::WRITECOPY
                        | prot::EXECUTE_READWRITE
                        | prot::EXECUTE_WRITECOPY
                )
            } else {
                matches!(state.protect & 0xFF, 0 | prot::NOACCESS)
            }
        {
            return false;
        }
        at = (at | (PAGE_SIZE - 1)) + 1;
    }
    true
}
