use super::*;

const LOGS: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-matrix-v5-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-matrix-v5-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-matrix-v5-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-matrix-v5-four-views-x86-laa.log"
    )),
];
const PRIORITY: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v6-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v6-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v6-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v6-four-views-x86-laa.log"
    )),
];
const GRANT_PRIORITY: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v12-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v12-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v12-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-alpc-private-priority-v12-four-views-x86-laa.log"
    )),
];
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_ascii_whitespace()
        .find_map(|word| word.strip_prefix(key))
        .unwrap()
}
fn hexadecimal(line: &str, key: &str) -> u32 {
    u32::from_str_radix(field(line, key), 16).unwrap()
}
fn one(arch: WinArch, case: usize, priority: u8, oracle: &str) {
    let (mut process, mut t) = fixture(arch, "NtAlpcSendWaitReceivePort", SERVICE);
    let p = process.state_mut();
    let base =
        p.vm.allocate(None, 12288, mem::RESERVE | mem::COMMIT, prot::READWRITE)
            .unwrap()
            .0;
    let send = base;
    let receive = base + 4096;
    let length = base + 8192;
    p.space.wr(base, &[0xA5; 12288]).unwrap();
    let header = if arch.is64() { 40 } else { 24 };
    p.space.wr(send, &vec![0; header]).unwrap();
    p.space.w16(send, 48).unwrap();
    p.space.w16(send + 2, header as u16 + 48).unwrap();
    p.space.wr(send + header as u64, &[0xC5; 48]).unwrap();
    p.space.wptr(length, arch.ptr_size(), 952).unwrap();
    let object = if priority == 2 && case == 46 {
        Object::Event {
            manual: false,
            signaled: 0,
        }
    } else {
        Object::LpcPort(LpcPort::new(None))
    };
    let id = p.objects.create(object);
    let access = if priority == 2 {
        if case == 50 { 1 } else { 0 }
    } else {
        0x1F0001
    };
    let port = p.objects.open_access(id, false, access).unwrap();
    let mut args = [port.into(), 0x20000, send, 0, send, length, 0, 0];
    if priority == 2 {
        match case {
            40 => args[2] = 1,
            41 => args[4] = 1,
            42 => args[5] = 1,
            43 => args[7] = 1,
            44 => args[1] = u32::MAX.into(),
            45 => args[2] = 0,
            46 => {}
            47 => {
                args[1] = u32::MAX.into();
                args[4] = 1;
            }
            48 => {
                p.vm.protect(length, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            49 => {
                args[4] = receive;
                p.vm.protect(receive, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            50 => {
                args[7] = 1;
                p.vm.protect(length, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            _ => panic!("unknown grant priority case"),
        }
    } else if priority == 1 {
        match case {
            40 => {
                args[0] = 0;
                args[2] = 1;
            }
            41 => {
                args[0] = 0;
                args[4] = 1;
            }
            42 => {
                args[0] = 0;
                args[5] = 1;
            }
            43 => {
                args[0] = 0;
                args[7] = 1;
            }
            44 => {
                args[0] = 0;
                args[6] = 1;
            }
            45 => {
                args[0] = 0;
                args[3] = 1;
            }
            46 => {
                args[0] = 0;
                args[1] = u32::MAX.into();
            }
            47 => {
                args[2] = 1;
                args[4] = 1;
            }
            48 => {
                args[2] = 1;
                args[5] = 1;
            }
            49 => {
                args[2] = 1;
                args[7] = 1;
            }
            50 => {
                args[2] = 1;
                args[6] = 1;
            }
            51 => {
                args[4] = 1;
                args[5] = 1;
            }
            52 => {
                args[4] = 1;
                args[7] = 1;
            }
            53 => {
                args[4] = 1;
                args[6] = 1;
            }
            54 => {
                args[5] = 1;
                args[7] = 1;
            }
            55 => {
                args[5] = 1;
                args[6] = 1;
            }
            56 => {
                args[6] = 1;
                args[7] = 1;
            }
            57 => {
                args[2] = 0;
                args[4] = 1;
            }
            58 => {
                args[2] = 0;
                args[5] = 1;
            }
            59 => {
                args[2] = 0;
                args[7] = 1;
            }
            60 => {
                args[2] = 0;
                args[1] = u32::MAX.into();
            }
            61 => {
                args[4] = 0;
                args[5] = 1;
            }
            62 => {
                args[4] = 0;
                args[7] = 1;
            }
            63 => {
                args[4] = 0;
                args[6] = 1;
            }
            64 => {
                args[1] = u32::MAX.into();
                args[4] = 1;
            }
            65 => {
                args[1] = u32::MAX.into();
                args[5] = 1;
            }
            66 => {
                args[1] = u32::MAX.into();
                args[7] = 1;
            }
            67 => {
                args[1] = u32::MAX.into();
                args[6] = 1;
            }
            68 => {
                p.vm.protect(send, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
                p.vm.protect(length, 4096, prot::READONLY).unwrap();
            }
            69 => {
                args[4] = receive;
                p.vm.protect(send, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
                p.vm.protect(receive, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            70 => {
                args[7] = 1;
                p.vm.protect(send, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            71 => {
                args[7] = 1;
                p.vm.protect(length, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            _ => panic!("unknown priority case"),
        }
    } else {
        match case {
            1 => args[2] = 0,
            2 => args[2] = 1,
            4 => args[4] = 0,
            5 => args[4] = 1,
            6 => args[4] = receive + 1,
            8 => args[5] = 1,
            16 => p.space.w16(send + 2, header as u16 - 1).unwrap(),
            18 => p.space.w16(send, 49).unwrap(),
            19 => p.space.w16(send + 2, 1025).unwrap(),
            20 => args[0] = 0,
            21 => args[0] = arch.ptr(u64::MAX),
            22 => {
                let id = p.objects.create(Object::Event {
                    manual: false,
                    signaled: 0,
                });
                args[0] = p.objects.open_access(id, false, u32::MAX).unwrap().into();
            }
            25 => args[1] = u32::MAX.into(),
            27 => args[6] = 1,
            28 => args[7] = 1,
            31 => {
                args[4] = receive;
                p.vm.protect(receive, 4096, prot::READONLY).unwrap();
            }
            32 => {
                p.vm.protect(send, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            33 => {
                args[4] = receive;
                p.vm.protect(receive, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            34 => {
                p.vm.protect(length, 4096, prot::READWRITE | prot::GUARD)
                    .unwrap();
            }
            37 => {
                args[4] = receive;
                p.vm.protect(receive, 4096, prot::READONLY | prot::GUARD)
                    .unwrap();
            }
            _ => panic!("unknown matrix case"),
        }
    }
    let error_offset = crate::user::windows::layout::offsets(arch).teb_last_error;
    p.space.w32(t.teb + error_offset, 0x12345678).unwrap();
    let mut original = vec![0; 12288];
    p.space.read_raw(base, &mut original).unwrap();
    let before = (
        p.vm.committed_bytes(),
        p.objects.handle_count(),
        p.objects.iter().count(),
        p.rng,
    );
    assert_eq!(
        invoke(p, &mut t, &args),
        Outcome::Continue,
        "{arch} case{case} priority{priority}"
    );
    assert_eq!(
        t.cpu.gpr(0),
        u64::from(hexadecimal(oracle, "status=")),
        "{arch} case{case} priority{priority}"
    );
    assert_eq!(hexadecimal(oracle, "exception="), 0);
    assert_eq!(
        p.space.u32(t.teb + error_offset).unwrap(),
        hexadecimal(oracle, "last-error=")
    );
    let mut actual = vec![0; 12288];
    p.space.read_raw(base, &mut actual).unwrap();
    assert_eq!(actual, original, "{arch} case{case} bytes changed");
    for (address, key) in [
        (send, "send-after="),
        (receive, "receive-after="),
        (length, "length-after="),
    ] {
        assert_eq!(
            p.vm.query(address).unwrap().protect,
            hexadecimal(oracle, key),
            "{arch} case{case} {key}"
        );
    }
    assert_eq!(
        (
            p.vm.committed_bytes(),
            p.objects.handle_count(),
            p.objects.iter().count(),
            p.rng
        ),
        before
    );
}
#[test]
fn native_alpc_pre_peer_faults_replay_four_complete_execution_views() {
    let mut rows = 0;
    for (index, arch) in [WinArch::Arm64, WinArch::X64, WinArch::X86, WinArch::X86]
        .into_iter()
        .enumerate()
    {
        for case in [
            1, 2, 4, 5, 6, 8, 16, 18, 19, 20, 21, 22, 25, 27, 28, 31, 32, 33, 34, 37,
        ] {
            if arch == WinArch::X86 && [16, 18, 19].contains(&case) {
                continue;
            }
            let prefix = format!("CASE id={case} ");
            let oracle = LOGS[index]
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap();
            one(arch, case, 0, oracle);
            rows += 1;
        }
        for case in 40..72 {
            let prefix = format!("CASE id={case} ");
            let oracle = PRIORITY[index]
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap();
            one(arch, case, 1, oracle);
            rows += 1;
        }
        for case in 40..51 {
            let prefix = format!("CASE id={case} ");
            let oracle = GRANT_PRIORITY[index]
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap();
            let prefix = format!("GRANT case={case} ");
            let grant = GRANT_PRIORITY[index]
                .lines()
                .find(|line| line.starts_with(&prefix))
                .unwrap();
            assert_eq!(hexadecimal(grant, "status="), 0);
            assert_eq!(
                hexadecimal(grant, "granted="),
                if case == 50 { 1 } else { 0 }
            );
            one(arch, case, 2, oracle);
            rows += 1;
        }
    }
    assert_eq!(rows, 246);
}
