//! Exact status/guard/caller-byte replay; only clock ticks are normalized.
use super::*;
use std::collections::BTreeMap;

const ARM64: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-performance-counter/native-performance-counter-arm64.log"
);
const X64: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-performance-counter/native-performance-counter-x64.log"
);
const X86: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-performance-counter/native-performance-counter-x86.log"
);
const X86_LAA: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-performance-counter/native-performance-counter-x86-laa.log"
);

fn fields(line: &str) -> BTreeMap<&str, &str> {
    line.split_ascii_whitespace()
        .filter_map(|field| field.split_once('='))
        .collect()
}

fn pointer(arch: WinArch, base: u64, kind: usize) -> u64 {
    match kind {
        0 => base + 64,
        1 => 0,
        2 => 1,
        3 => arch.ptr(u64::MAX),
        4 => base + 65,
        5 => base + 68,
        6 => base + 4096 + 64,
        7 => base + 8192 + 64,
        8 => base + 12288 + 64,
        9 => base + 16384 + 64,
        10 => base + 4092,
        11 => base + 8188,
        12 => base + 24572,
        13 => base + 32764,
        14 => base + 32768,
        _ => panic!("unknown captured pointer kind"),
    }
}

fn captured_counter(
    expected: &[u8],
    counter: u64,
    frequency: u64,
    base: u64,
    status: u32,
    before: u64,
    after: u64,
) -> u64 {
    let Some(offset) = counter
        .checked_sub(base)
        .and_then(|n| usize::try_from(n).ok())
    else {
        return before;
    };
    let Some(bytes) = expected.get(offset..offset.saturating_add(8)) else {
        return before;
    };
    let overwritten = |index: usize| {
        status == STATUS_SUCCESS
            && frequency != 0
            && frequency <= counter + index as u64
            && counter + (index as u64) < frequency + 8
    };
    let visible: Vec<_> = (0..8).filter(|&i| !overwritten(i)).collect();
    let candidate = match visible.as_slice() {
        [] => before,
        [0, 1, 2, 3] => {
            let low = u64::from(u32::from_le_bytes(bytes[..4].try_into().unwrap()));
            let value = (before & !0xFFFF_FFFF) | low;
            if value < before {
                value + (1 << 32)
            } else {
                value
            }
        }
        [4, 5, 6, 7] => {
            let high = u64::from(u32::from_le_bytes(bytes[4..].try_into().unwrap())) << 32;
            high.max(before)
        }
        [0, 1, 2, 3, 4, 5, 6, 7] => u64::from_le_bytes(bytes.try_into().unwrap()),
        _ => panic!("unexpected captured aligned alias"),
    };
    if !(before..=after).contains(&candidate) {
        // Rejected counter stores leave the canary intact. Choosing the
        // captured lower clock bound does not erase any expected caller byte.
        return before;
    }
    let candidate_bytes = candidate.to_le_bytes();
    assert!(visible.iter().all(|&i| candidate_bytes[i] == bytes[i]));
    candidate
}

#[test]
fn native_performance_counter_replays_all_1000_native_calls() {
    let mut total = 0;
    for (profile, arch, corpus) in [
        ("arm64", WinArch::Arm64, ARM64),
        ("x64", WinArch::X64, X64),
        ("x86", WinArch::X86, X86),
        ("x86-laa", WinArch::X86, X86_LAA),
    ] {
        let calibration = fields(corpus.lines().next().unwrap());
        let frequency_value = calibration["frequency"].parse().unwrap();
        let (mut process, mut thread) = fixture(arch, "NtQueryPerformanceCounter", SERVICE);
        let p = process.state_mut();
        let base = scratch(p);
        let rows: Vec<_> = corpus
            .lines()
            .filter(|line| line.starts_with("case="))
            .collect();
        assert_eq!(rows.len(), 250);
        for (ordinal, line) in rows.iter().enumerate() {
            let row = fields(line);
            assert_eq!(row["case"].parse::<usize>().unwrap(), ordinal);
            assert_eq!(row["escaped"], "00000000");
            let counter_kind = row["counter-kind"].parse().unwrap();
            let frequency_kind = row["frequency-kind"].parse().unwrap();
            let (counter, frequency) = match row["family"] {
                "matrix" => (
                    pointer(arch, base, counter_kind),
                    pointer(arch, base, frequency_kind),
                ),
                "alias" => (base + 64, base + 64 + frequency_kind as u64),
                "reverse-alias" => (base + 64 + counter_kind as u64, base + 64),
                _ => panic!("unknown captured family"),
            };
            let status = u32::from_str_radix(row["status"], 16).unwrap();
            let mut expected = vec![0xA5; 8 * 4096];
            for (&key, &value) in &row {
                if let Some(offset) = key.strip_prefix('b').and_then(|n| n.parse::<usize>().ok()) {
                    expected[offset] = u8::from_str_radix(value, 16).unwrap();
                }
            }
            let clock_value = captured_counter(
                &expected,
                counter,
                frequency,
                base,
                status,
                row["before"].parse().unwrap(),
                row["after"].parse().unwrap(),
            );
            p.performance_clock = PerformanceClock::Captured {
                counter: clock_value,
                frequency: frequency_value,
            };
            for page in 0..8 {
                p.vm.protect(base + page * 4096, 4096, prot::READWRITE)
                    .unwrap();
            }
            p.vm.poke(base, &vec![0xA5; 8 * 4096]).unwrap();
            for (page, protection) in [
                (1, prot::READONLY),
                (2, prot::NOACCESS),
                (3, prot::READWRITE | prot::GUARD),
                (4, prot::READONLY | prot::GUARD),
                (6, prot::READONLY),
                (7, prot::NOACCESS),
            ] {
                p.vm.protect(base + page * 4096, 4096, protection).unwrap();
            }
            assert_eq!(
                call(p, &mut thread, counter, frequency),
                status,
                "{profile} case{ordinal}"
            );
            for page in 0..8 {
                let expected_protection =
                    u32::from_str_radix(row[format!("p{page}").as_str()], 16).unwrap();
                assert_eq!(
                    p.vm.query(base + page * 4096).unwrap().protect,
                    expected_protection,
                    "{profile} case{ordinal} page{page}"
                );
                p.vm.protect(base + page * 4096, 4096, prot::READWRITE)
                    .unwrap();
            }
            let actual = p.space.bytes(base, 8 * 4096).unwrap();
            if let Some(offset) = actual.iter().zip(&expected).position(|(a, b)| a != b) {
                panic!(
                    "{profile} case{ordinal} byte{offset}: actual{:02X} expected{:02X}",
                    actual[offset], expected[offset]
                );
            }
            total += 1;
        }
    }
    assert_eq!(total, 1000);
}
