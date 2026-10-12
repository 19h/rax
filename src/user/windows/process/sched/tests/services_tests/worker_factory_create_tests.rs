//! Measured installed worker-factory construction contracts.
use super::*;
#[path = "worker_factory_create_replay_tests.rs"]
mod replay_tests;

#[cfg(windows)]
#[path = "worker_factory_create_leaf_tests.rs"]
mod leaf_tests;

const SERVICE: u32 = 0x74;
fn arguments(p: &mut Proc, t: &mut Thread, args: [u64; 10]) {
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
            for (index, value) in args.into_iter().enumerate().skip(4) {
                p.space.w64(sp + 8 + index as u64 * 8, value).unwrap();
            }
        }
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                if index < 8 {
                    t.cpu.set_gpr(index, value);
                } else {
                    p.space.w64(sp + (index as u64 - 8) * 8, value).unwrap();
                }
            }
        }
    }
}
fn call(p: &mut Proc, t: &mut Thread, args: [u64; 10]) -> Outcome {
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
    let (mut process, t) = fixture(arch, "NtCreateWorkerFactory", SERVICE);
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

#[test]
fn native_worker_factory_create_constructs_real_object_and_closes_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let port = p.objects.insert(Object::CompletionPort {
            queue: Default::default(),
            concurrency: 4,
        });
        let handles = p.objects.handle_count();
        let objects = p.objects.iter().count();
        let args = [
            out,
            0xF00FF,
            0,
            u64::from(port),
            arch.ptr(u64::MAX),
            0x12345678,
            0x87654321,
            512,
            0x100000,
            0x1000,
        ];
        assert_eq!(call(p, &mut t, args), Outcome::Continue);
        assert_eq!(t.cpu.gpr(0), 0);
        let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
        assert_eq!(
            p.objects.get(handle).unwrap().type_name(),
            "TpWorkerFactory"
        );
        assert_eq!(p.objects.access(handle), Some(0xF00FF));
        assert_eq!(p.objects.handle_count(), handles + 1);
        assert_eq!(p.objects.iter().count(), objects + 1);
        assert!(p.objects.close(handle).is_ok());
        assert_eq!(p.objects.handle_count(), handles);
        assert_eq!(p.objects.iter().count(), objects);
    }
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
fn native_worker_factory_create_owns_dependencies_through_duplicate_and_final_close_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let baseline = p.objects.iter().count();
        let handles = p.objects.handle_count();
        let port = p.objects.insert(Object::CompletionPort {
            queue: Default::default(),
            concurrency: 4,
        });
        let port_id = p.objects.id(u64::from(port)).unwrap();
        let proc_id = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        let proc_handle = p.objects.open_access(proc_id, false, 0x2A).unwrap();
        assert_eq!(
            call(
                p,
                &mut t,
                [
                    out,
                    0xF00FF,
                    0,
                    u64::from(port),
                    u64::from(proc_handle),
                    1,
                    2,
                    0,
                    0,
                    0
                ]
            ),
            Outcome::Continue
        );
        assert_eq!(t.cpu.gpr(0), 0);
        let h = p.space.ptr(out, arch.ptr_size()).unwrap();
        let second = p.objects.duplicate(h, false).unwrap();
        p.objects.close(u64::from(port)).unwrap();
        p.objects.close(u64::from(proc_handle)).unwrap();
        assert!(matches!(
            p.objects.obj(port_id),
            Some(Object::CompletionPort { .. })
        ));
        assert!(matches!(
            p.objects.obj(proc_id),
            Some(Object::Process { .. })
        ));
        p.objects.close(h).unwrap();
        assert_eq!(p.objects.iter().count(), baseline + 3);
        assert!(matches!(
            p.objects.get(u64::from(second)),
            Some(Object::WorkerFactory {
                maximum: 0,
                soft_maximum: None,
                reserve: 65536,
                commit: 4096,
                ..
            })
        ));
        p.objects.close(u64::from(second)).unwrap();
        assert_eq!(p.objects.iter().count(), baseline);
        assert_eq!(p.objects.handle_count(), handles);
    }
}
#[test]
fn native_worker_factory_create_keeps_cross_process_policy_explicit_without_publication_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let port = p.objects.insert(Object::CompletionPort {
            queue: Default::default(),
            concurrency: 4,
        });
        let proc_id = p.objects.create(Object::Process {
            pid: p.pid + 1,
            exit_code: None,
        });
        let proc_handle = p.objects.open_access(proc_id, false, 0x2A).unwrap();
        let handles = p.objects.handle_count();
        let objects = p.objects.iter().count();
        assert!(
            matches!(call(p,&mut t,[out,0xF00FF,0,u64::from(port),u64::from(proc_handle),1,2,512,0,0]),Outcome::Fail(reason) if reason.contains("cross-process"))
        );
        assert_eq!(p.objects.handle_count(), handles);
        assert_eq!(p.objects.iter().count(), objects);
        assert_eq!(p.space.bytes(out, 16).unwrap(), [0xA5; 16]);
    }
}
