//! Native compact TLS packets, caller stores and all three syscall transports.
use super::*;
use crate::user::windows::layout;

const SERVICE: u32 = 0x1c;

#[path = "process_tls_replay_tests.rs"]
mod replay;

fn call(p: &mut Proc, t: &mut Thread, process: u64, input: u64, length: u32) -> u32 {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    let args = [process, 35, input, u64::from(length)];
    match p.arch {
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(1, 0x1234_0004);
            for (index, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr([10, 2, 8, 9][index], value);
            }
        }
        WinArch::X86 => {
            p.space.w32(sp, 0x1234_0004).unwrap();
            p.space.w32(sp + 4, 0x1234_0000).unwrap();
            for (index, value) in args.into_iter().enumerate() {
                p.space
                    .w32(sp + 8 + index as u64 * 4, value as u32)
                    .unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
    }
    let result = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    };
    assert_eq!(result, Outcome::Continue);
    assert_eq!(t.cpu.pc(), 0x1234_0004);
    assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
    assert!(t.frames.is_empty());
    t.cpu.set_sp(sp);
    t.cpu.gpr(0) as u32
}

fn scratch(p: &mut Proc) -> u64 {
    p.vm.allocate(
        None,
        PAGE_SIZE * 8,
        mem::RESERVE | mem::COMMIT,
        prot::READWRITE,
    )
    .unwrap()
    .0
}

fn packet(p: &mut Proc, input: u64, operation: u32, count: u32, index: u32, next: &[u64]) -> u32 {
    let ptr = p.arch.ptr_size();
    p.space.wr(input, &[0; 128]).unwrap();
    p.space.w32(input + 4, operation).unwrap();
    p.space.w32(input + 8, count).unwrap();
    p.space.w32(input + 12, index).unwrap();
    for (ordinal, &value) in next.iter().enumerate() {
        p.space
            .wptr(input + 16 + ordinal as u64 * ptr * 3 + ptr, ptr, value)
            .unwrap();
    }
    (16 + u64::from(count) * ptr * 3) as u32
}

fn publish(p: &mut Proc, t: &mut Thread, array: u64) {
    let ptr = p.arch.ptr_size();
    p.space
        .wptr(t.teb + layout::offsets(p.arch).teb_tls_pointer, ptr, array)
        .unwrap();
    t.tls_array = array;
}

#[test]
fn native_process_tls_compact_index_vector_and_startup_sentinel_all_abis() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let ptr = arch.ptr_size();
        let old = base + PAGE_SIZE;
        let new = base + PAGE_SIZE * 2;
        let marker = 0x1234_5678;
        p.space.wptr(old, ptr, marker).unwrap();
        publish(p, &mut t, old);
        let length = packet(p, base, 0, 1, 0, &[new]);
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_SUCCESS
        );
        assert_eq!(p.space.ptr(old, ptr).unwrap(), new);
        assert_eq!(p.space.u32(base + 16).unwrap(), 2);
        assert_eq!(p.space.ptr(base + 16 + ptr, ptr).unwrap(), marker);
        p.space.wptr(old, ptr, marker).unwrap();
        let length = packet(p, base, 1, 1, 1, &[new]);
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_SUCCESS
        );
        assert_eq!(t.tls_array, new);
        assert_eq!(p.space.ptr(new, ptr).unwrap(), marker);
        assert_eq!(p.space.ptr(base + 16 + ptr, ptr).unwrap(), old);
        assert_eq!(
            p.space.ptr(base + 16 + ptr * 2, ptr).unwrap(),
            u64::from(t.tid)
        );

        let sentinel = t.teb + layout::offsets(arch).teb_tls_pointer;
        publish(p, &mut t, sentinel);
        let length = packet(p, base, 1, 1, 16, &[new]);
        p.space.wptr(new, ptr, marker).unwrap();
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_SUCCESS
        );
        assert_eq!(t.tls_array, new);
        assert_eq!(p.space.ptr(new, ptr).unwrap(), marker);
        assert_eq!(p.space.ptr(base + 16 + ptr, ptr).unwrap(), 0);
        publish(p, &mut t, 0);
        let length = packet(p, base, 1, 1, 0, &[old]);
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_SUCCESS
        );
        assert_eq!(t.tls_array, 0);
        assert_eq!(p.space.u32(base + 16).unwrap(), 0);
    }
}

#[test]
fn native_process_tls_later_copy_fault_and_wow_caller_copyback_preserve_state() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let ptr = arch.ptr_size();
        let old = base + PAGE_SIZE;
        let first = base + PAGE_SIZE * 2;
        let second = base + PAGE_SIZE * 3;
        p.space.wptr(old, ptr, 0x1234_5678).unwrap();
        let peer_tid = t.tid + 4;
        let mut peer = thread(p, peer_tid);
        publish(p, &mut peer, old);
        p.threads.insert(peer_tid, peer);
        publish(p, &mut t, old);
        let length = packet(p, base, 1, 2, 1, &[first, second]);
        p.vm.protect(second, PAGE_SIZE, prot::READONLY).unwrap();
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_ACCESS_VIOLATION
        );
        assert_eq!(t.tls_array, first);
        assert_eq!(p.threads[&peer_tid].tls_array, old);
        assert_eq!(p.space.ptr(first, ptr).unwrap(), 0x1234_5678);
        assert_eq!(
            p.space.u32(base + 16).unwrap(),
            if arch == WinArch::X86 { 0 } else { 2 }
        );

        p.threads.remove(&peer_tid);
        publish(p, &mut t, old);
        let length = packet(p, base, 0, 1, 0, &[first]);
        p.vm.protect(base, PAGE_SIZE, prot::READONLY).unwrap();
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_ACCESS_VIOLATION
        );
        assert_eq!(
            p.space.ptr(old, ptr).unwrap(),
            if arch == WinArch::X86 {
                first
            } else {
                0x1234_5678
            }
        );
        assert_eq!(p.space.u32(base + 16).unwrap(), 0);
    }
}

#[test]
fn native_process_tls_invalid_lengths_handles_flags_and_guards_all_abis() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let ptr = arch.ptr_size();
        let old = base + PAGE_SIZE;
        let new = base + PAGE_SIZE * 2;
        publish(p, &mut t, old);
        let length = packet(p, base, 0, 1, 0, &[new]);
        assert_eq!(call(p, &mut t, 0, base, length), STATUS_INVALID_PARAMETER);
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, 0),
            if arch == WinArch::X86 {
                STATUS_SUCCESS
            } else {
                STATUS_INFO_LENGTH_MISMATCH
            }
        );
        packet(p, base, 0, 1, 0, &[new]);
        p.space.w32(base + 16, 1).unwrap();
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_INVALID_PARAMETER
        );
        packet(p, base, 0, 1, 0, &[new]);
        p.vm.protect(base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_GUARD_PAGE_VIOLATION
        );
        assert_eq!(p.vm.query(base).unwrap().protect, prot::READWRITE);
        assert_eq!(p.space.u32(base + 16).unwrap(), 0);
        assert_eq!(
            p.space
                .ptr(t.teb + layout::offsets(arch).teb_tls_pointer, ptr)
                .unwrap(),
            old
        );
    }
}

#[test]
fn native_process_tls_caller_alias_observes_native_stores_and_wow_capture() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let ptr = arch.ptr_size();
        let old = base + PAGE_SIZE;
        p.space.wptr(old, ptr, 0x1122_3344).unwrap();
        p.space.wptr(old + ptr, ptr, 0x5566_7788).unwrap();
        publish(p, &mut t, old);
        let next = base + 16;
        let length = packet(p, base, 1, 1, 2, &[next]);
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, length),
            STATUS_SUCCESS
        );
        assert_eq!(t.tls_array, next);
        assert_eq!(
            p.space.u32(base + 16).unwrap(),
            if arch == WinArch::X86 { 2 } else { 0x1122_3344 }
        );
        assert_eq!(p.space.ptr(base + 16 + ptr, ptr).unwrap(), old);
        assert_eq!(
            p.space.ptr(base + 16 + ptr * 2, ptr).unwrap(),
            u64::from(t.tid)
        );
    }
}

#[test]
fn native_process_tls_structural_length_check_precedes_buffer_visit() {
    for arch in [WinArch::Arm64, WinArch::X64] {
        let (mut process, mut t) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        publish(p, &mut t, base + PAGE_SIZE);
        packet(p, base, 0, 1, 2, &[base + PAGE_SIZE * 2]);
        for length in [0, 15, 16, 39, 41, 56, 8192, u32::MAX] {
            p.vm.protect(base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                .unwrap();
            assert_eq!(
                call(p, &mut t, arch.ptr(u64::MAX), base, length),
                STATUS_INFO_LENGTH_MISMATCH
            );
            assert_eq!(
                p.vm.query(base).unwrap().protect,
                prot::READWRITE | prot::GUARD
            );
            assert_eq!(
                call(p, &mut t, arch.ptr(u64::MAX), 0, length),
                STATUS_INFO_LENGTH_MISMATCH
            );
        }
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, 64),
            STATUS_GUARD_PAGE_VIOLATION
        );
        assert_eq!(p.vm.query(base).unwrap().protect, prot::READWRITE);
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), base, 64),
            STATUS_INFO_LENGTH_MISMATCH
        );
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), 0, 64),
            STATUS_ACCESS_VIOLATION
        );
        assert_eq!(
            call(p, &mut t, arch.ptr(u64::MAX), 1, 15),
            STATUS_DATATYPE_MISALIGNMENT
        );
    }
}
