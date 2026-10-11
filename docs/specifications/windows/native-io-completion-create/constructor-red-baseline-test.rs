//! Constructor behavioral baseline on unmodified parent implementation.
use super::*;
const SERVICE: u32 = 0x73;

fn arguments(p: &mut Proc, t: &mut Thread, args: [u64; 4]) {
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
        }
        WinArch::Arm64 => {
            for (index, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(index, value);
            }
        }
    }
}
fn call(p: &mut Proc, t: &mut Thread, args: [u64; 4]) -> Outcome {
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
    let (mut process, t) = fixture(arch, "NtCreateIoCompletion", SERVICE);
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
fn native_io_completion_create_constructs_real_object_and_closes_without_leak_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let handles = p.objects.handle_count();
        let objects = p.objects.iter().count();
        assert_eq!(call(p, &mut t, [out, 0x1F0003, 0, 4]), Outcome::Continue);
        assert_eq!(t.cpu.gpr(0), 0);
        let handle = p.space.ptr(out, arch.ptr_size()).unwrap();
        assert!(
            matches!(p.objects.get(handle),Some(Object::CompletionPort{queue,..}) if queue.is_empty())
        );
        assert_eq!(p.objects.access(handle), Some(0x1F0003));
        assert_eq!(p.objects.flags(handle), Some(0));
        assert_eq!(p.objects.handle_count(), handles + 1);
        assert_eq!(p.objects.iter().count(), objects + 1);
        assert_eq!(
            p.space
                .bytes(out + arch.ptr_size(), 16 - arch.ptr_size() as usize)
                .unwrap(),
            vec![0xA5; 16 - arch.ptr_size() as usize]
        );
        assert!(p.objects.close(handle).is_ok());
        assert_eq!(p.objects.handle_count(), handles);
        assert_eq!(p.objects.iter().count(), objects);
    }
}
