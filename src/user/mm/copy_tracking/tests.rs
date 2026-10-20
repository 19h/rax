use crate::error::MemoryAccessKind;
use crate::user::mm::{AddressSpace, Mapping, PAGE_SIZE, Perms, SpaceConfig};

const BASE: u64 = 0x1_0000;
const RW: Perms = Perms::READ.union(Perms::WRITE);

fn space() -> AddressSpace {
    AddressSpace::new(SpaceConfig {
        va_limit: 1 << 32,
        arena_bytes: 64 * PAGE_SIZE,
        reserved_phys: Vec::new(),
    })
    .unwrap()
}

#[test]
fn private_copy_charge_tracks_warm_cold_identical_and_failed_accesses() {
    let space = space();
    space
        .map(BASE, 4 * PAGE_SIZE, Mapping::anonymous(RW))
        .unwrap();
    space.write(BASE, &[0x5A]).unwrap();
    space.track_private_copies(BASE, 3 * PAGE_SIZE).unwrap();
    assert_eq!(space.acquired_private_bytes(BASE, 4 * PAGE_SIZE), 0);
    let mut byte = [0];
    space.read(BASE, &mut byte).unwrap();
    space.read(BASE + PAGE_SIZE, &mut byte).unwrap();
    assert_eq!(space.acquired_private_bytes(BASE, 4 * PAGE_SIZE), 0);
    // Identical bytes still acquire charge on an already populated page.
    space.write(BASE, &[0x5A]).unwrap();
    // A write probe acquires charge without storing; this page was populated by a read.
    space
        .translate(BASE + PAGE_SIZE, MemoryAccessKind::Write)
        .unwrap();
    // This first access populates and acquires the third page.
    space.write(BASE + 2 * PAGE_SIZE, &[0]).unwrap();
    space.write(BASE + 3 * PAGE_SIZE, &[0]).unwrap();
    assert_eq!(
        space.acquired_private_bytes(BASE, 4 * PAGE_SIZE),
        3 * PAGE_SIZE
    );
    space.protect(BASE, 3 * PAGE_SIZE, Perms::READ).unwrap();
    assert_eq!(
        space.acquired_private_bytes(BASE, 4 * PAGE_SIZE),
        3 * PAGE_SIZE
    );
    assert!(space.write(BASE, &[0x5A]).is_err());
    assert_eq!(
        space.private_copy_capacity(BASE, 4 * PAGE_SIZE),
        3 * PAGE_SIZE
    );
}

#[test]
fn private_copy_charge_survives_remap_but_not_discard_unmap_or_reuse() {
    let space = space();
    space
        .map(BASE, 4 * PAGE_SIZE, Mapping::anonymous(RW))
        .unwrap();
    space.track_private_copies(BASE, 4 * PAGE_SIZE).unwrap();
    space.write_raw(BASE, &[0]).unwrap();
    space.write(BASE + PAGE_SIZE, &[0]).unwrap();
    space.set_flags(BASE, 2 * PAGE_SIZE, u32::MAX, 7).unwrap();
    space.protect(BASE, PAGE_SIZE, Perms::READ).unwrap();
    let moved = 0x10_0000;
    space.remap(BASE, 4 * PAGE_SIZE, moved).unwrap();
    assert_eq!(space.acquired_private_bytes(BASE, 4 * PAGE_SIZE), 0);
    assert_eq!(space.private_copy_capacity(BASE, 4 * PAGE_SIZE), 0);
    assert_eq!(
        space.acquired_private_bytes(moved, 4 * PAGE_SIZE),
        2 * PAGE_SIZE
    );
    assert_eq!(
        space.private_copy_capacity(moved, 4 * PAGE_SIZE),
        4 * PAGE_SIZE
    );
    space.discard(moved, PAGE_SIZE).unwrap();
    assert_eq!(
        space.acquired_private_bytes(moved, 4 * PAGE_SIZE),
        PAGE_SIZE
    );
    assert_eq!(
        space.private_copy_capacity(moved, 4 * PAGE_SIZE),
        4 * PAGE_SIZE
    );
    assert!(space.write(moved, &[0]).is_err());
    assert_eq!(
        space.acquired_private_bytes(moved, 4 * PAGE_SIZE),
        PAGE_SIZE
    );
    space.write_raw(moved, &[0]).unwrap();
    assert_eq!(
        space.acquired_private_bytes(moved, 4 * PAGE_SIZE),
        2 * PAGE_SIZE
    );
    space.unmap(moved, 2 * PAGE_SIZE).unwrap();
    assert_eq!(space.acquired_private_bytes(moved, 4 * PAGE_SIZE), 0);
    assert_eq!(
        space.private_copy_capacity(moved, 4 * PAGE_SIZE),
        2 * PAGE_SIZE
    );
    space
        .map(moved, 4 * PAGE_SIZE, Mapping::anonymous(RW))
        .unwrap();
    space.write_raw(moved, &[0]).unwrap();
    assert_eq!(space.private_copy_capacity(moved, 4 * PAGE_SIZE), 0);
    assert_eq!(space.acquired_private_bytes(moved, 4 * PAGE_SIZE), 0);
}

#[test]
fn private_copy_write_probes_acquire_once_under_concurrent_translation() {
    let space = space();
    space.map(BASE, PAGE_SIZE, Mapping::anonymous(RW)).unwrap();
    space.track_private_copies(BASE, PAGE_SIZE).unwrap();
    space.translate(BASE, MemoryAccessKind::Read).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let space = &space;
            scope.spawn(move || {
                for _ in 0..100 {
                    space.translate(BASE, MemoryAccessKind::Write).unwrap();
                }
            });
        }
    });
    assert_eq!(space.acquired_private_bytes(BASE, PAGE_SIZE), PAGE_SIZE);
    space.protect(BASE, PAGE_SIZE, Perms::READ).unwrap();
    assert!(space.translate(BASE, MemoryAccessKind::Write).is_err());
    assert_eq!(space.acquired_private_bytes(BASE, PAGE_SIZE), PAGE_SIZE);
}

#[test]
fn private_copy_raw_write_to_readonly_shared_backing_acquires_no_charge() {
    use crate::user::mm::{Backing, SharedObject, anonymous_file};
    use std::sync::Arc;
    let space = space();
    let file = anonymous_file().unwrap();
    file.set_len(PAGE_SIZE).unwrap();
    let object = Arc::new(SharedObject::file(file, false).unwrap());
    space
        .map(
            BASE,
            PAGE_SIZE,
            Mapping {
                perms: Perms::READ,
                backing: Backing::Shared { object, offset: 0 },
                shared: true,
                name: None,
                flags: 0,
            },
        )
        .unwrap();
    space.track_private_copies(BASE, PAGE_SIZE).unwrap();
    assert!(space.write_raw(BASE, &[0]).is_err());
    assert_eq!(space.acquired_private_bytes(BASE, PAGE_SIZE), 0);
    let mut value = [1];
    space.read_raw(BASE, &mut value).unwrap();
    assert_eq!(value, [0]);
}

#[test]
fn private_copy_cpu_stores_after_cached_reads_acquire_charge_in_x86_and_aarch64() {
    use crate::isa::arm::common::cpu::ArmCpu;
    use crate::user::cpu::aarch64::{A64Exit, A64UserCpu};
    use crate::user::cpu::x86_64::{RESERVED_PHYS, X86Exit, X86UserCpu};
    const CODE: u64 = 0x4_0000;
    for compat in [false, true] {
        let space = AddressSpace::new(SpaceConfig {
            va_limit: 1 << 32,
            arena_bytes: 64 * PAGE_SIZE,
            reserved_phys: RESERVED_PHYS.to_vec(),
        })
        .unwrap();
        space.map(BASE, PAGE_SIZE, Mapping::anonymous(RW)).unwrap();
        space.track_private_copies(BASE, PAGE_SIZE).unwrap();
        space
            .map(
                CODE,
                PAGE_SIZE,
                Mapping::anonymous(Perms::READ | Perms::EXEC),
            )
            .unwrap();
        // mov eax,[ebx/rbx]; mov [ebx/rbx],eax; int3 -- identical-value store.
        space
            .write_raw(CODE, &[0x8B, 0x03, 0x89, 0x03, 0xCC])
            .unwrap();
        let mut cpu = X86UserCpu::new(&space);
        cpu.set_compat(compat);
        cpu.vcpu_mut().user_regs_mut().rip = CODE;
        cpu.vcpu_mut().user_regs_mut().rbx = BASE;
        assert!(matches!(cpu.run(), X86Exit::Event(event) if event.vector == 3));
        assert_eq!(space.acquired_private_bytes(BASE, PAGE_SIZE), PAGE_SIZE);
    }
    let space = space();
    space.map(BASE, PAGE_SIZE, Mapping::anonymous(RW)).unwrap();
    space.track_private_copies(BASE, PAGE_SIZE).unwrap();
    space
        .map(
            CODE,
            PAGE_SIZE,
            Mapping::anonymous(Perms::READ | Perms::EXEC),
        )
        .unwrap();
    // ldr x0,[x1]; str x0,[x1]; svc #0.
    let code: Vec<u8> = [0xF940_0020u32, 0xF900_0020, 0xD400_0001]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    space.write_raw(CODE, &code).unwrap();
    let mut cpu = A64UserCpu::new(&space);
    cpu.core_mut().set_pc(CODE);
    cpu.core_mut().set_x(1, BASE);
    assert!(matches!(cpu.run(10), A64Exit::Svc { .. }));
    assert_eq!(space.acquired_private_bytes(BASE, PAGE_SIZE), PAGE_SIZE);
}
