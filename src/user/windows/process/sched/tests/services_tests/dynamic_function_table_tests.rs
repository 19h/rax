//! Native process-range ownership, fault ordering and syscall transport.

use super::{Outcome, Proc, Thread, WinArch, fixture, handle_stop, stop};
use crate::user::windows::layout;
use crate::user::windows::memory::{Mem, mem, prot};
use crate::user::windows::nt::{self, status::*};

const SERVICE: u32 = 0x1c;
const CLASS: u64 = 53;
const LAST_ERROR: u32 = 0x4A2E_19F7;

#[cfg(windows)]
#[path = "dynamic_function_table_leaf_tests.rs"]
mod installed;
#[path = "dynamic_function_table_replay_tests.rs"]
mod replay;

fn call(p: &mut Proc, thread: &mut Thread, process: u64, input: u64, length: u32) -> u32 {
    let last_error = thread.teb + layout::offsets(p.arch).teb_last_error;
    p.space.w32(last_error, LAST_ERROR).unwrap();
    let sp = thread.cpu.sp();
    thread.cpu.set_pc(0x1234_0004);
    let args = [process, CLASS, input, u64::from(length)];
    match p.arch {
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                thread.cpu.set_gpr(index, value);
            }
        }
        WinArch::X64 => {
            thread.cpu.set_gpr(0, SERVICE.into());
            thread.cpu.set_gpr(1, 0x1234_0004);
            for (index, value) in args.into_iter().enumerate() {
                thread.cpu.set_gpr([10, 2, 8, 9][index], value);
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
            thread.cpu.set_gpr(0, SERVICE.into());
        }
    }
    let outcome = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, thread, 0x1234_0000)
    } else {
        handle_stop(p, thread, stop(p.arch, SERVICE))
    };
    assert_eq!(outcome, Outcome::Continue);
    assert_eq!(thread.cpu.pc(), 0x1234_0004);
    assert_eq!(
        thread.cpu.sp(),
        sp + if p.arch == WinArch::X86 { 4 } else { 0 }
    );
    assert!(thread.frames.is_empty());
    assert_eq!(p.space.u32(last_error).unwrap(), LAST_ERROR);
    thread.cpu.set_sp(sp);
    thread.cpu.gpr(0) as u32
}

fn scratch(p: &mut Proc) -> u64 {
    p.vm.allocate(None, 0x8000, mem::RESERVE | mem::COMMIT, prot::READWRITE)
        .unwrap()
        .0
}

fn record(p: &mut Proc, table: u64, base: u64, maximum: u64) {
    p.vm.poke(table, &[0; 256]).unwrap();
    p.space.w64(table + 40, maximum).unwrap();
    p.space.w64(table + 48, base).unwrap();
}

fn packet(p: &mut Proc, input: u64, table: u64, remove: u8) {
    p.vm.poke(input, &[0; 16]).unwrap();
    p.space.wptr(input, p.arch.ptr_size(), table).unwrap();
    p.space.w8(input + p.arch.ptr_size(), remove).unwrap();
}

fn operation(p: &mut Proc, thread: &mut Thread, input: u64, table: u64, remove: u8) -> u32 {
    packet(p, input, table, remove);
    let process = p.arch.ptr(u64::MAX);
    let length = p.arch.ptr_size() as u32 * 2;
    call(p, thread, process, input, length)
}

#[test]
fn native_dynamic_function_table_process_range_lifecycle_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, table) = (memory, memory + 0x100);
        record(p, table, 0x400_0000_0000, 0x400_0000_1000);
        let original = p.space.bytes(table, 256).unwrap();
        assert_eq!(operation(p, &mut thread, input, table, 0), STATUS_SUCCESS);
        let expected_count = usize::from(arch != WinArch::X86);
        assert_eq!(p.seh.kernel_dynamic_tables.len(), expected_count);
        assert_eq!(
            operation(p, &mut thread, input, table, 0),
            if arch == WinArch::X86 {
                STATUS_SUCCESS
            } else {
                STATUS_CONFLICTING_ADDRESSES
            }
        );
        assert_eq!(p.seh.kernel_dynamic_tables.len(), expected_count);
        assert_eq!(operation(p, &mut thread, input, table, 255), STATUS_SUCCESS);
        assert_eq!(p.seh.kernel_dynamic_tables.len(), 0);
        assert_eq!(
            operation(p, &mut thread, input, table, 2),
            if arch == WinArch::X86 {
                STATUS_SUCCESS
            } else {
                STATUS_BAD_FUNCTION_TABLE
            }
        );
        assert_eq!(p.space.bytes(table, 256).unwrap(), original);
        assert!(p.seh.dynamic_tables.is_empty());
    }
}

#[test]
fn native_dynamic_function_table_invalid_process_precedes_length_and_pointer_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        for process in [0, 4, 0xDEAD_BEEC, arch.ptr(u64::MAX - 1), 0xFFFF_FFFF] {
            if process == arch.ptr(u64::MAX) {
                continue;
            }
            for length in [0, 8, 15, 16, 17, u32::MAX] {
                assert_eq!(
                    call(p, &mut thread, process, 1, length),
                    if arch == WinArch::X86 {
                        STATUS_SUCCESS
                    } else {
                        STATUS_INVALID_PARAMETER
                    }
                );
            }
        }
        assert_eq!(p.seh.kernel_dynamic_tables.len(), 0);
    }
}

#[test]
fn native_dynamic_function_table_survives_descriptor_range_and_thread_lifetime() {
    for arch in WinArch::ALL {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, copy) = (memory, memory + 0x100);
        let region = scratch(p);
        let descriptor = scratch(p);
        record(p, descriptor, region, region + 4096);
        assert_eq!(
            operation(p, &mut thread, input, descriptor, 0),
            STATUS_SUCCESS
        );
        let bytes = p.space.bytes(descriptor, 56).unwrap();
        p.vm.release(region).unwrap();
        p.vm.release(descriptor).unwrap();
        p.vm.poke(copy, &bytes).unwrap();
        assert_eq!(
            operation(p, &mut thread, input, copy, 0),
            if arch == WinArch::X86 {
                STATUS_SUCCESS
            } else {
                STATUS_CONFLICTING_ADDRESSES
            }
        );
        let tid = crate::user::windows::process::thread::create(p, 0x1234_0000, 0, 0x10000, false)
            .unwrap();
        let mut other = p.threads.remove(&tid).unwrap();
        assert_eq!(operation(p, &mut other, input, copy, 1), STATUS_SUCCESS);
        assert_eq!(p.seh.kernel_dynamic_tables.len(), 0);
        assert_eq!(operation(p, &mut other, input, copy, 0), STATUS_SUCCESS);
        assert_eq!(operation(p, &mut thread, input, copy, 1), STATUS_SUCCESS);
        assert_eq!(p.seh.kernel_dynamic_tables.len(), 0);
    }
}

#[test]
fn native_dynamic_function_table_status_mapping_matches_native_rtl() {
    assert_eq!(nt::status_to_error(STATUS_BAD_FUNCTION_TABLE), 559);
    assert_eq!(nt::status_to_error(STATUS_INTEGER_OVERFLOW), 534);
}
