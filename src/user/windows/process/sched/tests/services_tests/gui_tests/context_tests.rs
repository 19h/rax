use super::super::super::super::super::services::dispatch;
use super::*;
use crate::user::windows::memory::prot;
use crate::user::windows::objects::Object;

const QUERY: u32 = 0x147f;
const ERROR_SENTINEL: u32 = 0x1234_5678;
const STATUS_SENTINEL: u32 = 0xc123_abcd;
const POISON: u64 = 0xa5a5_a5a5_a5a5_a5a5;

fn select(p: &mut Proc, arch: WinArch) {
    let index = p.modules.nt_services.as_ref().unwrap().0;
    p.modules.win32u_services = Some((
        index,
        named_table(arch, "NtUserGetProcessUIContextInformation", QUERY),
    ));
    let Availability::Available(inputs) = &mut p.native.as_mut().unwrap().gui else {
        unreachable!();
    };
    std::sync::Arc::get_mut(inputs).unwrap().process_ui_context = Ok([2, 3]);
}

fn sentinels(p: &Proc, t: &Thread) {
    let offsets = crate::user::windows::layout::offsets(p.arch);
    p.space
        .w32(t.teb + offsets.teb_last_error, ERROR_SENTINEL)
        .unwrap();
    p.space
        .w32(t.teb + offsets.teb_last_status, STATUS_SENTINEL)
        .unwrap();
}

fn errors(p: &Proc, t: &Thread) -> (u32, u32) {
    let offsets = crate::user::windows::layout::offsets(p.arch);
    (
        p.space.u32(t.teb + offsets.teb_last_error).unwrap(),
        p.space.u32(t.teb + offsets.teb_last_status).unwrap(),
    )
}

fn converted(p: &mut Proc, t: &mut Thread, entry: u64) {
    select(p, p.arch);
    let index = p.modules.nt_services.as_ref().unwrap().0;
    p.modules.win32u_services = Some((index, named_table(p.arch, "NtGdiInit2", 0x12fd)));
    assert_eq!(dispatch(p, t, 0x12fd, entry + 4), Some(Outcome::Continue));
    arguments(p, t, [1, u64::from(u32::MAX), 0]);
    assert_eq!(dispatch(p, t, 5, entry + 8), Some(Outcome::Continue));
    p.modules.win32u_services = Some((
        index,
        named_table(p.arch, "NtUserGetProcessUIContextInformation", QUERY),
    ));
}

#[test]
fn process_ui_context_cold_completion_preserves_arguments_and_callback_failure() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        for status in [0, 0x103, 0x8000_0001, 0xc000_0001] {
            let (mut process, mut t, entry) = setup(arch);
            let p = process.state_mut();
            select(p, arch);
            let output = entry + 0x5000;
            p.space.wr(output, &[0xa5; 16]).unwrap();
            sentinels(p, &t);
            let resume = entry + 0x4000;
            t.cpu.set_pc(resume);
            let sp = t.cpu.sp();
            arguments(p, &mut t, [u64::MAX, output, 0]);
            assert_eq!(
                dispatch(p, &mut t, QUERY, resume - 4),
                Some(Outcome::Continue)
            );
            assert_eq!(p.space.u64(output).unwrap(), POISON);
            assert_eq!(
                p.gui.threads[&t.tid].callbacks[0].completion,
                Completion::ProcessUiContext {
                    process: u64::MAX,
                    output
                }
            );
            arguments(p, &mut t, [1, u64::from(u32::MAX), status]);
            assert_eq!(dispatch(p, &mut t, 5, entry + 8), Some(Outcome::Continue));
            let success = status as u32 as i32 >= 0;
            assert_eq!(t.cpu.gpr(0), u64::from(success));
            assert_eq!(
                p.space.u64(output).unwrap(),
                if success {
                    0x0000_0003_0000_0002
                } else {
                    POISON
                }
            );
            assert_eq!(p.space.u64(output + 8).unwrap(), POISON);
            assert_eq!(t.cpu.pc(), resume);
            assert_eq!(t.cpu.sp(), sp);
            assert_eq!(errors(p, &t), (ERROR_SENTINEL, STATUS_SENTINEL));
            assert_eq!(p.gui.threads[&t.tid].initialized, success);
        }
    }
}

#[test]
fn process_ui_context_converts_before_invalid_handle_and_checks_handle_before_guard() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        for handle in [0, u64::MAX - 1, 0x1234] {
            let (mut process, mut t, entry) = setup(arch);
            let p = process.state_mut();
            select(p, arch);
            let output = entry + 0x5000;
            p.space.w64(output, POISON).unwrap();
            p.vm.protect(output, 4096, prot::READWRITE | prot::GUARD)
                .unwrap();
            sentinels(p, &t);
            arguments(p, &mut t, [handle, output, 0]);
            assert_eq!(
                dispatch(p, &mut t, QUERY, entry + 4),
                Some(Outcome::Continue)
            );
            assert_eq!(p.gui.threads[&t.tid].callbacks.len(), 1);
            assert_eq!(errors(p, &t), (ERROR_SENTINEL, STATUS_SENTINEL));
            arguments(p, &mut t, [0, 0, 0]);
            assert_eq!(dispatch(p, &mut t, 5, entry + 8), Some(Outcome::Continue));
            assert_eq!(t.cpu.gpr(0), 0);
            assert_eq!(errors(p, &t), (87, STATUS_SENTINEL));
            assert!(p.gui.threads[&t.tid].initialized);
            assert_ne!(p.vm.query(output).unwrap().protect & prot::GUARD, 0);
            p.vm.protect(output, 4096, prot::READWRITE).unwrap();
            assert_eq!(p.space.u64(output).unwrap(), POISON);
        }
    }
}

#[test]
fn process_ui_context_writes_exactly_eight_unaligned_bytes_for_virtual_current_process() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        let (mut process, mut t, entry) = setup(arch);
        let p = process.state_mut();
        converted(p, &mut t, entry);
        let object = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        let limited = u64::from(p.objects.open_access(object, false, 0x1000).unwrap());
        let full = u64::from(p.objects.open_access(object, false, 0x1400).unwrap());
        for handle in [u64::MAX, limited, limited | 3, full] {
            for offset in 0..8 {
                let base = entry + 0x5000;
                p.space.wr(base, &[0xa5; 32]).unwrap();
                sentinels(p, &t);
                arguments(p, &mut t, [handle, base + offset, 0]);
                assert_eq!(
                    dispatch(p, &mut t, QUERY, entry + 4),
                    Some(Outcome::Continue)
                );
                assert_eq!(t.cpu.gpr(0), 1);
                let mut expected = [0xa5; 32];
                expected[offset as usize..offset as usize + 8]
                    .copy_from_slice(&[2, 0, 0, 0, 3, 0, 0, 0]);
                let mut actual = [0; 32];
                p.space.rd(base, &mut actual).unwrap();
                assert_eq!(actual, expected);
                assert_eq!(errors(p, &t), (ERROR_SENTINEL, STATUS_SENTINEL));
            }
        }
    }
}

#[test]
fn process_ui_context_faults_consume_guards_and_preserve_crossing_prefix() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        for protection in [
            prot::READONLY,
            prot::NOACCESS,
            prot::READWRITE | prot::GUARD,
        ] {
            let (mut process, mut t, entry) = setup(arch);
            let p = process.state_mut();
            converted(p, &mut t, entry);
            let output = entry + 0x5ffc;
            p.space.w64(output, POISON).unwrap();
            p.vm.protect(entry + 0x6000, 4096, protection).unwrap();
            sentinels(p, &t);
            arguments(p, &mut t, [u64::MAX, output, 0]);
            assert_eq!(
                dispatch(p, &mut t, QUERY, entry + 4),
                Some(Outcome::Continue)
            );
            assert_eq!(t.cpu.gpr(0), 0);
            assert_eq!(p.space.u32(output).unwrap(), 0xa5a5_a5a5);
            let guard = protection & prot::GUARD != 0;
            assert_eq!(
                errors(p, &t),
                if guard {
                    (0x8000_0001, 0x8000_0001)
                } else {
                    (998, 0xc000_0005)
                }
            );
            assert_eq!(p.vm.query(entry + 0x6000).unwrap().protect & prot::GUARD, 0);
            p.vm.protect(entry + 0x6000, 4096, prot::READWRITE).unwrap();
            assert_eq!(p.space.u64(output).unwrap(), POISON);
            if guard {
                arguments(p, &mut t, [u64::MAX, output, 0]);
                assert_eq!(
                    dispatch(p, &mut t, QUERY, entry + 4),
                    Some(Outcome::Continue)
                );
                assert_eq!(t.cpu.gpr(0), 1);
                assert_eq!(p.space.u64(output).unwrap(), 0x0000_0003_0000_0002);
            }
        }
    }
}

#[test]
fn process_ui_context_rejects_wrong_object_or_grant_without_touching_output() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        let (mut process, mut t, entry) = setup(arch);
        let p = process.state_mut();
        converted(p, &mut t, entry);
        let object = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        let denied = u64::from(p.objects.open_access(object, false, 0x100000).unwrap());
        let Availability::Available(inputs) = &p.native.as_ref().unwrap().gui else {
            unreachable!()
        };
        let desktop = u64::from(p.objects.desktop_handle(&inputs.assigned_desktop).unwrap());
        for handle in [denied, desktop] {
            sentinels(p, &t);
            arguments(p, &mut t, [handle, 1, 0]);
            assert_eq!(
                dispatch(p, &mut t, QUERY, entry + 4),
                Some(Outcome::Continue)
            );
            assert_eq!(t.cpu.gpr(0), 0);
            assert_eq!(errors(p, &t), (87, STATUS_SENTINEL));
        }
    }
}

#[test]
fn process_ui_context_missing_subject_or_metadata_reports_unavailable_without_fabrication() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        for missing_subject in [false, true] {
            let (mut process, mut t, entry) = setup(arch);
            let p = process.state_mut();
            if !missing_subject {
                let Availability::Available(inputs) = &mut p.native.as_mut().unwrap().gui else {
                    unreachable!()
                };
                std::sync::Arc::get_mut(inputs).unwrap().process_ui_context =
                    Err("controlled missing metadata".into());
            }
            // Unrelated conversion remains available despite missing query metadata.
            assert_eq!(
                dispatch(p, &mut t, 0x12fd, entry + 4),
                Some(Outcome::Continue)
            );
            arguments(p, &mut t, [0, 0, 0]);
            assert_eq!(dispatch(p, &mut t, 5, entry + 8), Some(Outcome::Continue));
            assert_ne!(t.cpu.gpr(0), 0);
            let index = p.modules.nt_services.as_ref().unwrap().0;
            p.modules.win32u_services = Some((
                index,
                named_table(arch, "NtUserGetProcessUIContextInformation", QUERY),
            ));
            let handle = if missing_subject {
                let object = p.objects.create(Object::Process {
                    pid: p.pid + 4,
                    exit_code: None,
                });
                u64::from(p.objects.open_access(object, false, 0x1000).unwrap())
            } else {
                u64::MAX
            };
            let output = entry + 0x5000;
            p.space.w64(output, POISON).unwrap();
            sentinels(p, &t);
            arguments(p, &mut t, [handle, output, 0]);
            assert!(
                matches!(dispatch(p, &mut t, QUERY, entry + 4), Some(Outcome::Fail(reason)) if reason.contains(if missing_subject { "another virtual process" } else { "controlled missing metadata" }))
            );
            assert_eq!(p.space.u64(output).unwrap(), POISON);
            assert_eq!(errors(p, &t), (ERROR_SENTINEL, STATUS_SENTINEL));
        }
    }
}
