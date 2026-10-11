//! Independent installed-build port construction contracts; synthetic service identity.
use super::*;

#[path = "io_completion_create_replay_tests.rs"]
mod replay_tests;

#[cfg(windows)]
#[path = "io_completion_create_leaf_tests.rs"]
mod leaf_tests;

const SERVICE: u32 = 0x73;

fn arguments(p: &mut Proc, t: &mut Thread, args: [u64; 4]) {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            for (index, value) in [0x1234_0004, 0x1234_0000]
                .into_iter()
                .chain(args)
                .enumerate()
            {
                p.space.w32(sp + index as u64 * 4, value as u32).unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(1, 0x1234_0004);
            for (register, value) in [10, 2, 8, 9].into_iter().zip(args) {
                t.cpu.set_gpr(register, value);
            }
        }
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
    }
}
fn call(p: &mut Proc, t: &mut Thread, args: [u64; 4]) -> Outcome {
    let sp = t.cpu.sp();
    arguments(p, t, args);
    let outcome = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    };
    if outcome == Outcome::Continue {
        assert_eq!(t.cpu.pc(), 0x1234_0004);
        assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
        assert!(t.frames.is_empty());
    }
    t.cpu.set_sp(sp);
    outcome
}
fn setup(arch: WinArch) -> (super::super::super::super::WindowsProcess, Thread, u64) {
    let (mut process, t) = fixture(arch, "NtCreateIoCompletion", SERVICE);
    let p = process.state_mut();
    let scratch =
        p.vm.allocate(
            None,
            4 * PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    p.space
        .wr(scratch, &vec![0xA5; (4 * PAGE_SIZE) as usize])
        .unwrap();
    (process, t, scratch)
}

fn attrs(p: &Proc, address: u64, length: u32, flags: u32, root: u64) {
    let bytes = if p.arch.is64() { 48 } else { 24 };
    p.space.wr(address, &vec![0; bytes]).unwrap();
    p.space.w32(address, length).unwrap();
    let offset = if p.arch.is64() { 8 } else { 4 };
    p.space
        .wptr(address + offset, p.arch.ptr_size(), root)
        .unwrap();
    p.space
        .w32(address + offset + 2 * p.arch.ptr_size(), flags)
        .unwrap();
}

#[test]
fn native_io_completion_create_preserves_unsigned_concurrency_and_guest_default_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        assert_eq!(
            p.space
                .u32(p.peb + crate::user::windows::layout::offsets(arch).peb_number_of_processors)
                .unwrap(),
            1
        );
        for count in [0, 1, 2, 4, 8, 16, 0x7FFF_FFFF, 0x8000_0000, u32::MAX] {
            assert_eq!(
                call(p, &mut t, [out, 0x1F0003, 0, count.into()]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), 0);
            let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
            assert!(
                matches!(p.objects.get(handle), Some(Object::CompletionPort{queue, concurrency}) if queue.is_empty() && *concurrency == count.max(1))
            );
            p.objects.close(handle).unwrap();
        }
    }
}

#[test]
fn native_io_completion_create_maps_grants_and_inheritance_all_abis() {
    let grants = [
        (0, 0),
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 0),
        (0xF0000, 0xF0000),
        (0x100000, 0x100000),
        (0x1F0003, 0x1F0003),
        (0x1F0007, 0x1F0003),
        (0x2000000, 0x1F0003),
        (0x80000000, 0x20001),
        (0x40000000, 0x20002),
        (0x20000000, 0x120000),
        (0x10000000, 0x1F0003),
    ];
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let oa = out + PAGE_SIZE;
        for (access, grant) in grants {
            for flags in [0, 2] {
                attrs(p, oa, if arch.is64() { 48 } else { 24 }, flags, 0);
                assert_eq!(call(p, &mut t, [out, access, oa, 4]), Outcome::Continue);
                assert_eq!(t.cpu.gpr(0), 0);
                let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
                assert_eq!(p.objects.access(handle), Some(grant));
                assert_eq!(p.objects.flags(handle), Some(u32::from(flags != 0)));
                p.objects.close(handle).unwrap();
            }
        }
    }
}

#[test]
fn native_io_completion_create_keeps_named_security_and_privilege_boundaries_explicit_all_abis() {
    for arch in WinArch::ALL {
        for role in 0..4 {
            let (mut process, mut t, out) = setup(arch);
            let p = process.state_mut();
            let oa = out + PAGE_SIZE;
            attrs(p, oa, if arch.is64() { 48 } else { 24 }, 0, 0);
            if role < 3 {
                let offset = match role {
                    0 => 2 * arch.ptr_size(),
                    1 => {
                        if arch.is64() {
                            32
                        } else {
                            16
                        }
                    }
                    _ => {
                        if arch.is64() {
                            40
                        } else {
                            20
                        }
                    }
                };
                p.space
                    .wptr(oa + offset, arch.ptr_size(), out + 2 * PAGE_SIZE)
                    .unwrap();
            }
            let handles = p.objects.handle_count();
            let objects = p.objects.iter().count();
            let access = if role == 3 { 0x01000000 } else { 0x1F0003 };
            assert!(
                matches!(call(p, &mut t, [out, access, oa, 4]), Outcome::Fail(reason) if reason.contains("I/O completion"))
            );
            assert_eq!(p.objects.handle_count(), handles);
            assert_eq!(p.objects.iter().count(), objects);
            assert_eq!(p.space.bytes(out, 16).unwrap(), [0xA5; 16]);
        }
    }
}

#[test]
fn native_io_completion_create_constructs_real_object_and_closes_without_leak_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let handles = p.objects.handle_count();
        let objects = p.objects.iter().count();
        assert_eq!(call(p, &mut t, [out, 0x1F0003, 0, 4]), Outcome::Continue);
        assert_eq!(t.cpu.gpr(0), 0);
        let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
        assert!(
            matches!(p.objects.get(handle),Some(Object::CompletionPort{queue,..}) if queue.is_empty())
        );
        assert_eq!(p.objects.access(handle), Some(0x1F0003));
        assert_eq!(p.objects.flags(handle), Some(0));
        assert_eq!(p.objects.handle_count(), handles + 1);
        assert_eq!(p.objects.iter().count(), objects + 1);
        assert_eq!(
            p.space
                .bytes(out + arch.ptr_size(), 16 - arch.ptr_size() as usize)
                .unwrap(),
            vec![0xA5; 16 - arch.ptr_size() as usize]
        );
        assert!(p.objects.close(handle).is_ok());
        assert_eq!(p.objects.handle_count(), handles);
        assert_eq!(p.objects.iter().count(), objects);
    }
}
