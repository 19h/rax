//! Current-process reads through the real kernel service dispatch seam.

use super::*;

fn setup_read(
    arch: WinArch,
) -> (
    super::super::super::super::super::WindowsProcess,
    Thread,
    u64,
) {
    let (mut process, thread) = fixture(arch, "NtReadVirtualMemory", 0x23);
    let p = process.state_mut();
    let base =
        p.vm.allocate(
            None,
            PAGE_SIZE * 3,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    (process, thread, base)
}

#[test]
fn native_read_virtual_memory_self_copies_bytes_and_pointer_width_count_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup_read(arch);
        let p = process.state_mut();
        let source = base + 32;
        let output = base + 64;
        let returned = base + 128;
        let bytes = [0x15, 0x26, 0x37, 0x48, 0x59, 0x6A, 0x7B, 0x8C];
        p.space.wr(source, &bytes).unwrap();
        p.space.wr(output, &[0xA5; 16]).unwrap();
        p.space.wr(returned, &[0xD3; 16]).unwrap();
        arguments(
            p,
            &mut thread,
            &[arch.ptr(u64::MAX), source, output, 8, returned],
        );
        assert_eq!(dispatch(p, &mut thread), Outcome::Continue, "{arch}");
        assert_eq!(thread.cpu.gpr(0), 0, "{arch}");
        assert_eq!(
            p.space.bytes(output, 16).unwrap(),
            [&bytes[..], &[0xA5; 8]].concat()
        );
        assert_eq!(p.space.ptr(returned, arch.ptr_size()).unwrap(), 8);
        assert_eq!(p.space.u8(returned + arch.ptr_size()).unwrap(), 0xD3);
        assert!(thread.frames.is_empty());
    }
}
