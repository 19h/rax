//! Native service argument transport; expectations come from installed NT captures.
use super::*;
const SERVICE: u32 = 0x77;
fn invoke(p: &mut Proc, t: &mut Thread, name: &str, args: &[u64]) -> Outcome {
    p.modules.nt_services = Some((
        p.modules.nt_services.as_ref().unwrap().0,
        table(p.arch, name, SERVICE),
    ));
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            for (index, value) in [0x1234_0004, 0x1234_0000]
                .into_iter()
                .chain(args.iter().copied())
                .enumerate()
            {
                p.space.w32(sp + index as u64 * 4, value as u32).unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(1, 0x1234_0004);
            for (index, value) in args.iter().copied().enumerate() {
                if index < 4 {
                    t.cpu.set_gpr([10, 2, 8, 9][index], value);
                } else {
                    p.space
                        .w64(sp + 40 + (index - 4) as u64 * 8, value)
                        .unwrap();
                }
            }
        }
        WinArch::Arm64 => {
            for (index, value) in args.iter().copied().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
    }
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
#[test]
fn native_wait_packet_associate_dispatches_all_eight_arguments() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
        let p = process.state_mut();
        let out =
            p.vm.allocate(None, PAGE_SIZE, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                .unwrap()
                .0;
        p.space.wr(out, &[0xA5; 16]).unwrap();
        let w = p
            .objects
            .insert(Object::WaitCompletionPacket { signaled: true });
        let port = p.objects.insert(Object::CompletionPort {
            queue: Default::default(),
            concurrency: 1,
        });
        let event = p.objects.insert(Object::Event {
            manual: true,
            signaled: 0,
        });
        assert_eq!(
            invoke(
                p,
                &mut t,
                "NtAssociateWaitCompletionPacket",
                &[
                    w.into(),
                    port.into(),
                    event.into(),
                    0x1234567887654321,
                    0xA1B2C3D4CCCCDDDD,
                    0xC1234567,
                    0xFEDCBA9876543210,
                    out
                ]
            ),
            Outcome::Continue,
            "{arch}"
        );
        assert_eq!(t.cpu.gpr(0), 0);
        assert_eq!(
            p.space.bytes(out, 16).unwrap(),
            [vec![0], vec![0xA5; 15]].concat()
        );
        assert_eq!(
            invoke(p, &mut t, "NtCancelWaitCompletionPacket", &[w.into(), 1]),
            Outcome::Continue
        );
        assert_eq!(t.cpu.gpr(0), 0);
        for h in [event, port, w] {
            p.objects.close(h.into()).unwrap();
        }
    }
}

#[cfg(windows)]
#[path = "wait_packet_associate_leaf_tests.rs"]
mod leaf_tests;
#[path = "wait_packet_associate_replay_tests.rs"]
mod replay_tests;

fn status(p: &mut Proc, t: &mut Thread, name: &str, args: &[u64]) -> u32 {
    assert_eq!(
        invoke(p, t, name, args),
        Outcome::Continue,
        "{}/{name}",
        p.arch
    );
    t.cpu.gpr(0) as u32
}
fn output(p: &mut Proc) -> u64 {
    p.vm.allocate(
        None,
        2 * PAGE_SIZE,
        mem::RESERVE | mem::COMMIT,
        prot::READWRITE,
    )
    .unwrap()
    .0
}
fn object(p: &mut Proc, kind: usize, access: u32) -> u64 {
    let obj = match kind {
        0 => Object::WaitCompletionPacket { signaled: true },
        1 => Object::CompletionPort {
            queue: Default::default(),
            concurrency: 1,
        },
        _ => Object::Event {
            manual: true,
            signaled: 0,
        },
    };
    let id = p.objects.create(obj);
    p.objects.open_access(id, false, access).unwrap().into()
}
fn associate(p: &mut Proc, t: &mut Thread, w: u64, port: u64, target: u64, out: u64) -> u32 {
    status(
        p,
        t,
        "NtAssociateWaitCompletionPacket",
        &[
            w,
            port,
            target,
            0x1234567887654321,
            0xA1B2C3D4CCCCDDDD,
            0xC1234567,
            0xFEDCBA9876543210,
            out,
        ],
    )
}
fn cancel(p: &mut Proc, t: &mut Thread, w: u64, flag: u64) -> u32 {
    status(p, t, "NtCancelWaitCompletionPacket", &[w, flag])
}
fn pump(p: &mut Proc) {
    sync::pump_wait_packets(p).unwrap();
}
fn queued(p: &Proc, port: u64) -> usize {
    match p.objects.get(port).unwrap() {
        Object::CompletionPort { queue, .. } => queue.len(),
        _ => panic!("completion port"),
    }
}

#[test]
fn native_wait_packet_associate_cancellation_dequeue_reuse_and_full_payload_all_abis() {
    for arch in WinArch::ALL {
        for manual in [false, true] {
            for initial in [0, 1] {
                let (mut process, mut t) =
                    fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
                let p = process.state_mut();
                let before = (p.objects.handle_count(), p.objects.iter().count());
                let out = output(p);
                p.space.wr(out, &[0xA5; 16]).unwrap();
                let w = object(p, 0, 0x1F0001);
                let port = object(p, 1, 0x1F0003);
                let target = p.objects.insert(Object::Event {
                    manual,
                    signaled: initial,
                }) as u64;
                assert_eq!(cancel(p, &mut t, w, 0), STATUS_CANCELLED);
                assert_eq!(associate(p, &mut t, w, port, target, out), 0);
                assert_eq!(
                    p.space.bytes(out, 16).unwrap(),
                    [vec![initial as u8], vec![0xA5; 15]].concat()
                );
                assert_eq!(
                    associate(p, &mut t, w, port, target, 0),
                    STATUS_INVALID_PARAMETER_1
                );
                if initial == 1 {
                    assert_eq!(cancel(p, &mut t, w, 0), STATUS_PENDING);
                    assert_eq!(queued(p, port), 1);
                } else {
                    assert_eq!(cancel(p, &mut t, w, 0), 0);
                    assert_eq!(queued(p, port), 0);
                    assert_eq!(associate(p, &mut t, w, port, target, 0), 0);
                }
                assert_eq!(cancel(p, &mut t, w, 2), 0);
                assert_eq!(queued(p, port), 0);
                assert_eq!(associate(p, &mut t, w, port, target, out), 0);
                status(p, &mut t, "NtSetEvent", &[target, 0]);
                pump(p);
                assert_eq!(queued(p, port), 1);
                let port_id = p.objects.id(port).unwrap();
                let packet = p.objects.dequeue_completion(port_id).unwrap();
                assert_eq!(packet.key, arch.ptr(0x1234567887654321));
                assert_eq!(packet.apc, arch.ptr(0xA1B2C3D4CCCCDDDD));
                assert_eq!(packet.status, 0xC1234567);
                assert_eq!(packet.information, arch.ptr(0xFEDCBA9876543210));
                assert_eq!(cancel(p, &mut t, w, 1), STATUS_CANCELLED);
                assert_eq!(associate(p, &mut t, w, port, target, out), 0);
                assert_eq!(p.space.bytes(out, 1).unwrap(), vec![u8::from(manual)]);
                assert_eq!(cancel(p, &mut t, w, 255), 0);
                for h in [target, port, w] {
                    p.objects.close(h).unwrap();
                }
                assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            }
        }
    }
}

#[test]
fn native_wait_packet_associate_retains_dependencies_and_final_handle_close_cancels_all_abis() {
    for arch in WinArch::ALL {
        for initial in [0, 1] {
            for order in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                let (mut process, mut t) =
                    fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
                let p = process.state_mut();
                let before = (p.objects.handle_count(), p.objects.iter().count());
                let w = object(p, 0, 0x1F0001);
                let port = object(p, 1, 0x1F0003);
                let target = p.objects.insert(Object::Event {
                    manual: true,
                    signaled: initial,
                }) as u64;
                let id = p.objects.id(target).unwrap();
                let port_id = p.objects.id(port).unwrap();
                let alias = p.objects.duplicate(w, false).unwrap() as u64;
                assert_eq!(associate(p, &mut t, w, port, target, 0), 0);
                p.objects.close(w).unwrap();
                assert!(p.objects.get(alias).is_some());
                assert_eq!(queued(p, port), initial as usize);
                let handles = [alias, port, target];
                for which in order {
                    p.objects.close(handles[which]).unwrap();
                    assert_eq!(
                        p.objects.obj(port_id).is_some(),
                        p.objects.get(port).is_some()
                            || (initial == 0 && p.objects.get(alias).is_some()),
                        "{arch:?}, initial={initial}, close={which}"
                    );
                }
                assert!(p.objects.obj(id).is_none());
                assert!(p.objects.obj(port_id).is_none());
                assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            }
        }
    }
    // Last handle closes while a separate object wait still pins the packet.
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
        let p = process.state_mut();
        let before = p.objects.iter().count();
        let w = object(p, 0, 0x1F0001);
        let port = object(p, 1, 0x1F0003);
        let target = object(p, 2, 0x1F0003);
        let wid = p.objects.id(w).unwrap();
        assert_eq!(associate(p, &mut t, w, port, target, 0), 0);
        p.objects.retain_many(&[wid]).unwrap();
        p.objects.close(w).unwrap();
        status(p, &mut t, "NtSetEvent", &[target, 0]);
        pump(p);
        assert_eq!(queued(p, port), 0);
        assert!(matches!(
            p.objects.obj(wid),
            Some(Object::WaitCompletionPacket { signaled: true })
        ));
        p.objects.release(wid);
        p.objects.close(target).unwrap();
        p.objects.close(port).unwrap();
        assert_eq!(p.objects.iter().count(), before);
    }
}

#[test]
fn native_wait_packet_associate_fifo_auto_reset_semaphore_and_drain_all_abis() {
    for arch in WinArch::ALL {
        for semaphore in [false, true] {
            let (mut process, mut t) = fixture(arch, "NtAssociateWaitCompletionPacket", SERVICE);
            let p = process.state_mut();
            let w1 = object(p, 0, 0x1F0001);
            let w2 = object(p, 0, 0x1F0001);
            let port = object(p, 1, 0x1F0003);
            let target = p.objects.insert(if semaphore {
                Object::Semaphore { count: 0, max: 2 }
            } else {
                Object::Event {
                    manual: false,
                    signaled: 0,
                }
            }) as u64;
            for (w, key) in [(w2, 2), (w1, 1)] {
                assert_eq!(
                    status(
                        p,
                        &mut t,
                        "NtAssociateWaitCompletionPacket",
                        &[w, port, target, key, 0, 0, key, 0]
                    ),
                    0
                );
            }
            let id = p.objects.id(port).unwrap();
            for key in [2, 1] {
                match p.objects.get_mut(target).unwrap() {
                    Object::Semaphore { count, .. } => *count += 1,
                    Object::Event { signaled, .. } => *signaled = 1,
                    _ => unreachable!(),
                };
                pump(p);
                assert_eq!(queued(p, port), 1);
                let item = p.objects.dequeue_completion(id).unwrap();
                assert_eq!(item.key, key);
                assert_eq!(item.information, key);
            }
            for h in [w1, w2, port, target] {
                p.objects.close(h).unwrap();
            }
            // Whole-process teardown clears associations without referencing drained IDs.
            let w = object(p, 0, 0x1F0001);
            let port = object(p, 1, 0x1F0003);
            let target = object(p, 2, 0x1F0003);
            assert_eq!(associate(p, &mut t, w, port, target, 0), 0);
            drop(p.objects.drain().collect::<Vec<_>>());
            assert_eq!(p.objects.handle_count(), 0);
            assert_eq!(p.objects.iter().count(), 0);
        }
    }
}
