use super::*;

fn select_query(p: &mut Proc, arch: WinArch) {
    let index = p.modules.nt_services.as_ref().unwrap().0;
    p.modules.win32u_services = Some((index, named_table(arch, "NtUserGetThreadDesktop", 0x107b)));
}

fn handle(p: &Proc) -> u64 {
    let Availability::Available(inputs) = &p.native.as_ref().unwrap().gui else {
        unreachable!()
    };
    u64::from(p.objects.desktop_handle(&inputs.assigned_desktop).unwrap())
}

#[test]
fn thread_desktop_cold_callback_uses_borrowed_handle_and_restores_failed_conversion() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        for status in [0, 0x103, 0x8000_0001, 0xc000_0001] {
            let (mut process, mut t, entry) = setup(arch);
            let p = process.state_mut();
            select_query(p, arch);
            let expected = handle(p);
            let count = p.objects.handle_count();
            let resume = entry + 0x4000;
            t.cpu.set_pc(resume);
            let sp = t.cpu.sp();
            let tid = t.tid;
            arguments(p, &mut t, [u64::from(tid), 0, 0]);
            assert_eq!(
                super::super::super::super::super::services::dispatch(
                    p,
                    &mut t,
                    0x107b,
                    resume - 4
                ),
                Some(Outcome::Continue)
            );
            assert_eq!(t.cpu.pc(), entry);
            assert_eq!(
                p.gui.threads[&tid].callbacks[0].completion,
                Completion::Value(expected)
            );
            arguments(p, &mut t, [1, u64::from(u32::MAX), status]);
            assert_eq!(
                super::super::super::super::super::services::dispatch(p, &mut t, 5, entry + 0x100),
                Some(Outcome::Continue)
            );
            let success = status as u32 as i32 >= 0;
            assert_eq!(t.cpu.gpr(0), if success { expected } else { 0 });
            assert_eq!(t.cpu.pc(), resume);
            assert_eq!(t.cpu.sp(), sp);
            assert_eq!(
                p.space.u64(t.teb + 0x78).unwrap(),
                if success { u64::from(tid) } else { 0 }
            );
            assert_eq!(p.objects.handle_count(), count);
        }
    }
}

#[test]
fn thread_desktop_cids_follow_retained_thread_object_lifetime_and_preserve_errors() {
    use crate::user::windows::objects::Object;
    for arch in [WinArch::Arm64, WinArch::X64] {
        let (mut process, mut t, entry) = setup(arch);
        let p = process.state_mut();
        assert_eq!(
            super::super::super::super::super::services::dispatch(p, &mut t, 0x12fd, entry + 4),
            Some(Outcome::Continue)
        );
        select_query(p, arch);
        let expected = handle(p);
        let other = p.objects.create(Object::Thread {
            tid: 12,
            exit_code: None,
        });
        let retained = p.objects.open(other, false);
        let offset = crate::user::windows::layout::offsets(arch);
        p.space
            .w32(t.teb + offset.teb_last_error, 0x1234_5678)
            .unwrap();
        p.space
            .w32(t.teb + offset.teb_last_status, 0xc123_abcd)
            .unwrap();
        for tid in [t.tid, t.tid | 1, t.tid | 2, t.tid | 3, 12, 13, 14, 15] {
            arguments(p, &mut t, [u64::from(tid), 0, 0]);
            assert_eq!(
                super::super::super::super::super::services::dispatch(p, &mut t, 0x107b, entry + 4),
                Some(Outcome::Continue)
            );
            assert_eq!(t.cpu.gpr(0), expected);
        }
        if let Some(Object::Thread { exit_code, .. }) = p.objects.obj_mut(other) {
            *exit_code = Some(0);
        }
        arguments(p, &mut t, [12, 0, 0]);
        assert_eq!(
            super::super::super::super::super::services::dispatch(p, &mut t, 0x107b, entry + 4),
            Some(Outcome::Continue)
        );
        assert_eq!(t.cpu.gpr(0), expected);
        p.objects.close(u64::from(retained)).unwrap();
        for tid in [0, 1, 4, 12, 15, 0xdead_beef, u32::MAX] {
            arguments(p, &mut t, [u64::from(tid), 0, 0]);
            assert_eq!(
                super::super::super::super::super::services::dispatch(p, &mut t, 0x107b, entry + 4),
                Some(Outcome::Continue)
            );
            assert_eq!(t.cpu.gpr(0), 0);
        }
        assert_eq!(
            p.space.u32(t.teb + offset.teb_last_error).unwrap(),
            0x1234_5678
        );
        assert_eq!(
            p.space.u32(t.teb + offset.teb_last_status).unwrap(),
            0xc123_abcd
        );
        assert_eq!(p.gui.threads[&t.tid].callbacks.len(), 1);
    }
}

#[test]
fn thread_desktop_native_close_denies_attachment_without_a_protect_flag() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        let (mut process, mut t, entry) = setup(arch);
        let p = process.state_mut();
        let assigned = handle(p);
        let index = p.modules.nt_services.as_ref().unwrap().0;
        p.modules.nt_services = Some((index, named_table(arch, "NtClose", 0x37)));
        assert_eq!(p.objects.flags(assigned), Some(0));
        let error = t.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
        p.space.w32(error, 0x1234_5678).unwrap();
        arguments(p, &mut t, [assigned, 0, 0]);
        assert_eq!(
            super::super::super::super::super::services::dispatch(p, &mut t, 0x37, entry + 4),
            Some(Outcome::Continue)
        );
        assert_eq!(t.cpu.gpr(0), 0xc000_0235);
        assert_eq!(p.objects.flags(assigned), Some(0));
        assert_eq!(p.space.u32(error).unwrap(), 0x1234_5678);
        let copy = p.objects.duplicate(assigned, false).unwrap();
        arguments(p, &mut t, [u64::from(copy), 0, 0]);
        assert_eq!(
            super::super::super::super::super::services::dispatch(p, &mut t, 0x37, entry + 4),
            Some(Outcome::Continue)
        );
        assert_eq!(t.cpu.gpr(0), 0);
        assert!(p.objects.get(assigned).is_some());
        assert!(p.objects.get(u64::from(copy)).is_none());
    }
}
