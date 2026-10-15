use super::*;
use crate::user::mm::SpaceConfig;

fn memory(limit: u64) -> VirtualMemory {
    VirtualMemory::new_with_commit_limit(
        AddressSpace::new(SpaceConfig {
            va_limit: 1 << 32,
            arena_bytes: 16 << 20,
            reserved_phys: vec![],
        })
        .unwrap(),
        65536,
        1 << 32,
        limit,
    )
}

#[test]
fn lpc_shared_views_alias_real_bytes_with_offsets_and_last_page_charges() {
    let mut vm = memory(3 * PAGE_SIZE);
    let object = Arc::new(SharedObject::anonymous(4 * PAGE_SIZE).unwrap());
    let weak = Arc::downgrade(&object);
    let first = vm
        .map_shared_view(object.clone(), None, 0, 2 * PAGE_SIZE)
        .unwrap();
    let second = vm
        .map_shared_view(object.clone(), None, PAGE_SIZE, 2 * PAGE_SIZE)
        .unwrap();
    assert_eq!(vm.committed_bytes(), 3 * PAGE_SIZE);
    let q = vm.query(second.base).unwrap();
    assert_eq!(
        (q.kind, q.state, q.protect, q.size),
        (mem::MAPPED, mem::COMMIT, prot::READWRITE, 2 * PAGE_SIZE)
    );
    // The 4 KiB edge lies inside a 16 KiB host page on Apple Silicon.
    vm.space()
        .write(first.base + PAGE_SIZE - 2, &[1, 2, 3, 4])
        .unwrap();
    assert_eq!(vm.space().u16(second.base).unwrap(), 0x0403);
    vm.space().w32(second.base + 4, 0x8877_6655).unwrap();
    assert_eq!(
        vm.space().u32(first.base + PAGE_SIZE + 4).unwrap(),
        0x8877_6655
    );
    drop(object);
    vm.release(first.base).unwrap();
    assert_eq!(vm.committed_bytes(), 2 * PAGE_SIZE);
    assert!(weak.upgrade().is_some());
    assert_eq!(vm.space().u32(second.base + 4).unwrap(), 0x8877_6655);
    vm.retire_shared_view(&second).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
    assert!(weak.upgrade().is_none());
}

#[test]
fn lpc_shared_view_failures_preserve_reservations_aliases_and_commitment() {
    let mut vm = memory(2 * PAGE_SIZE);
    let object = Arc::new(SharedObject::anonymous(4 * PAGE_SIZE).unwrap());
    let owner = vm
        .map_shared_view(object.clone(), Some(0x2000_0000), 0, 2 * PAGE_SIZE)
        .unwrap();
    let alias = vm
        .map_shared_view(object.clone(), Some(0x2100_0000), 0, 2 * PAGE_SIZE)
        .unwrap();
    assert_eq!(vm.committed_bytes(), 2 * PAGE_SIZE);
    for (base, offset, size, expected) in [
        (0x2200_0000, 0, 0, VmError::InvalidParameter),
        (0x2200_0001, 0, PAGE_SIZE, VmError::InvalidParameter),
        (0x2200_0000, 1, PAGE_SIZE, VmError::InvalidParameter),
        (
            0x2200_0000,
            4 * PAGE_SIZE,
            PAGE_SIZE,
            VmError::InvalidParameter,
        ),
        (
            0x2200_0000,
            u64::MAX & !(PAGE_SIZE - 1),
            PAGE_SIZE,
            VmError::InvalidParameter,
        ),
        (
            0x2200_0000,
            2 * PAGE_SIZE,
            PAGE_SIZE,
            VmError::CommitmentLimit,
        ),
        (0x2000_0000, 0, PAGE_SIZE, VmError::Conflicting),
    ] {
        assert_eq!(
            vm.map_shared_view(object.clone(), Some(base), offset, size)
                .unwrap_err(),
            expected
        );
        assert_eq!(vm.committed_bytes(), 2 * PAGE_SIZE);
        assert!(vm.is_free(0x2200_0000, PAGE_SIZE));
    }
    vm.space().w8(owner.base, 0xA5).unwrap();
    assert_eq!(vm.space().u8(alias.base).unwrap(), 0xA5);
    vm.retire_shared_view(&owner).unwrap();
    assert_eq!(vm.committed_bytes(), 2 * PAGE_SIZE);
    vm.retire_shared_view(&alias).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
}

#[test]
fn lpc_view_generation_preserves_private_and_mapped_address_replacements() {
    let mut vm = memory(8 * PAGE_SIZE);
    let object = Arc::new(SharedObject::anonymous(PAGE_SIZE).unwrap());
    let old = vm
        .map_shared_view(object.clone(), Some(0x2000_0000), 0, PAGE_SIZE)
        .unwrap();
    vm.release(old.base).unwrap();
    vm.allocate(
        Some(old.base),
        PAGE_SIZE,
        mem::RESERVE | mem::COMMIT,
        prot::READWRITE,
    )
    .unwrap();
    vm.space().w32(old.base, 0x1234).unwrap();
    vm.retire_shared_view(&old).unwrap();
    assert_eq!(vm.space().u32(old.base).unwrap(), 0x1234);
    assert_eq!(vm.committed_bytes(), PAGE_SIZE);
    vm.release(old.base).unwrap();
    // Supplementary model stress test; native controls measured private reuse.
    let replacement = vm
        .map_shared_view(object, Some(old.base), 0, PAGE_SIZE)
        .unwrap();
    vm.space().w32(old.base, 0xABCD).unwrap();
    vm.retire_shared_view(&old).unwrap();
    assert_eq!(vm.space().u32(old.base).unwrap(), 0xABCD);
    vm.retire_shared_view(&replacement).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
}

#[test]
fn lpc_views_reject_private_decommit_and_preserve_shared_protection() {
    let mut vm = memory(4 * PAGE_SIZE);
    let object = Arc::new(SharedObject::anonymous(PAGE_SIZE).unwrap());
    let first = vm
        .map_shared_view(object.clone(), None, 0, PAGE_SIZE)
        .unwrap();
    let second = vm.map_shared_view(object, None, 0, PAGE_SIZE).unwrap();
    assert_eq!(
        vm.free(first.base + 1, 0, mem::RELEASE),
        Err(VmError::CannotDeleteSection)
    );
    assert_eq!(
        vm.decommit(first.base, PAGE_SIZE),
        Err(VmError::CannotDeleteSection)
    );
    assert_eq!(
        vm.commit(first.base, PAGE_SIZE, prot::READWRITE),
        Err(VmError::AlreadyCommitted)
    );
    assert_eq!(
        vm.protect(first.base, PAGE_SIZE, prot::READONLY).unwrap(),
        prot::READWRITE
    );
    assert!(vm.space().w8(first.base, 1).is_err());
    vm.space().w8(second.base, 0x5A).unwrap();
    assert_eq!(vm.space().u8(first.base).unwrap(), 0x5A);
    assert_eq!(
        vm.protect(first.base, PAGE_SIZE, prot::NOACCESS).unwrap(),
        prot::READONLY
    );
    assert!(vm.space().u8(first.base).is_err());
    assert_eq!(
        vm.protect(first.base, PAGE_SIZE, prot::WRITECOPY),
        Err(VmError::NotSupported)
    );
    assert_eq!(
        vm.protect(first.base, PAGE_SIZE, prot::READWRITE).unwrap(),
        prot::NOACCESS
    );
    assert_eq!(vm.space().u8(first.base).unwrap(), 0x5A);
    assert_eq!(vm.committed_bytes(), PAGE_SIZE);
    vm.retire_shared_view(&first).unwrap();
    vm.retire_shared_view(&second).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
}
