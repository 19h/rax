//! Set/cancel semantics use native captures rather than a successful no-op.
use super::event_tests::{handle, setup, status};
use super::*;
use crate::user::windows::nt::status::*;

fn create(p: &mut Proc, t: &mut Thread, out: u64, attributes: u64, access: u64) -> u64 {
    p.space.w32(out + 32, 1).unwrap();
    assert_eq!(
        status(
            p,
            t,
            "NtCreateTimer2",
            &[
                out,
                if attributes & 2 != 0 { out + 32 } else { 0 },
                0,
                attributes,
                access
            ]
        ),
        STATUS_SUCCESS
    );
    handle(p, out)
}
fn inputs(p: &Proc, base: u64, due: i64, period: i64, version: u32, reserved: u32, tolerance: i64) {
    p.space.w64(base, due as u64).unwrap();
    p.space.w64(base + 8, period as u64).unwrap();
    p.space.w32(base + 16, version).unwrap();
    p.space.w32(base + 20, reserved).unwrap();
    p.space.w64(base + 24, tolerance as u64).unwrap();
}

#[test]
fn native_timer2_arm_observed_request_and_cancel_are_real_kernel_services_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, out) = setup(arch);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        let timer = create(p, &mut thread, out, 8, 0x1f0003);
        let data = out + 128;
        inputs(p, data, -3_000_000_000, 0, 0, 0, 750_000_000);
        let before = p.space.bytes(data, 32).unwrap();
        assert_eq!(
            status(p, &mut thread, "NtSetTimer2", &[timer, data, 0, data + 16]),
            STATUS_SUCCESS
        );
        assert_eq!(p.space.bytes(data, 32).unwrap(), before);
        assert!(matches!(
            p.objects.get(timer),
            Some(Object::IrTimer {
                attributes: 8,
                signaled: false,
                ..
            })
        ));
        assert_eq!(
            status(
                p,
                &mut thread,
                "NtWaitForSingleObject",
                &[timer, 0, data + 8]
            ),
            STATUS_TIMEOUT
        );
        for parameters in [0, 1, 4, arch.ptr(u64::MAX)] {
            assert_eq!(
                status(p, &mut thread, "NtCancelTimer2", &[timer, parameters]),
                STATUS_SUCCESS
            );
        }
        assert_eq!(status(p, &mut thread, "NtClose", &[timer]), STATUS_SUCCESS);
        assert_eq!(
            (p.objects.handle_count(), p.objects.iter().count()),
            baseline
        );
    }
}

#[test]
fn native_timer2_arm_rejected_values_and_cancel_preserve_notification_signal_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, out) = setup(arch);
        let p = process.state_mut();
        let timer = create(p, &mut thread, out, 0x8000_0008, 0x1f0003);
        let data = out + 128;
        for (version, tolerance, expected) in [
            (1, 0, STATUS_INVALID_PARAMETER_3),
            (u32::MAX, 0, STATUS_INVALID_PARAMETER_3),
            (0, -2, STATUS_INVALID_PARAMETER_3),
            (0, i64::MIN, STATUS_INVALID_PARAMETER_3),
        ] {
            if let Some(Object::IrTimer { signaled, .. }) = p.objects.get_mut(timer) {
                *signaled = true;
            }
            inputs(p, data, -3_000_000_000, 0, version, 0, tolerance);
            assert_eq!(
                status(p, &mut thread, "NtSetTimer2", &[timer, data, 0, data + 16]),
                expected
            );
            assert!(matches!(
                p.objects.get(timer),
                Some(Object::IrTimer { signaled: true, .. })
            ));
            assert_eq!(
                status(p, &mut thread, "NtCancelTimer2", &[timer, 1]),
                STATUS_SUCCESS
            );
            assert!(matches!(
                p.objects.get(timer),
                Some(Object::IrTimer { signaled: true, .. })
            ));
        }
        inputs(p, data, -3_000_000_000, 0, 0, u32::MAX, -1);
        assert_eq!(
            status(p, &mut thread, "NtSetTimer2", &[timer, data, 0, data + 16]),
            STATUS_SUCCESS
        );
        assert!(matches!(
            p.objects.get(timer),
            Some(Object::IrTimer {
                signaled: false,
                ..
            })
        ));
        p.objects.close(timer).unwrap();
    }
}

#[test]
fn native_timer2_arm_access_type_and_validation_order_match_native_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, out) = setup(arch);
        let p = process.state_mut();
        let data = out + 128;
        inputs(p, data, -3_000_000_000, 0, 1, 0, 0);
        for access in [0, 1, 2, 3, 0x100002, 0x1f0003] {
            let timer = create(p, &mut thread, out, 8, access);
            let expected = if access & 2 == 0 {
                STATUS_ACCESS_DENIED
            } else {
                STATUS_INVALID_PARAMETER_3
            };
            assert_eq!(
                status(p, &mut thread, "NtSetTimer2", &[timer, data, 0, data + 16]),
                expected
            );
            assert_eq!(
                status(p, &mut thread, "NtCancelTimer2", &[timer, 1]),
                if access & 2 == 0 {
                    STATUS_ACCESS_DENIED
                } else {
                    STATUS_SUCCESS
                }
            );
            p.objects.close(timer).unwrap();
        }
        for (timer, expected) in [
            (0, STATUS_INVALID_HANDLE),
            (0x7fff_ffff, STATUS_INVALID_HANDLE),
            (arch.ptr(u64::MAX), STATUS_OBJECT_TYPE_MISMATCH),
            (arch.ptr(u64::MAX - 1), STATUS_OBJECT_TYPE_MISMATCH),
        ] {
            assert_eq!(
                status(p, &mut thread, "NtSetTimer2", &[timer, data, 0, data + 16]),
                expected
            );
            assert_eq!(
                status(p, &mut thread, "NtCancelTimer2", &[timer, 1]),
                expected
            );
        }
    }
}

#[test]
fn native_timer2_arm_period_and_due_boundaries_are_captured_without_narrowing_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, out) = setup(arch);
        let p = process.state_mut();
        let data = out + 128;
        for attributes in [8, 4, 0x80000008, 2] {
            let timer = create(p, &mut thread, out, attributes, 0x1f0003);
            for due in [i64::MIN, -3_000_000_000, -1, 0, 1, i64::MAX] {
                for period in [i64::MIN, -1, 0, 1, 10000, i64::MAX] {
                    inputs(p, data, due, period, 0, 0, 0);
                    let expected = if attributes & 2 != 0 && period != 0 {
                        STATUS_INVALID_PARAMETER
                    } else {
                        STATUS_SUCCESS
                    };
                    assert_eq!(
                        status(
                            p,
                            &mut thread,
                            "NtSetTimer2",
                            &[timer, data, data + 8, data + 16]
                        ),
                        expected,
                        "{arch}/{attributes:x}/{due}/{period}"
                    );
                    assert_eq!(
                        status(p, &mut thread, "NtCancelTimer2", &[timer, 0]),
                        STATUS_SUCCESS
                    );
                }
            }
            p.objects.close(timer).unwrap();
        }
    }
}

#[test]
fn native_timer2_arm_null_due_and_optional_parameters_follow_native_precedence_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, out) = setup(arch);
        let p = process.state_mut();
        let timer = create(p, &mut thread, out, 8, 0x1f0003);
        let data = out + 128;
        inputs(p, data, -3_000_000_000, 0, 0, 0, 0);
        assert_eq!(
            status(p, &mut thread, "NtSetTimer2", &[timer, 0, 0, 0]),
            STATUS_INVALID_PARAMETER_2
        );
        assert_eq!(
            status(p, &mut thread, "NtSetTimer2", &[0, 0, 0, 0]),
            STATUS_INVALID_PARAMETER_2
        );
        assert_eq!(
            status(p, &mut thread, "NtSetTimer2", &[timer, data, 0, 0]),
            STATUS_SUCCESS
        );
        assert_eq!(
            status(p, &mut thread, "NtCancelTimer2", &[timer, 0]),
            STATUS_SUCCESS
        );
        p.objects.close(timer).unwrap();
    }
}

#[test]
fn native_timer2_arm_expiry_uses_shared_notification_and_synchronization_waits_all_abis() {
    for arch in WinArch::ALL {
        for notification in [false, true] {
            let (mut process, mut t, out) = setup(arch);
            let p = process.state_mut();
            let h = create(
                p,
                &mut t,
                out,
                8 | if notification { 0x80000000 } else { 0 },
                0x1f0003,
            );
            let id = p.objects.id(h).unwrap();
            inputs(p, out + 128, 0, 0, 0, 0, 0);
            assert_eq!(status(p, &mut t, "NtSetTimer2", &[h, out + 128, 0, 0]), 0);
            assert!(next_deadline(p).is_some());
            tick_timers(p, Instant::now()).unwrap();
            assert!(matches!(
                p.objects.obj(id),
                Some(Object::IrTimer {
                    schedule: None,
                    signaled: true,
                    ..
                })
            ));
            assert_eq!(sync::try_objects(p, t.tid, &[id], false).unwrap(), Some(0));
            assert_eq!(
                sync::try_objects(p, t.tid, &[id], true).unwrap(),
                if notification { Some(0) } else { None }
            );
            p.objects.close(h).unwrap();
        }
    }
}

#[test]
fn native_timer2_arm_parked_native_wait_retains_timer_after_final_handle_close_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let before = (p.objects.handle_count(), p.objects.iter().count());
        let h = create(p, &mut t, out, 8, 0x1f0003);
        let id = p.objects.id(h).unwrap();
        inputs(p, out + 128, 0, 0, 0, 0, 0);
        assert_eq!(status(p, &mut t, "NtSetTimer2", &[h, out + 128, 0, 0]), 0);
        assert_eq!(
            super::native_wait_tests::invoke(p, &mut t, h, 0, 0),
            Outcome::Park
        );
        let ThreadState::Waiting(wait) = &t.state else {
            panic!("native wait did not park")
        };
        let wait = wait.clone();
        p.objects.close(h).unwrap();
        assert!(p.objects.obj(id).is_some());
        tick_timers(p, Instant::now()).unwrap();
        let result = sync::poll(p, t.tid, &wait, Instant::now(), false)
            .unwrap()
            .unwrap();
        assert_eq!(result, 0);
        sync::on_cancel(p, t.tid, &wait).unwrap();
        t.state = ThreadState::Ready;
        assert_eq!(
            dispatch::wait_complete(p, &mut t, result),
            Outcome::Continue
        );
        assert!(t.frames.is_empty());
        assert!(p.objects.obj(id).is_none());
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
    }
}

#[test]
fn native_timer2_arm_expiry_reaches_wait_completion_packet_after_handle_close_all_abis() {
    for arch in WinArch::ALL {
        for notification in [false, true] {
            let (mut process, mut t, out) = setup(arch);
            let p = process.state_mut();
            let before = (p.objects.handle_count(), p.objects.iter().count());
            let h = create(
                p,
                &mut t,
                out,
                8 | if notification { 0x80000000 } else { 0 },
                0x1f0003,
            );
            let id = p.objects.id(h).unwrap();
            assert_eq!(
                status(
                    p,
                    &mut t,
                    "NtCreateWaitCompletionPacket",
                    &[out, 0x1f0001, 0]
                ),
                0
            );
            let packet = handle(p, out);
            assert_eq!(
                status(p, &mut t, "NtCreateIoCompletion", &[out, 0x1f0003, 0, 0]),
                0
            );
            let port = handle(p, out);
            inputs(p, out + 128, 0, 0, 0, 0, 0);
            assert_eq!(status(p, &mut t, "NtSetTimer2", &[h, out + 128, 0, 0]), 0);
            assert_eq!(
                status(
                    p,
                    &mut t,
                    "NtAssociateWaitCompletionPacket",
                    &[
                        packet,
                        port,
                        h,
                        0x1234,
                        0x5678,
                        0xc1234567,
                        0x9abc,
                        out + 256
                    ]
                ),
                0
            );
            p.objects.close(h).unwrap();
            tick_timers(p, Instant::now()).unwrap();
            sync::pump_wait_packets(p).unwrap();
            let port_id = p.objects.id(port).unwrap();
            let completion = p.objects.dequeue_completion(port_id).unwrap();
            assert_eq!(
                (
                    completion.key,
                    completion.apc,
                    completion.status,
                    completion.information
                ),
                (0x1234, 0x5678, 0xc1234567, 0x9abc)
            );
            // Notification transfers to the completion queue, releasing its
            // target pin. With no public handle, the timer is now destroyed.
            assert!(p.objects.obj(id).is_none());
            sync::pump_wait_packets(p).unwrap();
            assert!(p.objects.dequeue_completion(port_id).is_none());
            assert_eq!(
                status(p, &mut t, "NtCancelWaitCompletionPacket", &[packet, 1]),
                STATUS_CANCELLED
            );
            for h in [packet, port] {
                p.objects.close(h).unwrap();
            }
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
        }
    }
}

#[test]
fn native_timer2_arm_scheduler_resumes_a_real_parked_continuation_all_abis() {
    use crate::user::windows::objects::Timer2Schedule;
    for arch in WinArch::ALL {
        let mut p = process(arch);
        let obj = p.objects.create(Object::IrTimer {
            attributes: 8,
            timer_id: None,
            signaled: false,
            schedule: Some(Timer2Schedule::new(0, 0).unwrap()),
        });
        let h = u64::from(p.objects.open(obj, false));
        let mut t = thread(&mut p, 8);
        park(
            &mut t,
            sync::Wait::Objects {
                objs: vec![obj],
                all: false,
                deadline: None,
                alertable: false,
            },
        );
        if let ThreadState::Waiting(wait) = &t.state {
            sync::on_block(&mut p, t.tid, wait).unwrap();
        }
        p.objects.close(h).unwrap();
        t.frames.last_mut().unwrap().cont = Some(Box::new(move |_, result| {
            assert_eq!(result, 0);
            Ok(Flow::ExitThread(0))
        }));
        p.threads.insert(8, t);
        assert_eq!(run(&mut p), ExitStatus::Exited(0));
        assert!(p.objects.obj(obj).is_none());
    }
}
