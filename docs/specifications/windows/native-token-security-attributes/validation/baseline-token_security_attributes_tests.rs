//! Installed-kernel security-attribute argument and guard ordering.
use super::{Outcome, PAGE_SIZE, Proc, Thread, WinArch, fixture, handle_stop, mem, prot, stop};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::{
    STATUS_ACCESS_VIOLATION, STATUS_DATATYPE_MISALIGNMENT, STATUS_GUARD_PAGE_VIOLATION,
    STATUS_INVALID_PARAMETER,
};

const SERVICE: u32 = 0x167;

fn call(p: &mut Proc, t: &mut Thread, args: [u64; 6]) -> Outcome {
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
            for (register, value) in [10, 2, 8, 9].into_iter().zip(args[..4].iter().copied()) {
                t.cpu.set_gpr(register, value);
            }
            p.space.w64(sp + 40, args[4]).unwrap();
            p.space.w64(sp + 48, args[5]).unwrap();
        }
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
    }
    let result = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    };
    if result == Outcome::Continue {
        assert_eq!(t.cpu.pc(), 0x1234_0004);
        assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
        assert!(t.frames.is_empty());
    }
    t.cpu.set_sp(sp);
    result
}

#[test]
fn native_token_security_attributes_rejects_inconsistent_name_array_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtQuerySecurityAttributesToken", SERVICE);
        let p = process.state_mut();
        let returned = t.cpu.sp() - 64;
        p.space.w32(returned, 0x11223344).unwrap();
        let handle = arch.ptr(u64::MAX - 3);
        assert_eq!(
            call(p, &mut t, [handle, 0, 1, 0, 0, returned]),
            Outcome::Continue
        );
        assert_eq!(t.cpu.gpr(0), u64::from(STATUS_INVALID_PARAMETER));
        assert_eq!(p.space.u32(returned).unwrap(), 0x11223344);
    }
}

#[test]
fn native_token_security_attributes_name_capture_and_output_guard_priority_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtQuerySecurityAttributesToken", SERVICE);
        let p = process.state_mut();
        let returned = t.cpu.sp() - 64;
        let output =
            p.vm.allocate(None, PAGE_SIZE, mem::RESERVE | mem::COMMIT, prot::READWRITE)
                .unwrap()
                .0;
        p.space.w32(returned, 0x11223344).unwrap();
        p.vm.protect(output, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        let handle = arch.ptr(u64::MAX - 3);
        assert_eq!(
            call(p, &mut t, [handle, 1, 2, output, 816, returned]),
            Outcome::Continue
        );
        assert_eq!(
            t.cpu.gpr(0),
            u64::from(if arch == WinArch::X86 {
                STATUS_ACCESS_VIOLATION
            } else {
                STATUS_GUARD_PAGE_VIOLATION
            })
        );
        assert_eq!(
            p.vm.query(output).unwrap().protect,
            prot::READWRITE | if arch == WinArch::X86 { prot::GUARD } else { 0 }
        );
        assert_eq!(p.space.u32(returned).unwrap(), 0x11223344);
    }
}

#[test]
fn native_token_security_attributes_returned_alignment_precedes_handle_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtQuerySecurityAttributesToken", SERVICE);
        let p = process.state_mut();
        assert_eq!(
            call(p, &mut t, [0x13572468, 0, 0, 0, 0, 1]),
            Outcome::Continue
        );
        assert_eq!(t.cpu.gpr(0), u64::from(STATUS_DATATYPE_MISALIGNMENT));
    }
}
