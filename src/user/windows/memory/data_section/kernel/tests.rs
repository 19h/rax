use super::*;
use crate::user::mm::SpaceConfig;
use crate::user::windows::object_namespace::data::DataRegion;

fn source() -> Arc<DataSection> {
    DataSection::private_readonly(
        65536,
        vec![
            DataRegion {
                offset: 0,
                size: 4096,
                bytes: Some(vec![0x5a; 4096].into()),
            },
            DataRegion {
                offset: 4096,
                size: 61440,
                bytes: None,
            },
        ],
    )
    .unwrap()
}

fn memory(commit_limit: u64) -> VirtualMemory {
    VirtualMemory::new_with_commit_limit(
        AddressSpace::new(SpaceConfig {
            va_limit: 1 << 32,
            arena_bytes: 4 << 20,
            reserved_phys: vec![],
        })
        .unwrap(),
        65536,
        1 << 32,
        commit_limit,
    )
}

#[test]
fn kernel_data_growth_keeps_guest_readonly_and_selected_inputs_private() {
    let source = source();
    let mut first = memory(16384);
    let mut second = memory(16384);
    let owner = first
        .map_kernel_data_view(source.clone(), 0x2000_0000)
        .unwrap();
    let other = second
        .map_kernel_data_view(source.clone(), 0x2000_0000)
        .unwrap();
    assert_eq!(
        first.commit(owner.base + 4096, 4096, prot::READONLY),
        Err(VmError::AlreadyCommitted)
    );
    first
        .commit_kernel_data_view(&owner, owner.base + 4096, 4096)
        .unwrap();
    first
        .write_kernel_data_view(&owner, owner.base + 4096, &[1, 2, 3, 4])
        .unwrap();
    assert_eq!(first.query(owner.base + 4096).unwrap().kind, mem::MAPPED);
    assert_eq!(
        first.query(owner.base + 4096).unwrap().protect,
        prot::READONLY
    );
    assert_eq!(
        first.space().bytes(owner.base + 4096, 4).unwrap(),
        [1, 2, 3, 4]
    );
    assert!(first.space().w8(owner.base + 4096, 9).is_err());
    assert!(second.space().u8(other.base + 4096).is_err());
    assert!(source.regions[1].bytes.is_none());
    first
        .write_kernel_data_view(&owner, owner.base, &[0x33])
        .unwrap();
    assert_eq!(second.space().u8(other.base).unwrap(), 0x5a);
    assert_eq!(source.regions[0].bytes.as_ref().unwrap()[0], 0x5a);
}

#[test]
fn kernel_data_stale_token_cannot_write_grow_or_retire_a_replacement() {
    let mut vm = memory(16384);
    let section = source();
    let old = vm
        .map_kernel_data_view(section.clone(), 0x2000_0000)
        .unwrap();
    vm.release(old.base).unwrap();
    let replacement = vm.map_kernel_data_view(section, old.base).unwrap();
    assert!(!vm.owns_kernel_data_view(&old));
    assert_eq!(
        vm.write_kernel_data_view(&old, old.base, &[0x77]),
        Err(VmError::InvalidAddress)
    );
    assert_eq!(
        vm.commit_kernel_data_view(&old, old.base + 4096, 4096),
        Err(VmError::InvalidAddress)
    );
    vm.retire_kernel_data_view(&old).unwrap();
    assert!(vm.owns_kernel_data_view(&replacement));
    assert_eq!(vm.space().u8(old.base).unwrap(), 0x5a);
    vm.protect(replacement.base, 4096, prot::NOACCESS).unwrap();
    assert!(!vm.owns_kernel_data_view(&replacement));
    vm.retire_kernel_data_view(&replacement).unwrap();
    assert!(vm.is_free(replacement.base, replacement.size));
}

#[test]
fn kernel_data_failed_growth_and_range_checks_leave_allocation_unchanged() {
    let mut vm = memory(4096);
    let owner = vm.map_kernel_data_view(source(), 0x2000_0000).unwrap();
    assert_eq!(
        vm.commit_kernel_data_view(&owner, owner.base + 4096, 4096),
        Err(VmError::CommitmentLimit)
    );
    assert_eq!(vm.query(owner.base + 4096).unwrap().state, mem::RESERVE);
    assert_eq!(
        vm.write_kernel_data_view(&owner, owner.base + 4096, &[1]),
        Err(VmError::NotCommitted)
    );
    assert_eq!(
        vm.write_kernel_data_view(&owner, owner.base + owner.size, &[1]),
        Err(VmError::InvalidAddress)
    );
    assert_eq!(
        vm.commit_kernel_data_view(&owner, owner.base + owner.size, 4096),
        Err(VmError::InvalidAddress)
    );
    assert!(vm.owns_kernel_data_view(&owner));
    assert_eq!(vm.space().u8(owner.base).unwrap(), 0x5a);
}
