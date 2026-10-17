//! Replay native statuses, guards and caller bytes; normalize only clock ticks.
use super::*;
use std::collections::BTreeMap;

const ARM64: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-system-time/native/native-system-time-v2-probe-arm64.log"
);
const X64: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-system-time/native/native-system-time-v2-probe-x64.log"
);
const X86: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-system-time/native/native-system-time-v2-probe-x86.log"
);
const X86_LAA: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-system-time/native/native-system-time-v2-probe-x86-laa.log"
);

fn fields(line: &str) -> BTreeMap<&str, &str> {
    line.split_ascii_whitespace()
        .filter_map(|s| s.split_once('='))
        .collect()
}
fn hex(s: &str) -> u64 {
    u64::from_str_radix(s, 16).unwrap()
}

#[test]
fn native_system_time_replays_all_820_native_calls() {
    let mut total = 0;
    for (profile, arch, corpus) in [
        ("arm64", WinArch::Arm64, ARM64),
        ("x64", WinArch::X64, X64),
        ("x86", WinArch::X86, X86),
        ("x86-laa", WinArch::X86, X86_LAA),
    ] {
        let header = fields(corpus.lines().next().unwrap());
        assert_eq!(header["profile"], profile);
        let (mut process, mut thread) = fixture(arch, "NtQuerySystemTime", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let last_error = thread.teb + layout::offsets(arch).teb_last_error;
        let rows: Vec<_> = corpus
            .lines()
            .filter(|l| l.starts_with("case ") || l.starts_with("invalid "))
            .collect();
        assert_eq!(rows.len(), 205);
        for (ordinal, line) in rows.iter().enumerate() {
            let row = fields(line);
            assert_eq!(row["ordinal"].parse::<usize>().unwrap(), ordinal);
            assert_eq!(row["exception"], "00000000");
            let status = hex(row["status"]) as u32;
            p.vm.protect(base, 8192, prot::READWRITE).unwrap();
            p.vm.poke(base, &[0xA5; 8192]).unwrap();
            p.space.w32(last_error, 0x4A2E19F7).unwrap();
            let invalid = line.starts_with("invalid ");
            let offset = if invalid {
                0
            } else {
                row["offset"].parse::<usize>().unwrap()
            };
            let pointer = if invalid {
                match row["mode"] {
                    "0" => 0,
                    "1" => 1,
                    "2" => arch.ptr(u64::MAX),
                    "3" => arch.ptr(u64::MAX & !3),
                    "4" => arch.ptr(u64::MAX & !4095),
                    _ => panic!("native invalid pointer axis"),
                }
            } else {
                p.vm.protect(base, 4096, hex(row["p0"]) as u32).unwrap();
                p.vm.protect(base + 4096, 4096, hex(row["p1"]) as u32)
                    .unwrap();
                base + offset as u64
            };
            let before = now();
            assert_eq!(
                call(p, &mut thread, pointer),
                status,
                "{profile} case {ordinal}"
            );
            let after = now();
            assert_eq!(
                p.space.u32(last_error).unwrap(),
                hex(row["last-error"]) as u32
            );
            if !invalid {
                for (page, key) in [(0, "after0"), (1, "after1")] {
                    assert_eq!(
                        p.vm.query(base + page * 4096).unwrap().protect,
                        hex(row[key]) as u32,
                        "{profile} case {ordinal} {key}"
                    );
                }
            }
            p.vm.protect(base, 8192, prot::READWRITE).unwrap();
            let mut expected = vec![0xA5; 8192];
            if !invalid {
                let raw = row["bytes"];
                assert_eq!(raw.len(), 32);
                let mut captured = (0..16)
                    .map(|i| u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16).unwrap())
                    .collect::<Vec<_>>();
                if status == STATUS_SUCCESS {
                    let native_value = u64::from_le_bytes(captured[..8].try_into().unwrap());
                    assert!((hex(row["before"])..=hex(row["after"])).contains(&native_value));
                    let actual = p.space.u64(pointer).unwrap();
                    assert!((before..=after).contains(&actual));
                    captured[..8].copy_from_slice(&actual.to_le_bytes());
                }
                expected[offset..offset + 16].copy_from_slice(&captured);
            }
            assert_eq!(
                p.space.bytes(base, 8192).unwrap(),
                expected,
                "{profile} case {ordinal} caller bytes"
            );
            total += 1;
        }
    }
    assert_eq!(total, 820);
}
