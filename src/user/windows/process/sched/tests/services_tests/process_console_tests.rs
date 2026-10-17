//! Guest-owned process owner state and independently measured native adapters.
use super::*;
use crate::user::windows::nt::status::*;

#[path = "process_console_replay_tests.rs"]
mod replay;

#[cfg(windows)]
#[path = "process_console_leaf_tests.rs"]
mod installed;

fn setup(arch: WinArch) -> (super::super::super::super::WindowsProcess, Thread, u64) {
    let (mut process, thread) = fixture(arch, "NtSetInformationProcess", 0x1c);
    let base = process
        .state_mut()
        .vm
        .allocate(
            None,
            PAGE_SIZE * 6,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    (process, thread, base)
}

fn invoke(p: &mut Proc, t: &mut Thread, setting: bool, args: &[u64]) -> u32 {
    let (name, number) = if setting {
        ("NtSetInformationProcess", 0x1c)
    } else {
        ("NtQueryInformationProcess", 0x19)
    };
    let module = p.modules.by_name("ntdll.dll").unwrap();
    p.modules.nt_services = Some((module, table(p.arch, name, number)));
    super::nls_tests::arguments(p, t, args);
    t.cpu.set_gpr(0, u64::from(number));
    // ARM64 passes argument zero in X0 rather than a syscall-number register.
    if p.arch == WinArch::Arm64 {
        t.cpu.set_gpr(0, args[0]);
    }
    let sp = t.cpu.sp();
    let outcome = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, number))
    };
    assert_eq!(outcome, Outcome::Continue, "{name}/{}", p.arch);
    assert!(t.frames.is_empty());
    assert_eq!(t.cpu.pc(), 0x1234_0004);
    // This entry starts inside the CALL to the WoW transition. Its return
    // address is popped here; the installed NTDLL RET imm16 runs separately.
    assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
    t.cpu.set_sp(sp);
    t.cpu.gpr(0) as u32
}

#[test]
fn native_console_owner_wow_null_passthrough_and_nonnull_capture_keep_distinct_priority() {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.vm.protect(base, PAGE_SIZE, prot::NOACCESS).unwrap();
        for (handle, input, expected) in [
            (0, 0, STATUS_INVALID_PARAMETER),
            (arch.ptr(u64::MAX), 0, STATUS_ACCESS_VIOLATION),
            (
                0,
                1,
                if arch == WinArch::X86 {
                    STATUS_ACCESS_VIOLATION
                } else {
                    STATUS_DATATYPE_MISALIGNMENT
                },
            ),
            (
                0,
                base,
                if arch == WinArch::X86 {
                    STATUS_ACCESS_VIOLATION
                } else {
                    STATUS_INVALID_PARAMETER
                },
            ),
        ] {
            assert_eq!(
                invoke(p, &mut t, true, &[handle, 49, input, arch.ptr_size()]),
                expected,
                "{arch}/{handle:#x}/{input:#x}"
            );
            assert_eq!(p.console_host_process, 2);
        }
    }
}

#[test]
fn native_console_owner_tagged_values_are_process_local_without_objects_or_entropy() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let (other, _, _) = setup(arch);
        let p = process.state_mut();
        assert_eq!(p.console_host_process, 2);
        let objects = p.objects.iter().count();
        let handles = p.objects.handle_count();
        let committed = p.vm.committed_bytes();
        let rng = p.rng;
        for value in [1, 5, 0x1234_5679, 0xFFFF_FFFD, 0x1_0000_0001, u64::MAX - 2] {
            p.space.wptr(base, arch.ptr_size(), value).unwrap();
            let args = [arch.ptr(u64::MAX), 49, base, arch.ptr_size()];
            assert_eq!(invoke(p, &mut thread, true, &args), STATUS_SUCCESS);
            assert_eq!(p.console_host_process, arch.ptr(value));
            assert_eq!(p.space.ptr(base, arch.ptr_size()).unwrap(), arch.ptr(value));
            p.space.wr(base + 64, &[0xA5; 32]).unwrap();
            let status = invoke(
                p,
                &mut thread,
                false,
                &[
                    arch.ptr(u64::MAX),
                    49,
                    base + 64,
                    arch.ptr_size(),
                    base + 88,
                ],
            );
            if arch == WinArch::X86 {
                assert_eq!(status, STATUS_INVALID_INFO_CLASS);
                assert_eq!(p.space.bytes(base + 64, 32).unwrap(), [0xA5; 32]);
            } else {
                assert_eq!(status, STATUS_SUCCESS);
                assert_eq!(p.space.u64(base + 64).unwrap(), value);
                assert_eq!(p.space.u32(base + 88).unwrap(), 8);
                assert_eq!(p.space.bytes(base + 72, 16).unwrap(), [0xA5; 16]);
            }
            assert_eq!(other.state().console_host_process, 2);
            assert_eq!(p.objects.iter().count(), objects);
            assert_eq!(p.objects.handle_count(), handles);
            assert_eq!(p.vm.committed_bytes(), committed);
            assert_eq!(p.rng, rng);
        }
    }
}

#[test]
fn native_console_owner_rejected_tags_leave_prior_state_unchanged_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        for value in [0, 2, 3, 4, u64::MAX, u64::MAX - 1] {
            p.space.wptr(base, arch.ptr_size(), value).unwrap();
            assert_eq!(
                invoke(
                    p,
                    &mut thread,
                    true,
                    &[arch.ptr(u64::MAX), 49, base, arch.ptr_size()]
                ),
                STATUS_INVALID_PARAMETER
            );
            assert_eq!(p.console_host_process, 2);
        }
    }
}

#[test]
fn native_console_owner_query_unmodeled_peer_remains_explicitly_unsupported() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        let id = p.objects.create(Object::Process {
            pid: p.pid + 4,
            exit_code: None,
        });
        let h = p.objects.open_access(id, false, 0x1000).unwrap();
        let module = p.modules.by_name("ntdll.dll").unwrap();
        p.modules.nt_services = Some((module, table(arch, "NtQueryInformationProcess", 0x19)));
        super::nls_tests::arguments(p, &mut t, &[h.into(), 49, base, 8, 0]);
        if arch == WinArch::X64 {
            t.cpu.set_gpr(0, 0x19);
        }
        assert!(
            matches!(handle_stop(p, &mut t, stop(arch, 0x19)), Outcome::Fail(reason) if reason.contains("ProcessConsoleHostProcess for an unmodeled peer"))
        );
        assert_eq!(p.console_host_process, 2);
        assert!(p.objects.close(h.into()).unwrap().is_some());
    }
}
