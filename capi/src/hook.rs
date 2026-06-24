//! Execution hooks: registration, storage, and the C callback types.
//!
//! Hooks are dispatched by the run loop (`run.rs`). Callbacks receive the
//! engine handle and may freely call back into the API (read/write registers
//! and memory, request a stop): the run loop never holds a Rust borrow of the
//! engine across a callback, so such re-entrancy is sound.

use std::os::raw::{c_int, c_void};

use crate::engine::{Engine, engine_mut};
use crate::guard;
use crate::status::RaxStatus;

// Hook-type bitmask values. Mirror `RAX_HOOK_*` in `rax.h`.
pub const RAX_HOOK_CODE: u32 = 1 << 0;
pub const RAX_HOOK_BLOCK: u32 = 1 << 1;
pub const RAX_HOOK_INTR: u32 = 1 << 2;
pub const RAX_HOOK_IO_IN: u32 = 1 << 3;
pub const RAX_HOOK_IO_OUT: u32 = 1 << 4;
pub const RAX_HOOK_MMIO_READ: u32 = 1 << 5;
pub const RAX_HOOK_MMIO_WRITE: u32 = 1 << 6;
pub const RAX_HOOK_INVALID: u32 = 1 << 7;

/// Per-instruction / per-block callback: `(engine, address, size, user)`.
/// `size` is 0 when the instruction length has not been decoded.
pub type CodeCb = extern "C" fn(*mut Engine, u64, u32, *mut c_void);
/// Interrupt/exception callback: `(engine, intno, user)`.
pub type IntrCb = extern "C" fn(*mut Engine, u32, *mut c_void);
/// Port-input callback: `(engine, port, size, user) -> value`.
pub type IoInCb = extern "C" fn(*mut Engine, u32, u32, *mut c_void) -> u64;
/// Port-output callback: `(engine, port, size, value, user)`.
pub type IoOutCb = extern "C" fn(*mut Engine, u32, u32, u64, *mut c_void);
/// MMIO read callback: `(engine, addr, size, user) -> value`.
pub type MmioReadCb = extern "C" fn(*mut Engine, u64, u32, *mut c_void) -> u64;
/// MMIO write callback: `(engine, addr, size, value, user)`.
pub type MmioWriteCb = extern "C" fn(*mut Engine, u64, u32, u64, *mut c_void);
/// Invalid-instruction/fault callback: `(engine, address, user) -> handled`.
/// Returning non-zero tells the engine the situation was handled and execution
/// may continue; zero stops the run.
pub type InvalidCb = extern "C" fn(*mut Engine, u64, *mut c_void) -> c_int;

#[derive(Clone, Copy)]
pub struct RangeHook<C: Copy> {
    pub id: u32,
    pub begin: u64,
    pub end: u64,
    pub cb: C,
    pub user: *mut c_void,
}

impl<C: Copy> RangeHook<C> {
    /// Whether `addr` is in this hook's range. `begin > end` means "any address".
    #[inline]
    pub fn matches(&self, addr: u64) -> bool {
        self.begin > self.end || (addr >= self.begin && addr <= self.end)
    }
}

#[derive(Clone, Copy)]
pub struct SimpleHook<C: Copy> {
    pub id: u32,
    pub cb: C,
    pub user: *mut c_void,
}

/// All hooks registered on an engine.
pub struct HookTable {
    next_id: u32,
    pub code: Vec<RangeHook<CodeCb>>,
    pub block: Vec<RangeHook<CodeCb>>,
    pub intr: Vec<SimpleHook<IntrCb>>,
    pub io_in: Vec<SimpleHook<IoInCb>>,
    pub io_out: Vec<SimpleHook<IoOutCb>>,
    pub mmio_read: Vec<SimpleHook<MmioReadCb>>,
    pub mmio_write: Vec<SimpleHook<MmioWriteCb>>,
    pub invalid: Vec<SimpleHook<InvalidCb>>,
}

impl HookTable {
    pub fn new() -> Self {
        HookTable {
            next_id: 1,
            code: Vec::new(),
            block: Vec::new(),
            intr: Vec::new(),
            io_in: Vec::new(),
            io_out: Vec::new(),
            mmio_read: Vec::new(),
            mmio_write: Vec::new(),
            invalid: Vec::new(),
        }
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn remove(&mut self, id: u32) -> bool {
        macro_rules! drop_from {
            ($v:expr) => {{
                let before = $v.len();
                $v.retain(|h| h.id != id);
                before != $v.len()
            }};
        }
        drop_from!(self.code)
            || drop_from!(self.block)
            || drop_from!(self.intr)
            || drop_from!(self.io_in)
            || drop_from!(self.io_out)
            || drop_from!(self.mmio_read)
            || drop_from!(self.mmio_write)
            || drop_from!(self.invalid)
    }
}

// ===========================================================================
// FFI: hook registration
// ===========================================================================

macro_rules! check_handle {
    ($engine:expr) => {
        match unsafe { engine_mut($engine) } {
            Some(e) => e,
            None => return RaxStatus::Handle,
        }
    };
}

fn finish_id(out_id: *mut u32, id: u32) -> RaxStatus {
    if !out_id.is_null() {
        unsafe {
            *out_id = id;
        }
    }
    RaxStatus::Ok
}

/// Adds a per-instruction code hook for addresses in `[begin, end]`
/// (`begin > end` ⇒ all addresses). Requires a stepping-capable backend.
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_code(
    engine: *mut Engine,
    begin: u64,
    end: u64,
    cb: Option<CodeCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        if !e.vcpu.supports_stepping() {
            return e.fail(RaxStatus::Unsupported, "code hooks require a stepping-capable backend");
        }
        let id = e.hooks.alloc_id();
        e.hooks.code.push(RangeHook { id, begin, end, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds a basic-block hook (fires on entry to a block within `[begin, end]`).
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_block(
    engine: *mut Engine,
    begin: u64,
    end: u64,
    cb: Option<CodeCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        if !e.vcpu.supports_stepping() {
            return e.fail(RaxStatus::Unsupported, "block hooks require a stepping-capable backend");
        }
        let id = e.hooks.alloc_id();
        e.hooks.block.push(RangeHook { id, begin, end, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds an interrupt/exception hook.
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_intr(
    engine: *mut Engine,
    cb: Option<IntrCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        let id = e.hooks.alloc_id();
        e.hooks.intr.push(SimpleHook { id, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds a port-input hook (services `IN`/`INS`).
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_io_in(
    engine: *mut Engine,
    cb: Option<IoInCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        let id = e.hooks.alloc_id();
        e.hooks.io_in.push(SimpleHook { id, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds a port-output hook (services `OUT`/`OUTS`).
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_io_out(
    engine: *mut Engine,
    cb: Option<IoOutCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        let id = e.hooks.alloc_id();
        e.hooks.io_out.push(SimpleHook { id, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds an MMIO-read hook.
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_mmio_read(
    engine: *mut Engine,
    cb: Option<MmioReadCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        let id = e.hooks.alloc_id();
        e.hooks.mmio_read.push(SimpleHook { id, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds an MMIO-write hook.
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_mmio_write(
    engine: *mut Engine,
    cb: Option<MmioWriteCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        let id = e.hooks.alloc_id();
        e.hooks.mmio_write.push(SimpleHook { id, cb, user });
        finish_id(out_id, id)
    })
}

/// Adds an invalid-instruction/fault hook.
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_add_invalid(
    engine: *mut Engine,
    cb: Option<InvalidCb>,
    user: *mut c_void,
    out_id: *mut u32,
) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        let cb = match cb {
            Some(c) => c,
            None => return e.fail(RaxStatus::Arg, "null callback"),
        };
        let id = e.hooks.alloc_id();
        e.hooks.invalid.push(SimpleHook { id, cb, user });
        finish_id(out_id, id)
    })
}

/// Removes a previously added hook by id.
#[unsafe(no_mangle)]
pub extern "C" fn rax_hook_del(engine: *mut Engine, hook_id: u32) -> RaxStatus {
    guard(|| {
        let e = check_handle!(engine);
        if e.hooks.remove(hook_id) {
            RaxStatus::Ok
        } else {
            e.fail(RaxStatus::Hook, "no such hook id")
        }
    })
}
