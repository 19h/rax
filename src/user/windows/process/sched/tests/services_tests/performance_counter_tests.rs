//! Native counter-first stores and all three syscall argument transports.
use super::*;
use crate::user::windows::{layout, performance_clock::PerformanceClock};

const SERVICE: u32 = 0x31;
#[cfg(windows)]
#[path = "performance_counter_leaf_tests.rs"]
mod installed;
#[path = "performance_counter_replay_tests.rs"]
mod replay;

fn call(p: &mut Proc, t: &mut Thread, counter: u64, frequency: u64) -> u32 {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::Arm64 => {
            t.cpu.set_gpr(0, counter);
            t.cpu.set_gpr(1, frequency);
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(10, counter);
            t.cpu.set_gpr(2, frequency);
            t.cpu.set_gpr(1, 0x1234_0004);
        }
        WinArch::X86 => {
            p.space.w32(sp, 0x1234_0004).unwrap();
            p.space.w32(sp + 4, 0x1234_0000).unwrap();
            p.space.w32(sp + 8, counter as u32).unwrap();
            p.space.w32(sp + 12, frequency as u32).unwrap();
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
    let base =
        p.vm.allocate(None, 16 * 4096, mem::RESERVE, prot::READWRITE)
            .unwrap()
            .0;
    p.vm.commit(base, 8 * 4096, prot::READWRITE).unwrap();
    base
}

#[test]
fn native_performance_counter_abi_uses_one_clock_and_optional_frequency() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtQueryPerformanceCounter", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let before = p.performance_clock.counter(p.start_time).unwrap();
        assert_eq!(call(p, &mut t, base + 4, base + 20), STATUS_SUCCESS);
        let after = p.performance_clock.counter(p.start_time).unwrap();
        assert!((before..=after).contains(&p.space.u64(base + 4).unwrap()));
        assert_eq!(
            p.space.u64(base + 20).unwrap(),
            p.performance_clock.frequency()
        );
        assert_eq!(
            p.space
                .u64(layout::KUSER_SHARED_DATA + layout::kuser::QPC_FREQUENCY)
                .unwrap(),
            p.performance_clock.frequency()
        );
        assert_eq!(call(p, &mut t, base + 4, 0), STATUS_SUCCESS);
        assert!(p.space.u64(base + 4).unwrap() >= before);
    }
}

#[test]
fn native_performance_counter_failed_frequency_retains_counter_and_guards() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtQueryPerformanceCounter", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let counter = 0x1234_5678_90AB_CDEF;
        p.performance_clock = PerformanceClock::Captured {
            counter,
            frequency: 24_000_000,
        };
        for (protection, expected, consumed) in [
            (prot::READONLY, STATUS_ACCESS_VIOLATION, false),
            (
                prot::READWRITE | prot::GUARD,
                STATUS_GUARD_PAGE_VIOLATION,
                true,
            ),
            (prot::READONLY | prot::GUARD, STATUS_ACCESS_VIOLATION, false),
        ] {
            p.vm.protect(base + 4096, 4096, prot::READWRITE).unwrap();
            p.space.w64(base, u64::MAX).unwrap();
            p.space.w64(base + 4096, u64::MAX).unwrap();
            p.vm.protect(base + 4096, 4096, protection).unwrap();
            assert_eq!(call(p, &mut t, base, base + 4096), expected);
            assert_eq!(p.space.u64(base).unwrap(), counter);
            assert_eq!(
                p.vm.query(base + 4096).unwrap().protect,
                if consumed {
                    prot::READWRITE
                } else {
                    protection
                }
            );
            p.vm.protect(base + 4096, 4096, prot::READWRITE).unwrap();
            assert_eq!(p.space.u64(base + 4096).unwrap(), u64::MAX);
        }
        p.space.w64(base, u64::MAX).unwrap();
        assert_eq!(
            call(p, &mut t, base + 1, base + 4096),
            STATUS_DATATYPE_MISALIGNMENT
        );
        assert_eq!(p.space.u64(base).unwrap(), u64::MAX);
        assert_eq!(
            call(p, &mut t, base, base + 1),
            STATUS_DATATYPE_MISALIGNMENT
        );
        assert_eq!(p.space.u64(base).unwrap(), counter);
    }
}

#[test]
fn native_performance_counter_aliases_and_cross_page_store_are_exact() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtQueryPerformanceCounter", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let counter = 0x1122_3344_5566_7788u64;
        let frequency = 24_000_000u64;
        p.performance_clock = PerformanceClock::Captured { counter, frequency };
        for offset in [0u64, 4, 8, 12] {
            p.vm.poke(base, &[0xA5; 24]).unwrap();
            assert_eq!(call(p, &mut t, base, base + offset), STATUS_SUCCESS);
            let mut expected = [0xA5; 24];
            expected[..8].copy_from_slice(&counter.to_le_bytes());
            expected[offset as usize..offset as usize + 8]
                .copy_from_slice(&frequency.to_le_bytes());
            assert_eq!(p.space.bytes(base, 24).unwrap(), expected);
        }
        p.vm.poke(base + 4092, &[0xA5; 8]).unwrap();
        p.vm.protect(base + 4096, 4096, prot::READONLY).unwrap();
        assert_eq!(call(p, &mut t, base + 4092, 0), STATUS_ACCESS_VIOLATION);
        assert_eq!(p.space.bytes(base + 4092, 8).unwrap(), [0xA5; 8]);
    }
}
