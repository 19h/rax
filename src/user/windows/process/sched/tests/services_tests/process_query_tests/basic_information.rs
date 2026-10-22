use super::*;

#[path = "basic_information/replay.rs"]
mod replay;

#[test]
fn native_process_basic_information_self_preserves_virtual_identity_and_layouts() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, scratch) = setup(arch);
        let p = process.state_mut();
        let sp = thread.cpu.sp();
        let pointer = arch.ptr_size() as u64;
        let basic = if arch == WinArch::X86 { 24 } else { 48 };
        let extended = if arch == WinArch::X86 { 32 } else { 64 };
        for length in [basic, extended] {
            p.space.wr(scratch, &[0xA5; 128]).unwrap();
            thread.cpu.set_sp(sp);
            arguments(
                p,
                &mut thread,
                arch.ptr(u64::MAX),
                0,
                scratch,
                length,
                scratch + 96,
            );
            assert_eq!(dispatch(p, &mut thread), Outcome::Continue);
            assert_eq!(thread.cpu.gpr(0), 0);
            let base = if length == extended {
                assert_eq!(
                    p.space.ptr(scratch, arch.ptr_size()).unwrap(),
                    u64::from(extended)
                );
                pointer
            } else {
                0
            };
            assert_eq!(p.space.u32(scratch + base).unwrap(), 0x103);
            assert_eq!(
                p.space
                    .ptr(scratch + base + pointer, arch.ptr_size())
                    .unwrap(),
                p.peb
            );
            assert_eq!(
                p.space
                    .ptr(scratch + base + 2 * pointer, arch.ptr_size())
                    .unwrap(),
                1
            );
            assert_eq!(
                p.space.u32(scratch + base + 3 * pointer).unwrap(),
                if arch == WinArch::X86 { 0xA5A5_A5A5 } else { 8 }
            );
            assert_eq!(
                p.space
                    .ptr(scratch + base + 4 * pointer, arch.ptr_size())
                    .unwrap(),
                u64::from(p.pid)
            );
            assert_eq!(
                p.space
                    .ptr(scratch + base + 5 * pointer, arch.ptr_size())
                    .unwrap(),
                0
            );
            if length == extended {
                assert_eq!(
                    p.space.u32(scratch + pointer + u64::from(basic)).unwrap(),
                    if arch == WinArch::X86 { 2 } else { 0 }
                );
            }
            assert_eq!(p.space.u32(scratch + 96).unwrap(), length);
            assert_eq!(
                p.space.u32(scratch + u64::from(length)).unwrap(),
                0xA5A5_A5A5
            );
            assert!(thread.frames.is_empty());
        }
    }
}
