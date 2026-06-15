//! Minimal bump allocator over the per-arch heap window.
//!
//! The heap bounds come from [`crate::arch::heap_range`] — the linker symbols on
//! bare metal, a static buffer in usermode. There is no `free`; the suite only
//! needs monotonic allocation, and keeping it trivial avoids depending on a
//! global allocator in `no_std`.

pub struct BumpAllocator {
    next: usize,
    end: usize,
    start: usize,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        BumpAllocator {
            next: 0,
            end: 0,
            start: 0,
        }
    }

    /// # Safety
    /// Must be called once before any `alloc`, with the heap window unused.
    pub unsafe fn init(&mut self) {
        let (start, end) = crate::arch::heap_range();
        self.start = start as usize;
        self.next = start as usize;
        self.end = end as usize;
    }

    pub fn alloc<T>(&mut self, count: usize) -> Option<*mut T> {
        let size = core::mem::size_of::<T>().checked_mul(count)?;
        let align = core::mem::align_of::<T>();
        let aligned = (self.next + align - 1) & !(align - 1);
        let new_next = aligned.checked_add(size)?;
        if new_next > self.end {
            return None;
        }
        self.next = new_next;
        Some(aligned as *mut T)
    }

    pub fn allocated_bytes(&self) -> usize {
        self.next - self.start
    }

    pub fn capacity(&self) -> usize {
        self.end - self.start
    }
}

static mut ALLOCATOR: BumpAllocator = BumpAllocator::new();

#[inline]
#[allow(static_mut_refs)]
pub fn allocator() -> &'static mut BumpAllocator {
    unsafe { &mut *core::ptr::addr_of_mut!(ALLOCATOR) }
}
