use super::*;

const LOGS: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-limits-v9-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-limits-v9-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-limits-v9-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-limits-v9-four-views-x86-laa.log"
    )),
];
fn status(index: usize, case: usize) -> u32 {
    let prefix = format!("CASE id={case} ");
    let line = LOGS[index]
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap();
    let field = line
        .split_ascii_whitespace()
        .find_map(|word| word.strip_prefix("status="))
        .unwrap();
    u32::from_str_radix(field, 16).unwrap()
}
fn setup(
    arch: WinArch,
    access: u32,
) -> (
    super::super::super::super::super::WindowsProcess,
    Thread,
    u64,
    u32,
) {
    let (mut process, t) = fixture(arch, "NtAlpcSendWaitReceivePort", SERVICE);
    let p = process.state_mut();
    let base =
        p.vm.allocate(None, 4096, mem::RESERVE | mem::COMMIT, prot::READWRITE)
            .unwrap()
            .0;
    p.space.wr(base, &[0; 256]).unwrap();
    let header = if arch.is64() { 40 } else { 24 };
    p.space.w16(base, 48).unwrap();
    p.space.w16(base + 2, header + 48).unwrap();
    p.space.wptr(base + 128, arch.ptr_size(), 952).unwrap();
    let object = p.objects.create(Object::LpcPort(LpcPort::new(None)));
    let handle = p.objects.open_access(object, false, access).unwrap();
    (process, t, base, handle)
}

#[test]
fn native_alpc_port_connect_grant_denial_replays_all_execution_views_without_publication() {
    for (index, arch) in [WinArch::Arm64, WinArch::X64, WinArch::X86, WinArch::X86]
        .into_iter()
        .enumerate()
    {
        for (case, access) in [(53, 0), (55, 0x100000), (56, 0x20000), (58, 0x2000000)] {
            let (mut process, mut t, base, handle) = setup(arch, access);
            let p = process.state_mut();
            let before = p.space.bytes(base, 256).unwrap();
            let resources = (p.vm.committed_bytes(), p.objects.handle_count(), p.rng);
            assert_eq!(
                invoke(
                    p,
                    &mut t,
                    &[handle.into(), 0x20000, base, 0, base, base + 128, 0, 0]
                ),
                Outcome::Continue
            );
            assert_eq!(
                t.cpu.gpr(0),
                u64::from(status(index, case)),
                "{arch} case{case}"
            );
            assert_eq!(p.space.bytes(base, 256).unwrap(), before);
            assert_eq!(
                (p.vm.committed_bytes(), p.objects.handle_count(), p.rng),
                resources
            );
        }
    }
}

#[test]
fn native_alpc_word_length_wrap_replays_native_and_wow_validation_without_body_access() {
    for (index, arch) in [WinArch::Arm64, WinArch::X64, WinArch::X86, WinArch::X86]
        .into_iter()
        .enumerate()
    {
        for case in [48, 49] {
            let (mut process, mut t, base, handle) = setup(arch, 0x1F0001);
            let p = process.state_mut();
            p.space.w16(base, 0xFFFF).unwrap();
            if case == 49 {
                p.space
                    .w16(base + 2, if arch.is64() { 40 } else { 24 })
                    .unwrap();
            }
            let before = p.space.bytes(base, 256).unwrap();
            assert_eq!(
                invoke(
                    p,
                    &mut t,
                    &[handle.into(), 0x20000, base, 0, base, base + 128, 0, 0]
                ),
                Outcome::Continue
            );
            assert_eq!(
                t.cpu.gpr(0),
                u64::from(status(index, case)),
                "{arch} case{case}"
            );
            assert_eq!(p.space.bytes(base, 256).unwrap(), before);
        }
    }
}
