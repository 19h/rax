//! Replay original native observations, not expectations derived from the model.
use super::*;
use std::collections::HashMap;

fn rows(text: &str) -> Vec<HashMap<&str, &str>> {
    text.lines()
        .filter(|line| line.starts_with("case="))
        .map(|line| {
            line.split_whitespace()
                .map(|part| part.split_once('=').unwrap())
                .collect()
        })
        .collect()
}
fn number(row: &HashMap<&str, &str>, field: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[field], radix).unwrap()
}
fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
fn snapshot(p: &Proc, address: u64, length: usize) -> Vec<u8> {
    let Some(info) = p.vm.query(address) else {
        return vec![];
    };
    if info.state != mem::COMMIT || info.protect & (prot::NOACCESS | prot::GUARD) != 0 {
        return vec![];
    }
    let available = (info.base + info.size).saturating_sub(address) as usize;
    p.space.bytes(address, length.min(available)).unwrap()
}

fn replay(arch: WinArch, text: &str, expected_rows: usize) -> (usize, usize) {
    let observations = rows(text);
    assert_eq!(observations.len(), expected_rows);
    let (mut process, mut t, _) = setup(arch);
    let p = process.state_mut();
    let output_base =
        p.vm.allocate(
            None,
            8 * PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    let attrs_base =
        p.vm.allocate(
            None,
            8 * PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    let mut replayed = 0;
    let mut excluded = 0;
    let initial_handles = p.objects.handle_count();
    let initial_objects = p.objects.iter().count();
    for row in observations {
        let mode = number(&row, "mode", 10) as u32;
        let access = number(&row, "access", 16);
        let expected_status = number(&row, "status", 16);
        // Non-null nested attributes require NT namespace/security/QoS policy;
        // the SYSTEM oracle's privileged grant is not guest authorization.
        if matches!(mode, 11..=13 | 24..=26) || (access & 0x01000000 != 0 && expected_status == 0) {
            excluded += 1;
            continue;
        }
        let count = number(&row, "count", 16);
        let flags = number(&row, "flags", 16) as u32;
        let length = number(&row, "attrs-length", 10) as u32;
        let offset = number(&row, "offset", 10);
        let repeat = number(&row, "repeat", 10);
        // Each repeat must observe the previous guard consumption and bytes.
        if repeat == 0 {
            for base in [output_base, attrs_base] {
                p.vm.protect(base, 8 * PAGE_SIZE, prot::READWRITE).unwrap();
                p.space
                    .wr(base, &vec![0xA5; (8 * PAGE_SIZE) as usize])
                    .unwrap();
            }
        }
        let mut out = output_base + 128;
        let mut oa = attrs_base + 128;
        if repeat == 0 {
            attrs(p, oa, length, flags, 0);
        }
        let mut use_attrs = mode >= 5;
        match mode {
            0 => {}
            1 => out = 0,
            2 => out += offset,
            3 | 16 => {
                if repeat == 0 {
                    p.vm.protect(output_base, PAGE_SIZE, prot::READONLY)
                        .unwrap();
                }
            }
            4 | 17 => {
                if repeat == 0 {
                    p.vm.protect(output_base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
            }
            5 => {}
            6 => oa = 1,
            7 | 27 => {
                oa += offset;
                if repeat == 0 {
                    attrs(p, oa, length, flags, 0);
                }
                if mode == 27 {
                    out = 0;
                }
            }
            8 | 18 => {
                if repeat == 0 {
                    p.vm.protect(attrs_base, PAGE_SIZE, prot::NOACCESS).unwrap();
                }
                if mode == 18 {
                    out = 0;
                }
            }
            9 | 19 => {
                if repeat == 0 {
                    p.vm.protect(attrs_base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                if mode == 19 {
                    out = 0;
                }
            }
            10 => {
                if repeat == 0 {
                    attrs(p, oa, length, flags, offset);
                }
            }
            14 => {
                out = 0;
                oa = 1;
            }
            15 => out = 0,
            20 => out = oa,
            21 => out = oa + offset,
            22 => {
                oa = attrs_base + PAGE_SIZE - offset;
                attrs(p, oa, length, flags, 0);
                p.vm.protect(attrs_base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            23 => {
                out = output_base + PAGE_SIZE - offset;
                p.vm.protect(output_base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
                use_attrs = false;
            }
            _ => panic!("unregistered oracle role {mode}"),
        }
        if !use_attrs {
            oa = 0;
        }
        let backing = if matches!(mode, 20 | 21) {
            attrs_base + 128
        } else {
            output_base + 128
        };
        assert_eq!(
            call(p, &mut t, [out, access, oa, count]),
            Outcome::Continue,
            "{arch} case{}",
            row["case"]
        );
        assert_eq!(t.cpu.gpr(0), expected_status, "{arch} case{}", row["case"]);
        assert_eq!(number(&row, "exception", 16), 0);
        let successful = expected_status == 0;
        let handle = if successful {
            p.space.ptr(out, arch.ptr_size()).unwrap()
        } else {
            0
        };
        if successful {
            let native_handle = number(&row, "handle", 16);
            assert!(native_handle != 0 && native_handle % 4 == 0);
            assert_eq!(number(&row, "handle-bytes", 10), arch.ptr_size());
            assert_eq!(
                p.objects.access(handle),
                Some(number(&row, "grant", 16) as u32)
            );
            assert_eq!(
                p.objects.flags(handle),
                Some(number(&row, "handle-flags", 10) as u32)
            );
            assert!(
                matches!(p.objects.get(handle), Some(Object::CompletionPort{queue, concurrency}) if queue.is_empty() && u64::from(*concurrency) == count.max(1))
            );
            assert_eq!(p.objects.handle_count(), initial_handles + 1);
            assert_eq!(p.objects.iter().count(), initial_objects + 1);
        }
        for (name, address, size) in [
            ("output", out, 96),
            ("output-backing", backing, 96),
            ("attributes", oa, if arch.is64() { 48 } else { 24 }),
        ] {
            let mut expected = bytes(row[name]);
            assert_eq!(
                expected.len() as u64,
                number(&row, &format!("{name}-captured"), 10)
            );
            // Normalize only the native kernel's newly allocated HANDLE field.
            // All captured bytes, padding, aliases and suffixes remain exact.
            if successful
                && out >= address
                && out - address + arch.ptr_size() <= expected.len() as u64
            {
                let at = (out - address) as usize;
                let width = arch.ptr_size() as usize;
                assert_eq!(
                    &expected[at..at + width],
                    &number(&row, "handle", 16).to_le_bytes()[..width]
                );
                expected[at..at + width].copy_from_slice(&handle.to_le_bytes()[..width]);
            }
            assert_eq!(
                snapshot(p, address, size),
                expected,
                "{arch} case{} {name}",
                row["case"]
            );
        }
        for (base, key) in [(output_base, "output-guard"), (attrs_base, "attrs-guard")] {
            assert_eq!(
                p.vm.query(base).unwrap().protect & prot::GUARD != 0,
                number(&row, key, 10) != 0,
                "{arch} case{} {key}",
                row["case"]
            );
        }
        if successful {
            p.objects.close(handle).unwrap();
        }
        assert_eq!(
            number(&row, "handles-before", 10),
            number(&row, "handles-after", 10)
        );
        assert_eq!(p.objects.handle_count(), initial_handles);
        assert_eq!(p.objects.iter().count(), initial_objects);
        replayed += 1;
    }
    (replayed, excluded)
}

macro_rules! capture {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-io-completion-create/",
            $file
        ))
    };
}

#[test]
fn native_io_completion_create_replays_independent_constructor_and_root_captures() {
    let mut replayed = 0;
    let mut excluded = 0;
    for (arch, original, roots) in [
        (
            WinArch::Arm64,
            capture!("native-io-completion-create-probe-arm64.log"),
            capture!("native-io-completion-root-probe-arm64.log"),
        ),
        (
            WinArch::X64,
            capture!("native-io-completion-create-probe-x64.log"),
            capture!("native-io-completion-root-probe-x64.log"),
        ),
        (
            WinArch::X86,
            capture!("native-io-completion-create-probe-x86.log"),
            capture!("native-io-completion-root-probe-x86.log"),
        ),
        (
            WinArch::X86,
            capture!("native-io-completion-create-probe-x86-laa.log"),
            capture!("native-io-completion-root-probe-x86-laa.log"),
        ),
    ] {
        let (a, b) = replay(arch, original, 303);
        let (c, d) = replay(arch, roots, 21);
        replayed += a + c;
        excluded += b + d;
    }
    assert_eq!(replayed, 1184);
    assert_eq!(excluded, 112);
    println!(
        "native I/O completion captures: {replayed} exact replays; {excluded} explicit nested-policy/privileged-grant exclusions"
    );
}
