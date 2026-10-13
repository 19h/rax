//! Independent native captures and scheduler-data resource ownership.
use super::*;
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::{Object, SchedulerData};

#[cfg(windows)]
#[path = "scheduler_slot_leaf_tests.rs"]
mod installed;
#[path = "scheduler_slot_replay_tests.rs"]
mod replay;

fn setup(arch: WinArch) -> (super::super::super::super::WindowsProcess, Thread, u64) {
    let (mut process, t) = fixture(arch, "NtSetInformationThread", 0x0d);
    let p = process.state_mut();
    let base =
        p.vm.allocate(
            None,
            PAGE_SIZE * 2,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    (process, t, base)
}
fn owner(p: &mut Proc, grant: u32) -> u64 {
    let id = p.objects.create(Object::SchedulerSharedData {
        pid: p.pid,
        data: SchedulerData::default(),
    });
    p.objects.open_access(id, false, grant).unwrap().into()
}
fn record(p: &mut Proc, out: u64, action: u32, handle: u64, supplied: u64, padding: u32) {
    let mut bytes = vec![0; if p.arch.is64() { 24 } else { 12 }];
    bytes[..4].copy_from_slice(&action.to_le_bytes());
    let offset = if p.arch.is64() { 8 } else { 4 };
    if p.arch.is64() {
        bytes[4..8].copy_from_slice(&padding.to_le_bytes());
    }
    let n = p.arch.ptr_size() as usize;
    bytes[offset..offset + n].copy_from_slice(&handle.to_le_bytes()[..n]);
    bytes[offset + n..offset + 2 * n].copy_from_slice(&supplied.to_le_bytes()[..n]);
    p.space.wr(out, &bytes).unwrap();
}
fn call(p: &mut Proc, t: &mut Thread, handle: u64, out: u64, length: u64) -> u32 {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    let args = [handle, 57, out, length];
    match p.arch {
        WinArch::Arm64 => {
            for (i, v) in args.into_iter().enumerate() {
                t.cpu.set_gpr(i, v);
            }
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, 0x0d);
            t.cpu.set_gpr(1, 0x1234_0004);
            for (i, v) in args.into_iter().enumerate() {
                t.cpu.set_gpr([10, 2, 8, 9][i], v);
            }
        }
        WinArch::X86 => {
            t.cpu.set_gpr(0, 0x0d);
            p.space.w32(sp, 0x1234_0004).unwrap();
            p.space.w32(sp + 4, 0x1234_0000).unwrap();
            for (i, v) in args.into_iter().enumerate() {
                p.space.w32(sp + 8 + i as u64 * 4, v as u32).unwrap();
            }
        }
    }
    let outcome = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, 0x0d))
    };
    assert_eq!(outcome, Outcome::Continue);
    assert!(t.frames.is_empty());
    // Raw WoW transition consumes its synthetic dispatcher return slot;
    // installed-leaf tests separately verify the real RET16 caller cleanup.
    assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
    t.cpu.set_sp(sp);
    t.cpu.gpr(0) as u32
}
fn change(p: &mut Proc, t: &mut Thread, out: u64, action: u32, h: u64) -> u32 {
    record(p, out, action, h, p.arch.ptr(u64::MAX), 0xffffffff);
    call(
        p,
        t,
        p.arch.ptr(u64::MAX - 1),
        out,
        if p.arch.is64() { 24 } else { 12 },
    )
}
#[test]
fn native_scheduler_slot_owns_mapping_until_last_handle_and_thread_reference_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let h = owner(p, 1);
        assert_eq!(change(p, &mut t, out, 0, h), STATUS_SUCCESS);
        let slot = p.objects.scheduler_slot(t.tid);
        assert_ne!(slot, 0);
        let q = p.vm.query(slot).unwrap();
        assert_eq!(
            (q.allocation_base, q.size, q.state, q.kind, q.protect),
            (slot, PAGE_SIZE, mem::COMMIT, mem::MAPPED, prot::READWRITE)
        );
        let reserve = p.vm.query(slot + PAGE_SIZE).unwrap();
        assert_eq!(
            (
                reserve.allocation_base,
                reserve.size,
                reserve.state,
                reserve.kind
            ),
            (slot, PAGE_SIZE, mem::RESERVE, mem::MAPPED)
        );
        assert_eq!(p.space.bytes(slot, 64).unwrap(), vec![0; 64]);
        p.space.wr(slot, &[0x5a; 64]).unwrap();
        assert_eq!(change(p, &mut t, out, 1, 0x1234), STATUS_SUCCESS);
        assert_eq!(p.space.bytes(slot, 64).unwrap(), vec![0x5a; 64]);
        assert_eq!(change(p, &mut t, out, 0, h), STATUS_SUCCESS);
        assert_eq!(p.objects.scheduler_slot(t.tid), slot);
        assert_eq!(p.space.bytes(slot, 64).unwrap(), vec![0; 64]);
        let alias = p.objects.duplicate(h, true).unwrap();
        assert!(p.objects.close(h).unwrap().is_none());
        assert!(p.objects.close(alias.into()).unwrap().is_none());
        p.reclaim_scheduler_views().unwrap();
        assert_eq!(p.vm.query(slot).unwrap().state, mem::COMMIT);
        assert_eq!(change(p, &mut t, out, 1, 0), STATUS_SUCCESS);
        assert_eq!(p.objects.scheduler_slot(t.tid), 0);
        assert_eq!(p.vm.query(slot).unwrap().state, mem::FREE);
        let h = owner(p, 1);
        assert_eq!(change(p, &mut t, out, 0, h), STATUS_SUCCESS);
        let slot = p.objects.scheduler_slot(t.tid);
        assert!(p.objects.close(h).unwrap().is_none());
        super::super::super::super::thread::destroy(p, t, 77);
        assert_eq!(p.vm.query(slot).unwrap().state, mem::FREE);
    }
}
#[test]
fn native_scheduler_slot_pool_has_128_slots_commits_second_page_and_never_reaps_reused_addresses() {
    for arch in WinArch::ALL {
        let (mut process, _t, _out) = setup(arch);
        let p = process.state_mut();
        let h = owner(p, 1);
        let id = p.objects.id(h).unwrap();
        let mut first = 0;
        for tid in 0..128 {
            let slot = p.objects.assign_scheduler_slot(&mut p.vm, id, tid).unwrap();
            if tid == 0 {
                first = slot;
            }
            assert_eq!(slot, first + u64::from(tid) * 64);
        }
        assert_eq!(
            p.objects.assign_scheduler_slot(&mut p.vm, id, 128),
            Err(STATUS_INSUFFICIENT_RESOURCES)
        );
        assert_eq!(p.vm.query(first).unwrap().size, 8192);
        assert_eq!(p.vm.query(first + 4096).unwrap().allocation_base, first);
        p.objects.free_scheduler_slot(63);
        assert_eq!(
            p.objects.assign_scheduler_slot(&mut p.vm, id, 128).unwrap(),
            first + 63 * 64
        );
        for tid in 0..129 {
            p.objects.free_scheduler_slot(tid);
        }
        p.vm.release(first).unwrap();
        let replacement =
            p.vm.allocate(
                Some(first),
                8192,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        assert_eq!(replacement, first);
        p.space.wr(first, &[0x7b; 64]).unwrap();
        // The native owner caches the old address even after explicit unmap.
        assert_eq!(
            p.objects.assign_scheduler_slot(&mut p.vm, id, 128).unwrap(),
            first
        );
        assert_eq!(p.space.bytes(first, 64).unwrap(), vec![0x7b; 64]);
        p.objects.free_scheduler_slot(128);
        p.objects.close(h).unwrap();
        p.reclaim_scheduler_views().unwrap();
        assert_eq!(p.space.bytes(first, 64).unwrap(), vec![0x7b; 64]);
        assert_eq!(p.vm.query(first).unwrap().kind, mem::PRIVATE);
    }
}
#[test]
fn native_scheduler_slot_wow_copy_fault_preserves_hidden_binding_and_process_drain_reclaims_views()
{
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let h = owner(p, 1);
        record(p, out + PAGE_SIZE, 0, h, 0, 0);
        p.vm.protect(out + PAGE_SIZE, PAGE_SIZE, prot::READONLY)
            .unwrap();
        assert_eq!(
            call(
                p,
                &mut t,
                arch.ptr(u64::MAX - 1),
                out + PAGE_SIZE,
                if arch.is64() { 24 } else { 12 }
            ),
            STATUS_ACCESS_VIOLATION
        );
        let slot = p.objects.scheduler_slot(t.tid);
        assert_eq!(slot != 0, arch == WinArch::X86);
        if slot == 0 {
            p.vm.protect(out + PAGE_SIZE, PAGE_SIZE, prot::READWRITE)
                .unwrap();
            assert_eq!(change(p, &mut t, out, 0, h), STATUS_SUCCESS);
        }
        let slot = p.objects.scheduler_slot(t.tid);
        // A caller may consume no yielded objects; Drain still destroys all.
        p.objects.drain().take(0).for_each(drop);
        p.reclaim_scheduler_views().unwrap();
        assert_eq!(p.objects.scheduler_slot(t.tid), 0);
        assert_eq!(p.vm.query(slot).unwrap().state, mem::FREE);
    }
}
