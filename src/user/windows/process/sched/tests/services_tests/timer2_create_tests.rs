//! Installed-build Timer2 construction, independent capture replay and leaves.
use super::*;

#[path = "timer2_create_replay_tests.rs"]
mod replay_tests;

#[cfg(windows)]
#[path = "timer2_create_leaf_tests.rs"]
mod leaf_tests;

const SERVICE: u32 = 0x75;
fn arguments(p: &mut Proc, t: &mut Thread, args: [u64; 5]) {
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
            p.space.w64(sp + 40, args[4]).unwrap();
        }
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
    }
}
fn call(p: &mut Proc, t: &mut Thread, args: [u64; 5]) -> Outcome {
    let sp = t.cpu.sp();
    arguments(p, t, args);
    let result = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    };
    if result == Outcome::Continue {
        assert_eq!(t.cpu.pc(), 0x1234_0004);
        assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
        assert!(t.frames.is_empty());
    }
    t.cpu.set_sp(sp);
    result
}
fn setup(arch: WinArch) -> (super::super::super::super::WindowsProcess, Thread, u64) {
    let (mut process, t) = fixture(arch, "NtCreateTimer2", SERVICE);
    let p = process.state_mut();
    let out =
        p.vm.allocate(
            None,
            4 * PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    p.space
        .wr(out, &vec![0xA5; (4 * PAGE_SIZE) as usize])
        .unwrap();
    (process, t, out)
}

#[test]
fn native_timer2_create_constructs_real_object_and_closes_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let before_handles = p.objects.handle_count();
        let before_objects = p.objects.iter().count();
        for flags in [0, 4, 8, 12, 0x80000000, 0x80000008] {
            assert_eq!(
                call(p, &mut t, [out, 0, 0, flags, 0x100002]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), 0);
            let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
            assert_eq!(p.objects.get(handle).unwrap().type_name(), "IRTimer");
            assert_eq!(p.objects.access(handle), Some(0x100002));
            assert_eq!(p.objects.handle_count(), before_handles + 1);
            assert_eq!(p.objects.iter().count(), before_objects + 1);
            assert_eq!(
                p.space
                    .bytes(out + arch.ptr_size(), 16 - arch.ptr_size() as usize)
                    .unwrap(),
                vec![0xA5; 16 - arch.ptr_size() as usize]
            );
            p.objects.close(handle).unwrap();
            assert_eq!(p.objects.handle_count(), before_handles);
            assert_eq!(p.objects.iter().count(), before_objects);
        }
    }
}

#[test]
fn native_timer2_create_duplicate_retains_dormant_waitable_object_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let before = p.objects.iter().count();
        for flags in [8, 0x80000008] {
            assert_eq!(
                call(p, &mut t, [out, 0, 0, flags, 0x1F0003]),
                Outcome::Continue
            );
            assert_eq!(t.cpu.gpr(0), 0);
            let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
            let id = p.objects.id(handle).unwrap();
            let duplicate = u64::from(p.objects.duplicate(handle, false).unwrap());
            assert_eq!(p.objects.id(duplicate), Some(id));
            assert_eq!(p.objects.access(duplicate), Some(0x1F0003));
            assert_eq!(
                crate::user::windows::sync::try_objects(p, t.tid, &[id], false),
                Ok(None)
            );
            assert!(p.objects.close(handle).unwrap().is_none());
            assert!(matches!(
                p.objects.get(duplicate),
                Some(Object::IrTimer {
                    signaled: false,
                    ..
                })
            ));
            assert!(p.objects.close(duplicate).unwrap().is_some());
            assert_eq!(p.objects.iter().count(), before);
        }
    }
}

#[test]
fn native_timer2_create_privilege_policy_fails_without_output_or_object_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let before = p.objects.iter().count();
        let outcome = call(p, &mut t, [out, 0, 0, 8, 0x01000000]);
        assert!(
            matches!(outcome, Outcome::Fail(ref error) if error.contains("ACCESS_SYSTEM_SECURITY"))
        );
        assert_eq!(p.space.bytes(out, 16).unwrap(), vec![0xA5; 16]);
        assert_eq!(p.objects.iter().count(), before);
    }
}
