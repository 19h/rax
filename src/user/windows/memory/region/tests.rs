use crate::user::mm::{AddressSpace, PAGE_SIZE, SpaceConfig};
use crate::user::windows::memory::{AllocKind, VirtualMemory, prot};

#[test]
fn allocation_region_counts_committed_runs_and_pagefile_origin_without_population() {
    let space = AddressSpace::new(SpaceConfig {
        va_limit: 1 << 32,
        arena_bytes: 64 * PAGE_SIZE,
        reserved_phys: Vec::new(),
    })
    .unwrap();
    let mut vm = VirtualMemory::new(space, 0x1_0000, 1 << 32);
    let base = vm
        .reserve(
            None,
            16 * PAGE_SIZE,
            prot::READWRITE,
            AllocKind::Private,
            false,
            None,
        )
        .unwrap();
    assert_eq!(
        vm.allocation_region(base + 9 * PAGE_SIZE).unwrap().commit,
        0
    );
    vm.commit(base + PAGE_SIZE, 3 * PAGE_SIZE, prot::READWRITE)
        .unwrap();
    assert_eq!(
        vm.allocation_region(base + 9 * PAGE_SIZE).unwrap().commit,
        3 * PAGE_SIZE
    );
    vm.decommit(base + 2 * PAGE_SIZE, PAGE_SIZE).unwrap();
    assert_eq!(vm.allocation_region(base).unwrap().commit, 2 * PAGE_SIZE);
    let shared = vm.create_scheduler_view().unwrap();
    let info = vm.allocation_region(shared.base).unwrap();
    assert_eq!((info.kind, info.flags, info.commit), (0x0800_0000, 8, 0));
    vm.release(shared.base).unwrap();
    vm.reserve(
        Some(shared.base),
        2 * PAGE_SIZE,
        prot::READWRITE,
        AllocKind::Mapped,
        false,
        None,
    )
    .unwrap();
    let info = vm.allocation_region(shared.base).unwrap();
    assert_eq!((info.kind, info.flags, info.commit), (0x0004_0000, 2, 0));
}
