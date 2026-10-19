//! Replay native input/status/guard records; wall-clock poll races are not goldens.
use super::event_tests::{handle, setup, status};
use super::*;
use std::collections::HashMap;

fn n(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}
fn owner(
    p: &mut Proc,
    t: &mut Thread,
    out: u64,
    kind: u64,
    attrs: u64,
    access: u64,
) -> (u64, Option<u64>) {
    if kind == 0 {
        p.space.w32(out + 32, 1).unwrap();
        assert_eq!(
            status(
                p,
                t,
                "NtCreateTimer2",
                &[
                    out,
                    if attrs & 2 != 0 { out + 32 } else { 0 },
                    0,
                    attrs,
                    access
                ]
            ),
            0
        );
        let h = handle(p, out);
        return (h, Some(h));
    }
    let object = match kind {
        1 => Object::Timer {
            manual: false,
            signaled: false,
            due: None,
            period_ms: 0,
        },
        2 => Object::Event {
            manual: false,
            signaled: 0,
        },
        3 => Object::Mutex {
            owner: None,
            count: 0,
            abandoned: false,
        },
        4 => return (0, None),
        5 => return (0x7fff_ffff, None),
        6 => return (p.arch.ptr(u64::MAX), None),
        7 => return (p.arch.ptr(u64::MAX - 1), None),
        _ => unreachable!(),
    };
    let h = u64::from(p.objects.insert(object));
    (h, Some(h))
}

fn input(p: &mut Proc, base: u64, mode: u64, data: &[u8]) -> u64 {
    p.vm.protect(base, 3 * PAGE_SIZE, prot::READWRITE).unwrap();
    p.space
        .wr(base, &vec![0xa5; 3 * PAGE_SIZE as usize])
        .unwrap();
    let mut target = base + 128;
    target += match mode {
        6 => 1,
        7 => 2,
        8 => 4,
        14 => 7,
        _ => 0,
    };
    if (9..=13).contains(&mode) {
        target = base + PAGE_SIZE - [1, 4, 7, 8, 15][mode as usize - 9];
    }
    if mode == 15 {
        target = base + PAGE_SIZE - 16;
    }
    p.space.wr(target, data).unwrap();
    match mode {
        3 => {
            p.vm.protect(base, PAGE_SIZE, prot::NOACCESS).unwrap();
        }
        4 => {
            p.vm.protect(base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                .unwrap();
        }
        5 => {
            p.vm.protect(base, PAGE_SIZE, prot::READONLY).unwrap();
        }
        9..=13 | 15 => {
            p.vm.protect(base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                .unwrap();
        }
        _ => {}
    }
    match mode {
        1 => 0,
        2 => 1,
        16 => p.arch.ptr(u64::MAX),
        _ => target,
    }
}
fn guard(p: &Proc, base: u64) -> u64 {
    u64::from(p.vm.query(base).unwrap().protect & prot::GUARD)
}

fn replay_values(arch: WinArch, text: &str) {
    let (mut process, mut t, out) = setup(arch);
    let p = process.state_mut();
    let before = (p.objects.handle_count(), p.objects.iter().count());
    let mut count = 0;
    for line in text
        .lines()
        .filter(|line| line.starts_with("SET ") || line.starts_with("CANCEL "))
    {
        let row: HashMap<_, _> = line
            .split_whitespace()
            .skip(1)
            .map(|part| part.split_once('=').unwrap())
            .collect();
        let (h, owned) = owner(
            p,
            &mut t,
            out,
            n(&row, "kind", 10),
            n(&row, "attributes", 16),
            n(&row, "access", 16),
        );
        let data = out + 128;
        p.space
            .w64(
                data,
                if line.starts_with("SET ") {
                    n(&row, "due", 16)
                } else {
                    (-3_000_000_000i64) as u64
                },
            )
            .unwrap();
        p.space
            .w64(
                data + 8,
                if line.starts_with("SET ") {
                    n(&row, "period", 16)
                } else {
                    0
                },
            )
            .unwrap();
        p.space
            .w32(
                data + 16,
                if line.starts_with("SET ") {
                    n(&row, "version", 16) as u32
                } else {
                    0
                },
            )
            .unwrap();
        p.space
            .w32(
                data + 20,
                if line.starts_with("SET ") {
                    n(&row, "reserved", 16) as u32
                } else {
                    0
                },
            )
            .unwrap();
        p.space
            .w64(
                data + 24,
                if line.starts_with("SET ") {
                    n(&row, "tolerance", 16)
                } else {
                    0
                },
            )
            .unwrap();
        let snapshot = p.space.bytes(data, 32).unwrap();
        if line.starts_with("SET ") {
            let actual = status(
                p,
                &mut t,
                "NtSetTimer2",
                &[
                    h,
                    if n(&row, "due-present", 10) != 0 {
                        data
                    } else {
                        0
                    },
                    if n(&row, "period-present", 10) != 0 {
                        data + 8
                    } else {
                        0
                    },
                    if n(&row, "params-present", 10) != 0 {
                        data + 16
                    } else {
                        0
                    },
                ],
            );
            assert_eq!(u64::from(actual), n(&row, "status", 16), "{arch} {line}");
        } else {
            assert_eq!(
                u64::from(status(p, &mut t, "NtSetTimer2", &[h, data, 0, data + 16])),
                n(&row, "arm", 16),
                "{arch} {line}"
            );
            assert_eq!(
                u64::from(status(
                    p,
                    &mut t,
                    "NtCancelTimer2",
                    &[h, n(&row, "parameter", 16)]
                )),
                n(&row, "status", 16),
                "{arch} {line}"
            );
        }
        assert_eq!(p.space.bytes(data, 32).unwrap(), snapshot);
        if let Some(h) = owned {
            p.objects.close(h).unwrap();
        }
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
        count += 1;
    }
    assert_eq!(count, 826);
}

fn replay_faults(arch: WinArch, text: &str) {
    let (mut process, mut t, out) = setup(arch);
    let p = process.state_mut();
    let bases: Vec<_> = (0..3)
        .map(|_| {
            p.vm.allocate(
                None,
                3 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0
        })
        .collect();
    let baseline = (p.objects.handle_count(), p.objects.iter().count());
    let mut pointers = [0; 3];
    let mut counts = [0; 2];
    for line in text
        .lines()
        .filter(|line| line.starts_with("FAULT ") || line.starts_with("CANCEL-FAULT "))
    {
        let row: HashMap<_, _> = line
            .split_whitespace()
            .skip(1)
            .map(|part| part.split_once('=').unwrap())
            .collect();
        let cancel = line.starts_with("CANCEL-");
        let (h, owned) = owner(
            p,
            &mut t,
            out,
            n(&row, "kind", 10),
            8,
            n(&row, "access", 16),
        );
        if cancel {
            let pointer = input(p, bases[2], n(&row, "params-mode", 10), &[0; 16]);
            assert_eq!(guard(p, bases[2]), n(&row, "guard-before", 10));
            assert_eq!(
                u64::from(status(p, &mut t, "NtCancelTimer2", &[h, pointer])),
                n(&row, "status", 16),
                "{arch} {line}"
            );
            assert_eq!(
                guard(p, bases[2]),
                n(&row, "guard-after", 10),
                "{arch} {line}"
            );
            counts[1] += 1;
        } else {
            if n(&row, "repeat", 10) == 0 {
                pointers[0] = input(
                    p,
                    bases[0],
                    n(&row, "due-mode", 10),
                    &(-3_000_000_000i64).to_le_bytes(),
                );
                pointers[1] = input(
                    p,
                    bases[1],
                    n(&row, "period-mode", 10),
                    &100_000i64.to_le_bytes(),
                );
                let mut params = [0u8; 16];
                params[..4].copy_from_slice(&(n(&row, "version", 16) as u32).to_le_bytes());
                pointers[2] = input(p, bases[2], n(&row, "params-mode", 10), &params);
            }
            for (index, key) in [
                "due-guard-before",
                "period-guard-before",
                "params-guard-before",
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(guard(p, bases[index]), n(&row, key, 10), "{arch} {line}");
            }
            let actual = status(
                p,
                &mut t,
                "NtSetTimer2",
                &[h, pointers[0], pointers[1], pointers[2]],
            );
            assert_eq!(u64::from(actual), n(&row, "status", 16), "{arch} {line}");
            for (index, key) in [
                "due-guard-after",
                "period-guard-after",
                "params-guard-after",
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(guard(p, bases[index]), n(&row, key, 10), "{arch} {line}");
            }
            counts[0] += 1;
        }
        if let Some(h) = owned {
            p.objects.close(h).unwrap();
        }
        assert_eq!(
            (p.objects.handle_count(), p.objects.iter().count()),
            baseline
        );
    }
    assert_eq!(counts, [3534, 272]);
}

#[test]
fn native_timer2_arm_replays_3304_native_value_and_access_records_four_profiles() {
    replay_values(
        WinArch::Arm64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-v1/native-timer2-arm-v1-arm64.log"
        ),
    );
    replay_values(
        WinArch::X64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-v1/native-timer2-arm-v1-x64.log"
        ),
    );
    replay_values(
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-v1/native-timer2-arm-v1-x86.log"
        ),
    );
    replay_values(
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-v1/native-timer2-arm-v1-x86-laa.log"
        ),
    );
}

#[test]
fn native_timer2_arm_replays_15224_native_fault_and_guard_records_four_profiles() {
    replay_faults(
        WinArch::Arm64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-fault-v2/native-timer2-arm-fault-v2-arm64.log"
        ),
    );
    replay_faults(
        WinArch::X64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-fault-v2/native-timer2-arm-fault-v2-x64.log"
        ),
    );
    replay_faults(
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-fault-v2/native-timer2-arm-fault-v2-x86.log"
        ),
    );
    replay_faults(
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-timer2-arm/native-fault-v2/native-timer2-arm-fault-v2-x86-laa.log"
        ),
    );
}
