//! Allocation-wide region information, with native/WoW64 field layouts.
use super::{Outcome, WinArch, arguments, dispatch, query, read_ptr, setup};
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::memory::{Mem, mem, prot};
use crate::user::windows::nt::status::{STATUS_INVALID_HANDLE, STATUS_SUCCESS};

fn private_region(class: u32) {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        let output = base + 128;
        let returned = base + 256;
        assert_eq!(
            query(
                p,
                &mut thread,
                base + PAGE_SIZE,
                class,
                output,
                64,
                returned
            ),
            STATUS_SUCCESS
        );
        let width = if arch == WinArch::X86 { 4 } else { 8 };
        let size_offset = if width == 4 { 12 } else { 16 };
        let full_size = if width == 4 { 28 } else { 48 };
        assert_eq!(read_ptr(p, output), base);
        assert_eq!(p.space.u32(output + width).unwrap(), prot::READWRITE);
        assert_eq!(
            p.space.u32(output + width + 4).unwrap(),
            if class == 3 { mem::PRIVATE } else { 1 }
        );
        assert_eq!(read_ptr(p, output + size_offset), PAGE_SIZE * 4);
        assert_eq!(read_ptr(p, output + size_offset + width), PAGE_SIZE * 4);
        assert_eq!(read_ptr(p, output + size_offset + width * 2), 0);
        assert_eq!(
            read_ptr(p, output + size_offset + width * 3),
            u64::from(u32::MAX)
        );
        assert_eq!(read_ptr(p, returned), full_size);
        assert_eq!(p.space.u8(output + full_size).unwrap(), 0xA5);
    }
}

#[test]
fn native_region_information_private_allocation_all_abis() {
    private_region(3);
}

#[test]
fn native_region_information_ex_private_allocation_all_abis() {
    private_region(7);
}

#[test]
fn region_failed_query_output_and_return_probes_acquire_the_native_copy_charge() {
    use crate::user::windows::memory::AllocKind;
    for arch in WinArch::ALL {
        for returned_in_image in [false, true] {
            let (mut process, mut thread, base) = setup(arch);
            let p = process.state_mut();
            let image =
                p.vm.reserve(
                    None,
                    3 * PAGE_SIZE,
                    prot::EXECUTE_WRITECOPY,
                    AllocKind::Image,
                    false,
                    None,
                )
                .unwrap();
            p.vm.commit(image, 3 * PAGE_SIZE, prot::READONLY).unwrap();
            p.vm.space()
                .track_private_copies(image, 3 * PAGE_SIZE)
                .unwrap();
            p.vm.protect(image, PAGE_SIZE, prot::READWRITE).unwrap();
            let (output, returned) = if returned_in_image {
                (base + 128, image)
            } else {
                (image, base + 256)
            };
            arguments(p, &mut thread, &[0, image, 3, output, 48, returned]);
            assert_eq!(dispatch(p, &mut thread), Outcome::Continue);
            assert_eq!(thread.cpu.gpr(0) as u32, STATUS_INVALID_HANDLE);
            assert_eq!(p.space.u64(image).unwrap(), 0);
            p.vm.protect(image, PAGE_SIZE, prot::READONLY).unwrap();
            let expected = if returned_in_image || arch != WinArch::X86 {
                PAGE_SIZE
            } else {
                0
            };
            assert_eq!(
                p.vm.allocation_region(image).unwrap().commit,
                expected,
                "{arch}/{returned_in_image}"
            );
        }
    }
}

#[test]
fn region_query_serializes_explicit_numa_zero_and_default_after_reuse_all_abis() {
    use crate::user::windows::memory::{AddressRequirements, mem};
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        let allocated =
            p.vm.allocate_extended_numa(
                None,
                PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
                AddressRequirements::default(),
                Some(0),
            )
            .unwrap()
            .0;
        for class in [3, 7] {
            assert_eq!(
                query(p, &mut thread, allocated, class, base + 128, 64, 0),
                STATUS_SUCCESS
            );
            assert_eq!(read_ptr(p, base + 136 + arch.ptr_size() * 4), 0);
        }
        p.vm.release(allocated).unwrap();
        p.vm.allocate(
            Some(allocated),
            PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap();
        assert_eq!(
            query(p, &mut thread, allocated, 7, base + 128, 64, 0),
            STATUS_SUCCESS
        );
        assert_eq!(
            read_ptr(p, base + 136 + arch.ptr_size() * 4),
            u64::from(u32::MAX)
        );
    }
}

#[test]
fn region_guard_probes_charge_only_a_completed_wow_aliased_output_write() {
    use crate::user::windows::memory::AllocKind;
    use crate::user::windows::nt::status::STATUS_GUARD_PAGE_VIOLATION;
    for arch in WinArch::ALL {
        for role in 0..4 {
            let (mut process, mut thread, base) = setup(arch);
            let p = process.state_mut();
            let image =
                p.vm.reserve(
                    None,
                    3 * PAGE_SIZE,
                    prot::EXECUTE_WRITECOPY,
                    AllocKind::Image,
                    false,
                    None,
                )
                .unwrap();
            p.vm.commit(image, 3 * PAGE_SIZE, prot::READONLY).unwrap();
            p.vm.space()
                .track_private_copies(image, 3 * PAGE_SIZE)
                .unwrap();
            p.vm.protect(image, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                .unwrap();
            let output = if role == 0 || role == 2 {
                image
            } else {
                base + 128
            };
            let returned = if role == 0 { base + 256 } else { image };
            let handle = if role == 3 { 0 } else { arch.ptr(u64::MAX) };
            arguments(p, &mut thread, &[handle, image, 3, output, 64, returned]);
            assert_eq!(dispatch(p, &mut thread), Outcome::Continue);
            let status = if arch != WinArch::X86 || role == 0 {
                STATUS_GUARD_PAGE_VIOLATION
            } else if role == 3 {
                STATUS_INVALID_HANDLE
            } else {
                STATUS_SUCCESS
            };
            assert_eq!(thread.cpu.gpr(0) as u32, status, "{arch}/{role}");
            assert_eq!(p.vm.query(image).unwrap().protect & prot::GUARD, 0);
            p.vm.protect(image, PAGE_SIZE, prot::READONLY).unwrap();
            assert_eq!(
                p.vm.allocation_region(image).unwrap().commit,
                if arch == WinArch::X86 && role == 2 {
                    PAGE_SIZE
                } else {
                    0
                },
                "{arch}/{role}"
            );
        }
    }
}
