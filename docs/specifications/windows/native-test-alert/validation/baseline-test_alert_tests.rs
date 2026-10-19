//! Native no-argument alert query and owned queued-APC behavior.
use super::*;

const SERVICE: u32 = 0x78;

fn call(p: &mut Proc, t: &mut Thread) -> Outcome {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            for (index, value) in [0x1234_0004u32, 0x1234_0000, u32::MAX, u32::MAX]
                .into_iter()
                .enumerate()
            {
                p.space.w32(sp + index as u64 * 4, value).unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(1, 0x1234_0004);
            for index in [10, 2, 8, 9] {
                t.cpu.set_gpr(index, u64::MAX);
            }
        }
        WinArch::Arm64 => {
            for index in 0..8 {
                t.cpu.set_gpr(index, u64::MAX);
            }
        }
    }
    if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    }
}

#[test]
fn native_test_alert_empty_call_ignores_argument_slots_and_returns_native_status_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtTestAlert", SERVICE);
        let p = process.state_mut();
        let sp = t.cpu.sp();
        assert_eq!(call(p, &mut t), Outcome::Continue);
        assert_eq!(t.cpu.gpr(0), 0);
        assert_eq!(t.cpu.pc(), 0x1234_0004);
        assert_eq!(t.cpu.sp(), sp + if arch == WinArch::X86 { 4 } else { 0 });
        assert!(t.apcs.is_empty());
        assert!(t.frames.is_empty());
    }
}

#[test]
fn native_test_alert_pending_callbacks_enter_owned_scheduler_delivery_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtTestAlert", SERVICE);
        let p = process.state_mut();
        t.apcs.extend([(0x50000, 0x1111), (0x50100, 0x2222)]);
        assert_eq!(call(p, &mut t), Outcome::Park);
        assert_eq!(
            t.apcs.len(),
            2,
            "delivery starts on the next scheduler poll"
        );
        assert!(matches!(
            t.state,
            ThreadState::Waiting(sync::Wait::Sleep {
                alertable: true,
                ..
            })
        ));
        assert_eq!(t.frames.len(), 1);
    }
}
