//! This module defines memory regions

use crate::{
    aligned_memory::Pod,
    ebpf,
    error::{EbpfError, ProgramResult, StableResult},
    program::SBPFVersion,
    vm::Config,
};
use std::fmt::Formatter;
use std::{array, cell::UnsafeCell, fmt, mem, ops::Range, ptr};

/* Explanation of the Gapped Memory

    The MemoryMapping supports a special mapping mode which is used for the stack MemoryRegion.
    In this mode the backing address space of the host is sliced in power-of-two aligned frames.
    The exponent of this alignment is specified in vm_gap_shift. Then the virtual address space
    of the guest is spread out in a way which leaves gaps, the same size as the frames, in
    between the frames. This effectively doubles the size of the guests virtual address space.
    But the actual mapped memory stays the same, as the gaps are not mapped and accessing them
    results in an AccessViolation.

    Guest: frame 0 | gap 0 | frame 1 | gap 1 | frame 2 | gap 2 | ...
              |                /                 /
              |          *----*    *------------*
              |         /         /
    Host:  frame 0 | frame 1 | frame 2 | ...
*/

/// Callback executed before generate_access_violation()
pub type AccessViolationHandler = Box<dyn Fn(&mut MemoryRegion, u64, AccessType, u64, u64)>;
/// Fail always
#[allow(clippy::result_unit_err)]
pub fn default_access_violation_handler(
    _region: &mut MemoryRegion,
    _region_max_len: u64,
    _access_type: AccessType,
    _vm_addr: u64,
    _len: u64,
) {
}

/// Types that can be used as backing memory for memory mappings.
///
/// ## Safety
///
/// The implementers must ensure that the returned address and byte length are contained by the
/// implementing type's objects.
pub unsafe trait HostMemoryObject {
    /// The provenance-exposed address in the host address space for this object.
    ///
    /// Normally this is just an address of the pointer.
    fn host(self) -> HostBuffer;
}

/// Types that can be directly mapped into VM memory space should implement this trait.
///
/// The blanket implementations of [`HostMemoryObject`] for `*const T` where `T: VmExposable` allow
/// exposing the implementing types to the guests directly.
pub trait VmExposable {}

/// Types that can be directly and mutably mapped into VM memory space.
///
/// The blanket implementations of [`HostMemoryObject`] for `*mut T` where `T: VmExposableMut` allow
/// mutably exposing the implementing types to the guests directly.
///
/// ## Safety
///
/// The type must not have any invariants that could be broken by the guest's modifications to the
/// data within objects of this type.
pub unsafe trait VmExposableMut {}

unsafe impl VmExposableMut for u8 {}
impl<T: VmExposableMut> VmExposable for T {}

unsafe impl<T: VmExposable> HostMemoryObject for *const T {
    fn host(self) -> HostBuffer {
        HostBuffer::Immutable(ptr::slice_from_raw_parts(
            self.cast(),
            std::mem::size_of::<T>(),
        ))
    }
}

unsafe impl<T: VmExposableMut> HostMemoryObject for *mut T {
    fn host(self) -> HostBuffer {
        HostBuffer::Mutable(ptr::slice_from_raw_parts_mut(
            self.cast(),
            std::mem::size_of::<T>(),
        ))
    }
}

unsafe impl<T: VmExposable> HostMemoryObject for *const [T] {
    fn host(self) -> HostBuffer {
        HostBuffer::Immutable(ptr::slice_from_raw_parts(
            self.cast(),
            self.len().checked_mul(core::mem::size_of::<T>()).unwrap(),
        ))
    }
}

unsafe impl<T: VmExposableMut> HostMemoryObject for *mut [T] {
    fn host(self) -> HostBuffer {
        HostBuffer::Mutable(ptr::slice_from_raw_parts_mut(
            self.cast(),
            self.len().checked_mul(core::mem::size_of::<T>()).unwrap(),
        ))
    }
}

unsafe impl<T: VmExposable, const N: usize> HostMemoryObject for *const [T; N] {
    fn host(self) -> HostBuffer {
        HostBuffer::Immutable(ptr::slice_from_raw_parts(
            self.cast(),
            N.checked_mul(core::mem::size_of::<T>()).unwrap(),
        ))
    }
}

unsafe impl<T: VmExposableMut, const N: usize> HostMemoryObject for *mut [T; N] {
    fn host(self) -> HostBuffer {
        HostBuffer::Mutable(ptr::slice_from_raw_parts_mut(
            self.cast(),
            N.checked_mul(core::mem::size_of::<T>()).unwrap(),
        ))
    }
}

/// Either mutable or immutable slice, returned by [`MemoryRegion::host_buffer`].
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum HostBuffer {
    /// The `MemoryRegion` is read-only.
    Immutable(*const [u8]),
    /// The `MemoryRegion` is writable (`AccessType::Store` is permitted.)
    Mutable(*mut [u8]),
}

impl HostBuffer {
    /// The length of this host buffer.
    pub fn len(&self) -> usize {
        match self {
            HostBuffer::Immutable(p) => p.len(),
            HostBuffer::Mutable(p) => p.len(),
        }
    }

    /// `true` if this host buffer has a length of 0.
    pub fn is_empty(&self) -> bool {
        match self {
            HostBuffer::Immutable(p) => p.is_empty(),
            HostBuffer::Mutable(p) => p.is_empty(),
        }
    }

    /// `true` if this is a `HostBuffer::Mutable`.
    pub fn is_mutable(&self) -> bool {
        matches!(self, HostBuffer::Mutable(_))
    }

    /// Make this host buffer mutable.
    ///
    /// # Safety
    ///
    /// This host buffer *must* have been initially constructed with a mutable pointer.
    pub unsafe fn mutable(self) -> Self {
        match self {
            HostBuffer::Immutable(p) => HostBuffer::Mutable(p.cast_mut()),
            HostBuffer::Mutable(_) => self,
        }
    }

    /// Make this host buffer immutable.
    pub fn immutable(self) -> Self {
        match self {
            HostBuffer::Immutable(_) => self,
            HostBuffer::Mutable(p) => Self::Immutable(p.cast_const()),
        }
    }

    /// Subslice this host buffer with the provided range.
    #[inline]
    pub fn get(self, range: std::ops::Range<usize>) -> Option<Self> {
        if range.end > self.len() {
            return None;
        }
        let new_len = range.len();
        unsafe {
            // SAFETY:
            //
            // Contract from `ptr::add`: If the computed offset is non-zero, then self
            // must be derived from a pointer to some allocation, and the entire memory
            // range between self and the result must be in bounds of that allocation.
            // In particular, this range must not "wrap around" the edge of the address
            // space.
            //
            // Evidence: We take care to ensure to only get here if the `begin_offset`
            // (and a stronger condition: `begin_offset + len`) would be within `len()`
            // of the base pointer.
            //
            // Contract from `ptr::add`: The offset in bytes, count * size_of::<T>(),
            // computed on mathematical integers (without "wrapping around"), must fit
            // in an isize.
            //
            // Evidence: Allocation size in Rust, including those of the region backing
            // memory here specifically, may not exceed `isize::MAX` bytes.
            Some(match self {
                HostBuffer::Immutable(p) => HostBuffer::Immutable(ptr::slice_from_raw_parts(
                    p.byte_add(range.start).cast(),
                    new_len,
                )),
                HostBuffer::Mutable(p) => HostBuffer::Mutable(ptr::slice_from_raw_parts_mut(
                    p.byte_add(range.start).cast(),
                    new_len,
                )),
            })
        }
    }

    /// Pointer to a slice of the host memory.
    #[inline(always)]
    pub fn ptr(self) -> *const [u8] {
        match self {
            HostBuffer::Immutable(p) => p,
            HostBuffer::Mutable(p) => p,
        }
    }

    /// Mutble pointer to a slice of the host memory.
    #[inline(always)]
    pub fn ptr_mut(self) -> *mut [u8] {
        match self {
            HostBuffer::Immutable(p) => {
                debug_assert!(false, "ptr_mut, but buffer is immutable");
                p.cast_mut()
            }
            HostBuffer::Mutable(p) => p,
        }
    }
}

unsafe impl HostMemoryObject for HostBuffer {
    fn host(self) -> HostBuffer {
        self
    }
}

/// Memory region for bounds checking and address translation
#[derive(Eq, PartialEq, Clone)]
pub struct MemoryRegion {
    host: HostBuffer,
    /// start virtual address
    vm_addr: u64,
    /// Size of regular gaps as bit shift (63 means this region is continuous)
    vm_gap_shift: u8,
    /// User defined payload for the [AccessViolationHandler]
    pub access_violation_handler_payload: Option<u16>,
}

impl MemoryRegion {
    /// Create a VM memory region with host `address` pointing to a `len` bytes of data.
    ///
    /// This region will be made available in the guest at `vm_addr`.
    fn new_internal(host: HostBuffer, vm_addr: u64, vm_gap_size: u64) -> Self {
        let mut vm_gap_shift = (std::mem::size_of::<u64>() as u8)
            .saturating_mul(8)
            .saturating_sub(1);
        if vm_gap_size > 0 {
            vm_gap_shift = vm_gap_shift.saturating_sub(vm_gap_size.leading_zeros() as u8);
            debug_assert_eq!(Some(vm_gap_size), 1_u64.checked_shl(vm_gap_shift as u32));
        };
        MemoryRegion {
            host,
            vm_addr,
            vm_gap_shift,
            access_violation_handler_payload: None,
        }
    }

    /// Creates a new, empty `MemoryRegion`.
    ///
    /// This does not require to provide any backing host memory.
    pub fn new_empty(vm_addr: u64) -> Self {
        const EMPTY: &[u8] = &[];
        Self::new_internal((&raw const *EMPTY).host(), vm_addr, 0)
    }

    /// Creates a new `MemoryRegion` backed by the provided host memory.
    ///
    /// The backing memory must remain allocated for the duration of the returned `MemoryRegion`.
    pub fn new<HO: HostMemoryObject>(host: HO, vm_addr: u64) -> Self {
        Self::new_internal(host.host(), vm_addr, 0)
    }

    /// Creates a new gapped `MemoryRegion` backed by the provided host memory.
    ///
    /// The backing memory must remain allocated for the duration of the returned `MemoryRegion`.
    pub fn new_gapped<HO: HostMemoryObject>(host: HO, vm_addr: u64, vm_gap_size: u64) -> Self {
        Self::new_internal(host.host(), vm_addr, vm_gap_size)
    }

    /// Redirect this memory region to a different location in host memory.
    ///
    /// Depending on whether `HO` is mutable, the writability of the region is adjusted as well.
    ///
    /// # Safety
    ///
    /// If this `MemoryRegion` is a part of a [`MemoryMapping`] then, after redirection, this region
    /// must adhere to all the same contracts as the `MemoryRegion`s used for
    /// [`MemoryMapping::replace_region`].
    pub unsafe fn redirect<HO: HostMemoryObject>(&mut self, host: HO) {
        self.host = host.host();
    }

    /// Ensure that this memory region is immutable.
    pub fn make_immutable(&mut self) {
        unsafe {
            // SAFETY:
            // Contract from `MemoryRegion::redirect`: memory region must be live for
            // the duration of the mapping.
            //
            // Evidence: Since we aren't changing where the host buffer is pointing at,
            // the condition must already have been satisfied at the time this function was called
            // and thus remains satisfied.
            //
            // Contract from `MemoryRegion::redirect`: For `MemoryRegions` marked writable...
            //
            // Evidence: Memory region is no longer writable.
            self.redirect(self.host_buffer().immutable());
        }
    }

    /// Returns the vm address space covered by this MemoryRegion
    pub fn vm_addr_range(&self) -> Range<u64> {
        let bytes = self.len() as u64;
        if self.vm_gap_shift == 63 {
            self.vm_addr..self.vm_addr.saturating_add(bytes)
        } else {
            self.vm_addr..self.vm_addr.saturating_add(bytes.saturating_mul(2))
        }
    }

    /// Return the raw slice to the host memory that this memory region points at.
    ///
    /// This can be used to construct a new memory region.
    pub fn host_buffer(&self) -> HostBuffer {
        self.host
    }

    /// Length of this memory region in bytes.
    pub fn len(&self) -> usize {
        self.host.len()
    }

    /// Is the length of this memory region 0 bytes?
    pub fn is_empty(&self) -> bool {
        self.host.is_empty()
    }

    /// Return the `gap_size` with which the memory region has been constructed.
    pub fn gap_size(&self) -> u64 {
        if self.vm_gap_shift == 63 {
            0
        } else {
            1 << self.vm_gap_shift
        }
    }

    /// Convert a virtual machine address into a host slice.
    ///
    /// The returned slice will have exactly `len` bytes. If the provided `vm_addr` does not
    /// correlate to a valid subslice of this region, a `None` will be returned.
    #[inline]
    pub(crate) fn vm_to_host_buffer(&self, vm_addr: u64, len: u64) -> Option<HostBuffer> {
        // This can happen if a region starts at an offset from the base region
        // address, eg with rodata regions if config.optimize_rodata = true, see
        // Elf::get_ro_region.
        if vm_addr < self.vm_addr {
            return None;
        }

        let begin_offset = vm_addr.saturating_sub(self.vm_addr);
        if self.vm_gap_shift == 63 {
            // fast path for non-gapped regions
            if let Some(end_offset) = begin_offset.checked_add(len) {
                return self.host.get(begin_offset as usize..end_offset as usize);
            }
            return None;
        }

        let is_in_gap = (begin_offset
            .checked_shr(self.vm_gap_shift as u32)
            .unwrap_or(0)
            & 1)
            == 1;
        let gap_mask = (-1i64).checked_shl(self.vm_gap_shift as u32).unwrap_or(0) as u64;
        let gapped_offset =
            (begin_offset & gap_mask).checked_shr(1).unwrap_or(0) | (begin_offset & !gap_mask);
        if let Some(end_offset) = gapped_offset.checked_add(len) {
            if !is_in_gap {
                return self.host.get(gapped_offset as usize..end_offset as usize);
            }
        }
        None
    }
}

impl fmt::Debug for MemoryRegion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let vm_addr = self.vm_addr_range();
        let (host_addr, len, writable) = match self.host {
            HostBuffer::Immutable(p) => (p.addr() as u64, p.len() as u64, false),
            HostBuffer::Mutable(p) => (p.addr() as u64, p.len() as u64, true),
        };
        write!(
            f,
            "host_addr: {:#x?}-{:#x?}, vm_addr: {:#x?}-{:#x?}, len: {}, writable: {}, payload {:?}",
            host_addr,
            host_addr.saturating_add(len),
            vm_addr.start,
            vm_addr.end,
            len,
            writable,
            self.access_violation_handler_payload,
        )
    }
}

impl std::cmp::PartialOrd for MemoryRegion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::cmp::Ord for MemoryRegion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.vm_addr.cmp(&other.vm_addr)
    }
}

/// Type of memory access
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccessType {
    /// Read
    Load,
    /// Write
    Store,
}

impl std::fmt::Display for AccessType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Self::Load => "reading",
            Self::Store => "writing",
        })
    }
}

/// Memory mapping based on eytzinger search.
pub struct UnalignedMemoryMapping {
    /// Common parts
    regions: Box<[MemoryRegion]>,
    /// Regions vm_addr fields in Eytzinger order
    region_addresses: Box<[u64]>,
    /// Converts the Eytzinger order back to the original order
    region_index_lookup: Box<[usize]>,
    /// Cache of the last `MappingCache::SIZE` vm_addr => region_index lookups
    cache: UnsafeCell<MappingCache>,
}

impl fmt::Debug for UnalignedMemoryMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnalignedMemoryMapping")
            .field("regions", &self.regions)
            .field("cache", &self.cache)
            .finish()
    }
}

impl UnalignedMemoryMapping {
    fn construct_eytzinger_order(&mut self, mut in_index: usize, out_index: usize) -> usize {
        if out_index >= self.regions.len() {
            return in_index;
        }
        in_index =
            self.construct_eytzinger_order(in_index, out_index.saturating_mul(2).saturating_add(1));
        self.region_addresses[out_index] = self.regions[in_index].vm_addr;
        self.region_index_lookup[out_index] = in_index;
        self.construct_eytzinger_order(
            in_index.saturating_add(1),
            out_index.saturating_mul(2).saturating_add(2),
        )
    }

    /// Create an uninitialized UnalignedMapping
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    pub unsafe fn new_uninitialized(regions: Vec<MemoryRegion>) -> Self {
        let number_of_regions = regions.len();
        Self {
            regions: regions.into_boxed_slice(),
            region_addresses: vec![0; number_of_regions].into_boxed_slice(),
            region_index_lookup: vec![0; number_of_regions].into_boxed_slice(),
            cache: UnsafeCell::new(MappingCache::new()),
        }
    }

    /// Creates a new MemoryMapping structure from the given regions
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    pub unsafe fn new(regions: Vec<MemoryRegion>) -> Result<Self, EbpfError> {
        let mut mapping = Self::new_uninitialized(regions);
        mapping.initialize()?;
        Ok(mapping)
    }

    /// Initialize the memory mapping
    pub fn initialize(&mut self) -> Result<(), EbpfError> {
        self.regions.sort();
        let number_of_regions = self.regions.len();
        for index in 1..number_of_regions {
            let first = &self.regions[index.saturating_sub(1)];
            let second = &self.regions[index];
            if first.vm_addr_range().end > second.vm_addr {
                return Err(EbpfError::InvalidMemoryRegion(index));
            }
        }

        self.construct_eytzinger_order(0, 0);
        Ok(())
    }

    /// Returns the `MemoryRegion` which may contain the given address.
    #[allow(clippy::arithmetic_side_effects)]
    #[inline(always)]
    pub fn find_region(&self, vm_addr: u64) -> Option<(usize, &MemoryRegion)> {
        // Safety:
        // &mut references to the mapping cache are only created internally from methods that do not
        // invoke each other. UnalignedMemoryMapping is !Sync, so the cache reference below is
        // guaranteed to be unique.
        let cache = unsafe { &mut *self.cache.get() };
        if let Some(index) = cache.find(vm_addr) {
            // Safety:
            // Cached index, we validated it before caching it. See the corresponding safety section
            // in the miss branch.
            Some((index, unsafe { self.regions.get_unchecked(index) }))
        } else {
            let mut index = 1;
            while index <= self.region_addresses.len() {
                // Safety:
                // we start the search at index=1 and in the loop condition check
                // for index <= len, so bound checks can be avoided
                index = (index << 1)
                    + unsafe { *self.region_addresses.get_unchecked(index - 1) <= vm_addr }
                        as usize;
            }
            index >>= index.trailing_zeros() + 1;
            if index == 0 {
                return None;
            }
            // Safety:
            // we check for index==0 above, and by construction if we get here index
            // must be contained in region
            index = unsafe { *self.region_index_lookup.get_unchecked(index - 1) };
            let region = unsafe { self.regions.get_unchecked(index) };
            cache.insert(region.vm_addr_range(), index);
            Some((index, region))
        }
    }

    /// Replaces the `MemoryRegion` at the given index
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    #[inline(always)]
    pub unsafe fn replace_region(
        &mut self,
        index: usize,
        region: MemoryRegion,
    ) -> Result<(), EbpfError> {
        self.regions[index] = region;
        self.cache.get_mut().flush();
        Ok(())
    }
}

/// Memory mapping that uses the upper half of an address to identify the
/// underlying memory region.
#[derive(Debug)]
pub struct AlignedMemoryMapping {
    regions: Vec<MemoryRegion>,
}

impl AlignedMemoryMapping {
    /// Creates a new initialized MemoryMapping structure from the given regions
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    pub unsafe fn new(regions: Vec<MemoryRegion>) -> Result<Self, EbpfError> {
        let mut mapping = Self::new_uninitialized(regions);
        mapping.initialize()?;
        Ok(mapping)
    }

    /// Create an uninitialized MemoryMapping
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    pub unsafe fn new_uninitialized(regions: Vec<MemoryRegion>) -> Self {
        Self { regions }
    }

    /// Initialize the memory mapping by sorting its regions and filling gaps
    pub fn initialize(&mut self) -> Result<(), EbpfError> {
        static EMPTY_SLICE: &[u8] = &[];
        self.regions.sort();
        let mut expected_region_index = 0;
        while expected_region_index < self.regions.len() {
            let actual_region_index = self
                .regions
                .get(expected_region_index)
                .unwrap()
                .vm_addr
                .checked_shr(ebpf::VIRTUAL_ADDRESS_BITS as u32)
                .unwrap_or(0) as usize;
            if actual_region_index > expected_region_index {
                self.regions.insert(
                    expected_region_index,
                    MemoryRegion::new(
                        &raw const *EMPTY_SLICE,
                        (expected_region_index as u64).saturating_mul(ebpf::MM_REGION_SIZE),
                    ),
                );
            } else if actual_region_index < expected_region_index {
                return Err(EbpfError::InvalidMemoryRegion(actual_region_index));
            }
            expected_region_index = expected_region_index.saturating_add(1);
        }

        Ok(())
    }

    /// Returns the `MemoryRegion` which may contain the given address.
    #[inline(always)]
    pub fn find_region(&self, vm_addr: u64) -> Option<(usize, &MemoryRegion)> {
        let index = vm_addr.wrapping_shr(ebpf::VIRTUAL_ADDRESS_BITS as u32) as usize;
        if index < self.regions.len() {
            // Safety: bounds check above
            let region = unsafe { self.regions.get_unchecked(index) };
            return Some((index, region));
        }
        None
    }

    /// Replaces the `MemoryRegion` at the given index
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    #[inline(always)]
    pub unsafe fn replace_region(
        &mut self,
        index: usize,
        region: MemoryRegion,
    ) -> Result<(), EbpfError> {
        let begin_index = region
            .vm_addr
            .checked_shr(ebpf::VIRTUAL_ADDRESS_BITS as u32)
            .unwrap_or(0) as usize;
        let end_index = region
            .vm_addr
            .saturating_add((region.len() as u64).saturating_sub(1))
            .checked_shr(ebpf::VIRTUAL_ADDRESS_BITS as u32)
            .unwrap_or(0) as usize;
        if begin_index != index || end_index != index {
            return Err(EbpfError::InvalidMemoryRegion(index));
        }
        self.regions[index] = region;
        Ok(())
    }
}

/// Common parts of [UnalignedMemoryMapping] and [AlignedMemoryMapping]
pub struct MemoryMapping {
    /// Access violation handler
    access_violation_handler: AccessViolationHandler,
    max_call_depth: i64,
    stack_frame_size: i64,
    disable_address_translation: bool,
    /// Executable sbpf_version
    sbpf_version: SBPFVersion,
    initialized: bool,
    ty: MemoryMappingType,
}

impl fmt::Debug for MemoryMapping {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryMapping")
            .field("max_call_depth", &self.max_call_depth)
            .field("stack_frame_size", &self.stack_frame_size)
            .field("sbpf_version", &self.sbpf_version)
            .field("ty", &self.ty)
            .finish()
    }
}

/// Maps virtual memory to host memory.
#[derive(Debug)]
pub enum MemoryMappingType {
    /// Aligned memory mapping which uses the upper half of an address to
    /// identify the underlying memory region.
    Aligned(AlignedMemoryMapping),
    /// Memory mapping that allows mapping unaligned memory regions.
    Unaligned(UnalignedMemoryMapping),
}

impl MemoryMapping {
    /// Creates a new memory mapping.
    ///
    /// Uses aligned or unaligned memory mapping depending on the value of
    /// `config.aligned_memory_mapping=true`.
    ///
    /// # Safety
    ///
    /// In addition to the requirements of [`MemoryMapping::new_uninitialized`], the provided
    /// `access_violation_handler` must return with its [`MemoryRegion`] unmodified, or initialized
    /// to another equally correct [`MemoryRegion`] as described in the safety invariants for
    /// `new_uninitialized`.
    pub unsafe fn new_with_access_violation_handler(
        regions: Vec<MemoryRegion>,
        config: &Config,
        sbpf_version: SBPFVersion,
        access_violation_handler: AccessViolationHandler,
    ) -> Result<Self, EbpfError> {
        let mut mapping =
            Self::new_uninitialized(regions, config, sbpf_version, access_violation_handler);
        mapping.initialize()?;
        Ok(mapping)
    }

    /// Creates an unitialized memory mapping
    ///
    /// # Safety
    ///
    /// The memory pointed to by the [`MemoryRegion`]s must point to a valid object live for the
    /// duration of this `MemoryMapping`.
    ///
    /// For `MemoryRegion`s marked writable, the memory pointed to by the memory region must accept
    /// arbitrary bytes being overwritten without it resulting in unsoundness (due to e.g. broken
    /// internal type invariants.)
    pub unsafe fn new_uninitialized(
        regions: Vec<MemoryRegion>,
        config: &Config,
        sbpf_version: SBPFVersion,
        access_violation_handler: AccessViolationHandler,
    ) -> Self {
        let ty = if sbpf_version >= SBPFVersion::V4 || config.aligned_memory_mapping {
            MemoryMappingType::Aligned(AlignedMemoryMapping::new_uninitialized(regions))
        } else {
            debug_assert!(
                sbpf_version <= SBPFVersion::V3,
                "SBPFv4 and later versions do not support unaligned memory"
            );
            MemoryMappingType::Unaligned(UnalignedMemoryMapping::new_uninitialized(regions))
        };

        Self {
            access_violation_handler: Box::new(access_violation_handler),
            max_call_depth: config.max_call_depth as i64,
            stack_frame_size: config.stack_frame_size as i64,
            disable_address_translation: !config.enable_address_translation,
            sbpf_version,
            initialized: false,
            ty,
        }
    }

    /// Creates a new memory mapping for tests and benches.
    ///
    /// `access_violation_handler` defaults to a function which always returns an error.
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    pub unsafe fn new(
        regions: Vec<MemoryRegion>,
        config: &Config,
        sbpf_version: SBPFVersion,
    ) -> Result<Self, EbpfError> {
        Self::new_with_access_violation_handler(
            regions,
            config,
            sbpf_version,
            Box::new(default_access_violation_handler),
        )
    }

    /// Map virtual memory to host memory.
    pub fn map(
        &self,
        access_type: AccessType,
        vm_addr: u64,
        len: u64,
    ) -> StableResult<HostBuffer, EbpfError> {
        debug_assert!(self.initialized);
        if self.disable_address_translation {
            // NOTE TRICKY: this pointer most likely did *not* get its provenance exposed in the
            // Rust-land! This option in general is extremely unsafe and have us constructing
            // pointers to no man's land. We acknowledge this and don't consider it to be a bug,
            // given that the option isn't meant to be used for any serious applications of this
            // crate.
            let ptr = ptr::with_exposed_provenance_mut(vm_addr as usize);
            let buffer = HostBuffer::Mutable(ptr::slice_from_raw_parts_mut(ptr, len as usize));
            return StableResult::Ok(buffer);
        }
        if let Some((_index, region)) = self.find_region(vm_addr) {
            if region.host_buffer().is_mutable() || access_type != AccessType::Store {
                if let Some(host_buffer) = region.vm_to_host_buffer(vm_addr, len) {
                    return StableResult::Ok(host_buffer);
                }
            }
        }
        StableResult::Err(self.generate_access_violation(access_type, vm_addr, len))
    }

    /// Map virtual memory to host memory and potentially call the [AccessViolationHandler].
    ///
    /// This requires the [MemoryMapping] to be mutable and
    /// can cause previously translated addresses to become invalid.
    #[inline(always)]
    pub fn map_with_access_violation_handler(
        &mut self,
        access_type: AccessType,
        vm_addr: u64,
        len: u64,
    ) -> StableResult<HostBuffer, EbpfError> {
        debug_assert!(self.initialized);
        if self.disable_address_translation {
            // NOTE TRICKY: this pointer most likely did *not* get its provenance exposed in the
            // Rust-land! This option in general is extremely unsafe and have us constructing
            // pointers to no man's land. We acknowledge this and don't consider it to be a bug,
            // given that the option isn't meant to be used for any serious applications of this
            // crate.
            let ptr = ptr::with_exposed_provenance_mut(vm_addr as usize);
            let buffer = HostBuffer::Mutable(ptr::slice_from_raw_parts_mut(ptr, len as usize));
            return StableResult::Ok(buffer);
        }

        let index = match self.find_region(vm_addr) {
            Some((index, region)) => {
                if region.host_buffer().is_mutable() || access_type != AccessType::Store {
                    if let Some(host_buffer) = region.vm_to_host_buffer(vm_addr, len) {
                        return StableResult::Ok(host_buffer);
                    }
                }
                index
            }
            None => usize::MAX,
        };
        // The slow path is out of line, so that the code in there doesn't make the callers of the
        // fast path save and restore registers they otherwise do not need to. Note, that a single
        // call site for the cold path matters. Split it up and LLVM merges their results with the
        // fast path's through the stack.
        self.map_with_access_violation_handler_slow(index, access_type, vm_addr, len)
    }

    // `#[cold]`, `#[inline(never)]` have been experimentally determined to improve mapping function
    // throughput.
    // In the future using a `rust-cold` calling convention would help this further.
    #[cold]
    #[inline(never)]
    fn map_with_access_violation_handler_slow(
        &mut self,
        region_index: usize,
        access_type: AccessType,
        vm_addr: u64,
        len: u64,
    ) -> StableResult<HostBuffer, EbpfError> {
        let regions = self.get_regions();
        let Some(region) = regions.get(region_index) else {
            debug_assert!(self.find_region(vm_addr).is_none());
            return StableResult::Err(self.generate_access_violation_for(
                None,
                access_type,
                vm_addr,
                len,
            ));
        };
        let mut region = region.clone();
        let max_len = regions
            .get(region_index.saturating_add(1))
            .map_or(u64::MAX, |next_region| next_region.vm_addr)
            .saturating_sub(region.vm_addr);
        (self.access_violation_handler)(&mut region, max_len, access_type, vm_addr, len);
        if region.host_buffer().is_mutable() || access_type != AccessType::Store {
            if let Some(host_buffer) = region.vm_to_host_buffer(vm_addr, len) {
                if let Err(err) = unsafe { self.replace_region(region_index, region) } {
                    return StableResult::Err(err);
                }
                return StableResult::Ok(host_buffer);
            }
        }
        // The handler did not change the region in the mapping, so the original one still applies.
        let region = self.get_regions().get(region_index);
        StableResult::Err(self.generate_access_violation_for(region, access_type, vm_addr, len))
    }

    /// Loads `size_of::<T>()` bytes at the given guest address.
    #[inline]
    pub fn load<T: Pod + Into<u64>>(&mut self, vm_addr: u64) -> ProgramResult {
        let len = mem::size_of::<T>() as u64;
        debug_assert!(len <= mem::size_of::<u64>() as u64);
        debug_assert!(self.initialized);
        let ptr = match self.map_with_access_violation_handler(AccessType::Load, vm_addr, len) {
            StableResult::Err(e) => return ProgramResult::Err(e),
            StableResult::Ok(buf) => buf.ptr(),
        };
        ProgramResult::Ok(unsafe {
            // SAFETY:
            //
            // Contract from `ptr::read_unaligned`: `src` must be valid for reads.
            // Evidence: So long as `disable_address_translation` is not `true`, `map_*` only
            // returns valid, allocated subslices of memory.
            // Contract from `ptr::read_unaligned`: `src` must point to a properly initialized value
            // of type T.
            // Evidence: `T: Pod`.
            ptr::read_unaligned::<T>(ptr.cast()).into()
        })
    }

    /// Store `value` at the given guest address.
    #[inline]
    pub fn store<T: Pod>(&mut self, value: T, vm_addr: u64) -> ProgramResult {
        let len = mem::size_of::<T>() as u64;
        debug_assert!(len <= mem::size_of::<u64>() as u64);
        debug_assert!(self.initialized);
        let ptr = match self.map_with_access_violation_handler(AccessType::Store, vm_addr, len) {
            StableResult::Err(e) => return ProgramResult::Err(e),
            StableResult::Ok(buf) => buf.ptr_mut(),
        };
        StableResult::Ok(unsafe {
            // SAFETY:
            //
            // Contract from `ptr::read_unaligned`: `src` must be valid for reads.
            // Evidence: So long as `disable_address_translation` is not `true`, `map_*` only
            // returns valid, allocated subslices of memory.
            // Contract from `ptr::read_unaligned`: `src` must point to a properly initialized value
            // of type T.
            // Evidence: `T: Pod`.
            ptr::write_unaligned::<T>(ptr.cast(), value);
            0
        })
    }

    /// Returns the `MemoryRegion` which may contain the given address.
    #[inline(always)]
    pub fn find_region(&self, vm_addr: u64) -> Option<(usize, &MemoryRegion)> {
        debug_assert!(self.initialized);
        match &self.ty {
            MemoryMappingType::Aligned(inner) => inner.find_region(vm_addr),
            MemoryMappingType::Unaligned(inner) => inner.find_region(vm_addr),
        }
    }

    /// Returns the `MemoryRegion`s in this mapping.
    #[inline(always)]
    pub fn get_regions(&self) -> &[MemoryRegion] {
        match &self.ty {
            MemoryMappingType::Aligned(inner) => &inner.regions,
            MemoryMappingType::Unaligned(inner) => &inner.regions,
        }
    }

    /// Returns the [`MemoryRegion`]s as mutable.
    ///
    /// Modifying the regions might break the initialization constraints, so this function
    /// uninitializes the mapping. The memory mappings must be initialized
    /// again with [`Self::initialize`] before further use.
    pub fn get_regions_mut(&mut self) -> &mut [MemoryRegion] {
        self.initialized = false;

        let regions = match &mut self.ty {
            MemoryMappingType::Aligned(inner) => inner.regions.as_mut_slice(),
            MemoryMappingType::Unaligned(inner) => &mut inner.regions,
        };

        regions
    }

    /// Replaces the `MemoryRegion` at the given index
    ///
    /// # Safety
    ///
    /// Refer to [`MemoryMapping::new_uninitialized`].
    #[inline(always)]
    pub unsafe fn replace_region(
        &mut self,
        index: usize,
        region: MemoryRegion,
    ) -> Result<(), EbpfError> {
        debug_assert!(self.initialized);
        let regions = self.get_regions();
        let next_region_start = regions
            .get(index.saturating_add(1))
            .map_or(u64::MAX, |next_region| next_region.vm_addr);
        if index >= regions.len()
            || regions[index].vm_addr != region.vm_addr
            || region.vm_addr_range().end > next_region_start
        {
            return Err(EbpfError::InvalidMemoryRegion(index));
        }
        match &mut self.ty {
            MemoryMappingType::Aligned(inner) => inner.replace_region(index, region),
            MemoryMappingType::Unaligned(inner) => inner.replace_region(index, region),
        }
    }

    /// Initialize the MemoryMapping
    pub fn initialize(&mut self) -> Result<(), EbpfError> {
        let result = match &mut self.ty {
            MemoryMappingType::Aligned(inner) => inner.initialize(),
            MemoryMappingType::Unaligned(inner) => inner.initialize(),
        };
        self.initialized = result.is_ok();
        result
    }

    fn generate_access_violation(
        &self,
        access_type: AccessType,
        vm_addr: u64,
        len: u64,
    ) -> EbpfError {
        self.generate_access_violation_for(
            self.find_region(vm_addr).map(|(_, region)| region),
            access_type,
            vm_addr,
            len,
        )
    }

    /// Like [`Self::generate_access_violation`], with `region` being what
    /// `find_region(vm_addr)` returns.
    fn generate_access_violation_for(
        &self,
        region: Option<&MemoryRegion>,
        access_type: AccessType,
        vm_addr: u64,
        len: u64,
    ) -> EbpfError {
        let stack_frame = (vm_addr as i64)
            .saturating_sub(ebpf::MM_STACK_START as i64)
            .checked_div(self.stack_frame_size)
            .unwrap_or(0);
        if !self.sbpf_version.manual_stack_frame_bump()
            && (-1..self.max_call_depth.saturating_add(1)).contains(&stack_frame)
        {
            EbpfError::StackAccessViolation(access_type, vm_addr, len, stack_frame)
        } else {
            let region_name = match vm_addr & (!ebpf::MM_BYTECODE_START.saturating_sub(1)) {
                _ if region.map(|r| r.vm_addr_range().contains(&vm_addr)) != Some(true) => {
                    "unallocated"
                }
                ebpf::MM_BYTECODE_START => "program",
                ebpf::MM_STACK_START => "stack",
                ebpf::MM_HEAP_START => "heap",
                ebpf::MM_INPUT_START => "input",
                _ => "allocated",
            };
            EbpfError::AccessViolation(access_type, vm_addr, len, region_name)
        }
    }
}

/// Fast, small linear cache used to speed up unaligned memory mapping.
#[derive(Debug)]
struct MappingCache {
    // The cached entries.
    entries: [(Range<u64>, usize); MappingCache::SIZE],
    // Index of the last accessed memory region.
    //
    // New entries are written backwards, so that find() can always scan
    // forward which is faster.
    head: usize,
}

impl MappingCache {
    // must be a power of two
    const SIZE: usize = 4;

    fn new() -> MappingCache {
        MappingCache {
            entries: array::from_fn(|_| (0..0, 0)),
            head: 0,
        }
    }

    #[inline]
    fn find(&self, vm_addr: u64) -> Option<usize> {
        for i in 0..Self::SIZE {
            let index = self.head.wrapping_add(i) % Self::SIZE;
            // Safety:
            // index is guaranteed to be between 0..Self::SIZE
            let (vm_range, region_index) = unsafe { self.entries.get_unchecked(index) };
            if vm_range.contains(&vm_addr) {
                return Some(*region_index);
            }
        }

        None
    }

    #[inline]
    fn insert(&mut self, vm_range: Range<u64>, region_index: usize) {
        self.head = self.head.wrapping_sub(1) % Self::SIZE;
        // Safety:
        // self.head is guaranteed to be between 0..Self::SIZE
        unsafe { *self.entries.get_unchecked_mut(self.head) = (vm_range, region_index) };
    }

    #[inline]
    fn flush(&mut self) {
        self.entries = array::from_fn(|_| (0..0, 0));
        self.head = 0;
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};
    use test_utils::assert_error;

    use super::*;

    #[test]
    fn test_mapping_cache() {
        let mut cache = MappingCache::new();
        assert_eq!(cache.find(0), None);

        let mut ranges = vec![10u64..20, 20..30, 30..40, 40..50];
        for (region, range) in ranges.iter().cloned().enumerate() {
            cache.insert(range, region);
        }
        for (region, range) in ranges.iter().enumerate() {
            if region > 0 {
                assert_eq!(cache.find(range.start - 1), Some(region - 1));
            } else {
                assert_eq!(cache.find(range.start - 1), None);
            }
            assert_eq!(cache.find(range.start), Some(region));
            assert_eq!(cache.find(range.start + 1), Some(region));
            assert_eq!(cache.find(range.end - 1), Some(region));
            if region < 3 {
                assert_eq!(cache.find(range.end), Some(region + 1));
            } else {
                assert_eq!(cache.find(range.end), None);
            }
        }

        cache.insert(50..60, 4);
        ranges.push(50..60);
        for (region, range) in ranges.iter().enumerate() {
            if region == 0 {
                assert_eq!(cache.find(range.start), None);
                continue;
            }
            if region > 1 {
                assert_eq!(cache.find(range.start - 1), Some(region - 1));
            } else {
                assert_eq!(cache.find(range.start - 1), None);
            }
            assert_eq!(cache.find(range.start), Some(region));
            assert_eq!(cache.find(range.start + 1), Some(region));
            assert_eq!(cache.find(range.end - 1), Some(region));
            if region < 4 {
                assert_eq!(cache.find(range.end), Some(region + 1));
            } else {
                assert_eq!(cache.find(range.end), None);
            }
        }
    }

    #[test]
    fn test_mapping_cache_flush() {
        let mut cache = MappingCache::new();
        assert_eq!(cache.find(0), None);
        cache.insert(0..10, 0);
        assert_eq!(cache.find(0), Some(0));
        cache.flush();
        assert_eq!(cache.find(0), None);
    }

    #[test]
    fn test_map_empty() {
        for aligned_memory_mapping in [false, true] {
            let config = Config {
                aligned_memory_mapping,
                ..Config::default()
            };
            let m = unsafe { MemoryMapping::new(vec![], &config, SBPFVersion::V3) }.unwrap();
            assert_error!(
                m.map(AccessType::Load, ebpf::MM_REGION_SIZE, 8),
                "AccessViolation"
            );
        }
    }

    #[test]
    fn test_gapped_map() {
        for aligned_memory_mapping in [false, true] {
            let config = Config {
                aligned_memory_mapping,
                ..Config::default()
            };
            let mut mem1 = [0xff; 8];
            let mem2 = [0; 8];
            let mut m = unsafe {
                MemoryMapping::new(
                    vec![
                        MemoryRegion::new(&raw const mem2[..], ebpf::MM_REGION_SIZE),
                        MemoryRegion::new_gapped(&raw mut mem1[..], ebpf::MM_REGION_SIZE * 2, 2),
                    ],
                    &config,
                    SBPFVersion::V3,
                )
                .unwrap()
            };
            for frame in 0..4 {
                let address = ebpf::MM_STACK_START + frame * 4;
                assert!(m.find_region(address).is_some());
                assert!(m.map(AccessType::Load, address, 2).is_ok());
                assert_error!(m.map(AccessType::Load, address + 2, 2), "AccessViolation");
                assert_eq!(m.load::<u16>(address).unwrap(), 0xFFFF);
                assert_error!(m.load::<u16>(address + 2), "AccessViolation");
                assert!(m.store::<u16>(0xFFFF, address).is_ok());
                assert_error!(m.store::<u16>(0xFFFF, address + 2), "AccessViolation");
            }
        }
    }

    #[test]
    fn test_unaligned_map_overlap() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };
        let mem1 = [1, 2, 3, 4];
        let mem2 = [5, 6];
        assert_error!(
            unsafe {
                MemoryMapping::new(
                    vec![
                        MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE),
                        MemoryRegion::new(
                            &raw const mem2,
                            ebpf::MM_REGION_SIZE + mem1.len() as u64 - 1,
                        ),
                    ],
                    &config,
                    SBPFVersion::V3,
                )
            },
            "InvalidMemoryRegion(1)"
        );
        assert!(unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                ],
                &config,
                SBPFVersion::V3,
            )
        }
        .is_ok());
    }

    #[test]
    fn test_unaligned_map() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };
        let mut mem1 = [11];
        let mem2 = [22, 22];
        let mem3 = [33];
        let mem4 = [44, 44];
        let m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                    MemoryRegion::new(
                        &raw const mem3,
                        ebpf::MM_REGION_SIZE + (mem1.len() + mem2.len()) as u64,
                    ),
                    MemoryRegion::new(
                        &raw const mem4,
                        ebpf::MM_REGION_SIZE + (mem1.len() + mem2.len() + mem3.len()) as u64,
                    ),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };

        assert_eq!(
            m.map(AccessType::Load, ebpf::MM_REGION_SIZE, 1)
                .unwrap()
                .ptr()
                .addr(),
            mem1.as_ptr().addr()
        );

        assert_eq!(
            m.map(AccessType::Store, ebpf::MM_REGION_SIZE, 1)
                .unwrap()
                .ptr()
                .addr(),
            mem1.as_ptr().addr()
        );

        assert_error!(
            m.map(AccessType::Load, ebpf::MM_REGION_SIZE, 2),
            "AccessViolation"
        );

        assert_eq!(
            m.map(
                AccessType::Load,
                ebpf::MM_REGION_SIZE + mem1.len() as u64,
                1,
            )
            .unwrap()
            .ptr()
            .addr(),
            mem2.as_ptr().addr()
        );

        assert_eq!(
            m.map(
                AccessType::Load,
                ebpf::MM_REGION_SIZE + (mem1.len() + mem2.len()) as u64,
                1,
            )
            .unwrap()
            .ptr()
            .addr(),
            mem3.as_ptr().addr()
        );

        assert_eq!(
            m.map(
                AccessType::Load,
                ebpf::MM_REGION_SIZE + (mem1.len() + mem2.len() + mem3.len()) as u64,
                1,
            )
            .unwrap()
            .ptr()
            .addr(),
            mem4.as_ptr().addr()
        );

        assert_error!(
            m.map(
                AccessType::Load,
                ebpf::MM_REGION_SIZE + (mem1.len() + mem2.len() + mem3.len() + mem4.len()) as u64,
                1,
            ),
            "AccessViolation"
        );
    }

    #[test]
    fn test_unaligned_region() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };

        let mut mem1 = [0xFF; 4];
        let mem2 = [0xDD; 4];
        let m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + 4),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };
        assert!(m.find_region(ebpf::MM_REGION_SIZE - 1).is_none());
        assert_eq!(
            HostBuffer::Mutable(&raw mut mem1[..]),
            m.find_region(ebpf::MM_REGION_SIZE).unwrap().1.host,
        );
        assert_eq!(
            HostBuffer::Mutable(&raw mut mem1[..]),
            m.find_region(ebpf::MM_REGION_SIZE + 3).unwrap().1.host,
        );
        assert_eq!(
            HostBuffer::Immutable(&raw const mem2[..]),
            m.find_region(ebpf::MM_REGION_SIZE + 4).unwrap().1.host,
        );
        assert_eq!(
            HostBuffer::Immutable(&raw const mem2[..]),
            m.find_region(ebpf::MM_REGION_SIZE + 7).unwrap().1.host,
        );
        assert!(m.find_region(ebpf::MM_REGION_SIZE + 8).is_some());
    }

    #[test]
    fn test_aligned_region() {
        let config = Config {
            aligned_memory_mapping: true,
            ..Config::default()
        };

        let mut mem1 = [0xFF; 4];
        let mem2 = [0xDD; 4];
        let m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE * 2),
                ],
                &config,
                SBPFVersion::V4,
            )
            .unwrap()
        };
        assert_eq!(m.find_region(ebpf::MM_REGION_SIZE - 1).unwrap().1.len(), 0);
        assert_eq!(
            HostBuffer::Mutable(&raw mut mem1[..]),
            m.find_region(ebpf::MM_REGION_SIZE).unwrap().1.host,
        );
        assert_eq!(
            HostBuffer::Mutable(&raw mut mem1[..]),
            m.find_region(ebpf::MM_REGION_SIZE + 3).unwrap().1.host,
        );
        assert!(m.find_region(ebpf::MM_REGION_SIZE + 4).is_some());
        assert_eq!(
            HostBuffer::Immutable(&raw const mem2[..]),
            m.find_region(ebpf::MM_REGION_SIZE * 2).unwrap().1.host,
        );
        assert_eq!(
            HostBuffer::Immutable(&raw const mem2[..]),
            m.find_region(ebpf::MM_REGION_SIZE * 2 + 3).unwrap().1.host,
        );
        assert!(m.find_region(ebpf::MM_REGION_SIZE * 3 + 4).is_none());
    }

    #[test]
    fn test_unaligned_map_load() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };
        let mem1 = [0x11, 0x22];
        let mem2 = [0x33];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };

        assert_eq!(m.load::<u16>(ebpf::MM_REGION_SIZE).unwrap(), 0x2211);
        assert_error!(m.load::<u32>(ebpf::MM_REGION_SIZE), "AccessViolation");
        assert_error!(m.load::<u32>(ebpf::MM_REGION_SIZE + 4), "AccessViolation");
    }

    #[test]
    fn test_unaligned_map_store() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };
        let mut mem1 = [0xff, 0xff];
        let mut mem2 = [0xff];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw mut mem2, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };

        m.store(0x1122u16, ebpf::MM_REGION_SIZE).unwrap();
        assert_eq!(m.load::<u16>(ebpf::MM_REGION_SIZE).unwrap(), 0x1122);

        assert_error!(
            m.store(0x33445566u32, ebpf::MM_REGION_SIZE),
            "AccessViolation"
        );
    }

    #[test]
    fn test_unaligned_map_store_out_of_bounds() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };

        let mut mem1 = [0xFF];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE)],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };
        m.store(0x11u8, ebpf::MM_REGION_SIZE).unwrap();
        assert_error!(m.store(0x11u8, ebpf::MM_REGION_SIZE - 1), "AccessViolation");
        assert_error!(m.store(0x11u8, ebpf::MM_REGION_SIZE + 1), "AccessViolation");
        // this gets us line coverage for the case where we're completely
        // outside the address space (the case above is just on the edge)
        assert_error!(m.store(0x11u8, ebpf::MM_REGION_SIZE + 2), "AccessViolation");

        let mut mem1 = [0xFF; 4];
        let mut mem2 = [0xDD; 4];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw mut mem2, ebpf::MM_REGION_SIZE + 4),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };
        assert_error!(
            m.store(0x1122334455667788u64, ebpf::MM_REGION_SIZE),
            "AccessViolation"
        );
    }

    #[test]
    fn test_unaligned_map_load_out_of_bounds() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };

        let mem1 = [0xff];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE)],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };
        assert_eq!(m.load::<u8>(ebpf::MM_REGION_SIZE).unwrap(), 0xff);
        assert_error!(m.load::<u8>(ebpf::MM_REGION_SIZE - 1), "AccessViolation");
        assert_error!(m.load::<u8>(ebpf::MM_REGION_SIZE + 1), "AccessViolation");
        assert_error!(m.load::<u8>(ebpf::MM_REGION_SIZE + 2), "AccessViolation");

        let mem1 = [0xFF; 4];
        let mem2 = [0xDD; 4];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + 4),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };
        assert_error!(m.load::<u64>(ebpf::MM_REGION_SIZE), "AccessViolation");
    }

    #[test]
    #[should_panic(expected = "AccessViolation")]
    fn test_store_readonly() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };
        let mut mem1 = [0xff, 0xff];
        let mem2 = [0xff, 0xff];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw mut mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };
        m.store(0x11223344, ebpf::MM_REGION_SIZE).unwrap();
    }

    #[test]
    fn test_unaligned_map_replace_region() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };
        let mem1 = [11];
        let mem2 = [22, 22];
        let mem3 = [33];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                ],
                &config,
                SBPFVersion::V3,
            )
            .unwrap()
        };

        assert_eq!(
            m.map(AccessType::Load, ebpf::MM_REGION_SIZE, 1)
                .unwrap()
                .ptr()
                .addr(),
            mem1.as_ptr().addr()
        );

        assert_eq!(
            m.map(
                AccessType::Load,
                ebpf::MM_REGION_SIZE + mem1.len() as u64,
                1,
            )
            .unwrap()
            .ptr()
            .addr(),
            mem2.as_ptr().addr()
        );

        assert_error!(
            unsafe {
                m.replace_region(
                    2,
                    MemoryRegion::new(&raw const mem3, ebpf::MM_REGION_SIZE + mem1.len() as u64),
                )
            },
            "InvalidMemoryRegion(2)"
        );

        let region_index = m
            .get_regions()
            .iter()
            .position(|mem| mem.vm_addr == ebpf::MM_REGION_SIZE + mem1.len() as u64)
            .unwrap();

        // old.vm_addr != new.vm_addr
        assert_error!(
            unsafe {
                m.replace_region(
                    region_index,
                    MemoryRegion::new(
                        &raw const mem3,
                        ebpf::MM_REGION_SIZE + mem1.len() as u64 + 1,
                    ),
                )
            },
            "InvalidMemoryRegion({})",
            region_index
        );

        unsafe {
            m.replace_region(
                region_index,
                MemoryRegion::new(&raw const mem3, ebpf::MM_REGION_SIZE + mem1.len() as u64),
            )
            .unwrap()
        };

        assert_eq!(
            m.map(
                AccessType::Load,
                ebpf::MM_REGION_SIZE + mem1.len() as u64,
                1,
            )
            .unwrap()
            .ptr()
            .addr(),
            mem3.as_ptr().addr()
        );
    }

    #[test]
    fn test_aligned_map_replace_region() {
        let config = Config {
            aligned_memory_mapping: true,
            ..Config::default()
        };
        let mem1 = [11];
        let mem2 = [22, 22];
        let mem3 = [33, 33];
        let mut m = unsafe {
            MemoryMapping::new(
                vec![
                    MemoryRegion::new(&raw const mem1, ebpf::MM_REGION_SIZE),
                    MemoryRegion::new(&raw const mem2, ebpf::MM_REGION_SIZE * 2),
                ],
                &config,
                SBPFVersion::V4,
            )
            .unwrap()
        };

        assert_eq!(
            m.map(AccessType::Load, ebpf::MM_REGION_SIZE * 2, 1)
                .unwrap()
                .ptr()
                .addr(),
            mem2.as_ptr().addr()
        );

        // index > regions.len()
        assert_error!(
            unsafe {
                m.replace_region(
                    3,
                    MemoryRegion::new(&raw const mem3, ebpf::MM_REGION_SIZE * 2),
                )
            },
            "InvalidMemoryRegion(3)"
        );

        // index != addr >> VIRTUAL_ADDRESS_BITS
        assert_error!(
            unsafe {
                m.replace_region(
                    2,
                    MemoryRegion::new(&raw const mem3, ebpf::MM_REGION_SIZE * 3),
                )
            },
            "InvalidMemoryRegion(2)"
        );

        // index + len != addr >> VIRTUAL_ADDRESS_BITS
        assert_error!(
            unsafe {
                m.replace_region(
                    2,
                    MemoryRegion::new(&raw const mem3, ebpf::MM_REGION_SIZE * 3 - 1),
                )
            },
            "InvalidMemoryRegion(2)"
        );

        unsafe {
            m.replace_region(
                2,
                MemoryRegion::new(&raw const mem3, ebpf::MM_REGION_SIZE * 2),
            )
            .unwrap()
        };

        assert_eq!(
            m.map(AccessType::Load, ebpf::MM_REGION_SIZE * 2, 1)
                .unwrap()
                .ptr()
                .addr(),
            mem3.as_ptr().addr()
        );
    }

    #[test]
    fn test_access_violation_handler_map() {
        for aligned_memory_mapping in [true, false] {
            let config = Config {
                aligned_memory_mapping,
                ..Config::default()
            };
            let original = [11, 22];
            let copied = Rc::new(RefCell::new(Vec::new()));
            let mut regions = vec![MemoryRegion::new(&raw const original, ebpf::MM_REGION_SIZE)];
            regions[0].access_violation_handler_payload = Some(0);

            let c = Rc::clone(&copied);
            let mut m = unsafe {
                MemoryMapping::new_with_access_violation_handler(
                    regions,
                    &config,
                    SBPFVersion::V3,
                    Box::new(move |region, _, _, _, _| {
                        let mut vec = c.borrow_mut();
                        vec.extend_from_slice(&original);
                        region.redirect(&raw mut vec[..]);
                    }),
                )
                .unwrap()
            };

            assert_eq!(
                m.map_with_access_violation_handler(AccessType::Load, ebpf::MM_REGION_SIZE, 1)
                    .unwrap()
                    .ptr()
                    .addr(),
                original.as_ptr().addr()
            );
            assert_eq!(
                m.map_with_access_violation_handler(AccessType::Store, ebpf::MM_REGION_SIZE, 1)
                    .unwrap()
                    .ptr()
                    .addr(),
                copied.borrow().as_ptr().addr()
            );
        }
    }

    #[test]
    fn test_access_violation_handler_load_store() {
        for aligned_memory_mapping in [true, false] {
            let config = Config {
                aligned_memory_mapping,
                ..Config::default()
            };
            let original = [11, 22];
            let copied = Rc::new(RefCell::new(Vec::new()));
            let mut regions = vec![MemoryRegion::new(&raw const original, ebpf::MM_REGION_SIZE)];
            regions[0].access_violation_handler_payload = Some(0);

            let c = Rc::clone(&copied);
            let mut m = unsafe {
                MemoryMapping::new_with_access_violation_handler(
                    regions,
                    &config,
                    SBPFVersion::V3,
                    Box::new(move |region, _, _, _, _| {
                        let mut vec = c.borrow_mut();
                        vec.extend_from_slice(&original);
                        region.redirect(&raw mut vec[..]);
                    }),
                )
                .unwrap()
            };

            assert_eq!(
                m.map(AccessType::Load, ebpf::MM_REGION_SIZE, 1)
                    .unwrap()
                    .ptr()
                    .addr(),
                original.as_ptr().addr()
            );

            assert_eq!(m.load::<u8>(ebpf::MM_REGION_SIZE).unwrap(), 11);
            assert_eq!(m.load::<u8>(ebpf::MM_REGION_SIZE + 1).unwrap(), 22);
            assert!(copied.borrow().is_empty());

            m.store(33u8, ebpf::MM_REGION_SIZE).unwrap();
            assert_eq!(original[0], 11);
            assert_eq!(m.load::<u8>(ebpf::MM_REGION_SIZE).unwrap(), 33);
            assert_eq!(m.load::<u8>(ebpf::MM_REGION_SIZE + 1).unwrap(), 22);
        }
    }

    #[test]
    fn test_access_violation_handler_region_id() {
        for aligned_memory_mapping in [true, false] {
            let config = Config {
                aligned_memory_mapping,
                ..Config::default()
            };
            let original1 = [11, 22];
            let original2 = [33, 44];
            let copied = Rc::new(RefCell::new(Vec::new()));

            let mut regions = vec![
                MemoryRegion::new(&raw const original1, ebpf::MM_REGION_SIZE),
                MemoryRegion::new(&raw const original2, ebpf::MM_REGION_SIZE * 2),
            ];
            regions[0].access_violation_handler_payload = Some(42);

            let c = Rc::clone(&copied);
            let mut m = unsafe {
                MemoryMapping::new_with_access_violation_handler(
                    regions,
                    &config,
                    SBPFVersion::V3,
                    Box::new(move |region, _, _, _, _| {
                        // check that the argument passed to MemoryRegion::new is then passed to the
                        // callback
                        assert_eq!(region.access_violation_handler_payload, Some(42));
                        let mut vec = c.borrow_mut();
                        vec.extend_from_slice(&original1);
                        region.redirect(&raw mut vec[..]);
                    }),
                )
                .unwrap()
            };

            m.store(55u8, ebpf::MM_REGION_SIZE).unwrap();
            assert_eq!(original1[0], 11);
            assert_eq!(m.load::<u8>(ebpf::MM_REGION_SIZE).unwrap(), 55);
        }
    }

    #[test]
    #[should_panic(expected = "AccessViolation")]
    fn test_map_access_violation_handler_error() {
        let config = Config::default();
        let original = [11, 22];

        let m = unsafe {
            MemoryMapping::new_with_access_violation_handler(
                vec![MemoryRegion::new(&raw const original, ebpf::MM_REGION_SIZE)],
                &config,
                SBPFVersion::V4,
                Box::new(default_access_violation_handler),
            )
            .unwrap()
        };

        m.map(AccessType::Store, ebpf::MM_REGION_SIZE, 1).unwrap();
    }

    #[test]
    #[should_panic(expected = "AccessViolation")]
    fn test_store_access_violation_handler_error() {
        let config = Config::default();
        let original = [11, 22];

        let mut m = unsafe {
            MemoryMapping::new_with_access_violation_handler(
                vec![MemoryRegion::new(&raw const original, ebpf::MM_REGION_SIZE)],
                &config,
                SBPFVersion::V4,
                Box::new(default_access_violation_handler),
            )
            .unwrap()
        };

        m.store(33u8, ebpf::MM_REGION_SIZE).unwrap();
    }

    #[test]
    fn test_access_violation_region_identification() {
        let config = Config::default();
        let original = [11, 22];
        let region = 0x10_0000_0000;
        let mut m = unsafe {
            MemoryMapping::new(
                vec![MemoryRegion::new(&raw const original, region)],
                &config,
                SBPFVersion::V4,
            )
            .unwrap()
        };
        let store_err_inbound = m.store(33u8, region).unwrap_err();
        assert_eq!(
            store_err_inbound.to_string(),
            "Access violation writing 1 bytes at address 0x1000000000 (in allocated region)"
        );
        let store_err_oob = m.load::<u64>(region + 3).unwrap_err();
        assert_eq!(
            store_err_oob.to_string(),
            "Access violation reading 8 bytes at address 0x1000000003 (in unallocated region)"
        );
    }

    #[test]
    fn v4_aligned_mapping() {
        let config = Config {
            aligned_memory_mapping: false,
            ..Config::default()
        };

        let mem = [11, 12];
        let mapping = unsafe {
            MemoryMapping::new_with_access_violation_handler(
                vec![MemoryRegion::new(&raw const mem, ebpf::MM_REGION_SIZE)],
                &config,
                SBPFVersion::V4,
                Box::new(default_access_violation_handler),
            )
            .unwrap()
        };

        assert!(matches!(mapping.ty, MemoryMappingType::Aligned(_)));
    }
}
