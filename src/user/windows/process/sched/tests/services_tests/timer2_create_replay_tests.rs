//! Replays independent native byte/status/fault observations, including ID aliases.
use super::*;
use std::collections::HashMap;

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
fn attributes(p: &Proc, at: u64, length: u32, flags: u32, root: u64) {
    p.space
        .wr(at, &vec![0; if p.arch.is64() { 48 } else { 24 }])
        .unwrap();
    p.space.w32(at, length).unwrap();
    if p.arch.is64() {
        p.space.w64(at + 8, root).unwrap();
        p.space.w32(at + 24, flags).unwrap();
    } else {
        p.space.w32(at + 4, root as u32).unwrap();
        p.space.w32(at + 12, flags).unwrap();
    }
}
fn replay(arch: WinArch, text: &str, expected_rows: usize) -> (usize, usize) {
    let observations: Vec<_> = text
        .lines()
        .filter(|l| l.starts_with("case="))
        .map(|l| {
            l.split_whitespace()
                .map(|p| p.split_once('=').unwrap())
                .collect::<HashMap<_, _>>()
        })
        .collect();
    assert_eq!(observations.len(), expected_rows);
    let (mut process, mut t, _) = setup(arch);
    let p = process.state_mut();
    let allocate = |p: &mut Proc| {
        p.vm.allocate(
            None,
            8 * PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0
    };
    let output_base = allocate(p);
    let attrs_base = allocate(p);
    let extra_base = allocate(p);
    let id_base = allocate(p);
    let before_handles = p.objects.handle_count();
    let before_objects = p.objects.iter().count();
    let mut checked = 0;
    let mut excluded = 0;
    for row in observations {
        let n = |key: &str, radix| number(&row, key, radix);
        let access = n("access", 16);
        let expected_status = n("status", 16);
        // SACL privilege policy is an explicit boundary, independent of native
        // SYSTEM/reduced-token authority. Earlier parameter/fault paths replay.
        if access & 0x01000000 != 0 && matches!(expected_status, 0 | 0xC0000061) {
            excluded += 1;
            continue;
        }
        let mode = n("mode", 10);
        let offset = n("offset", 10);
        let repeat = n("repeat", 10);
        let id_mode = n("id-mode", 10);
        let id_value = n("id-value", 16) as u32;
        let flags = n("flags", 16) as u32;
        let length = n("attrs-length", 10) as u32;
        if repeat == 0 {
            for base in [output_base, attrs_base, extra_base, id_base] {
                p.vm.protect(base, 8 * PAGE_SIZE, prot::READWRITE).unwrap();
                p.space
                    .wr(base, &vec![0xA5; (8 * PAGE_SIZE) as usize])
                    .unwrap();
            }
        }
        let mut out = output_base + 128;
        let mut oa = attrs_base + 128;
        if repeat == 0 {
            attributes(p, oa, length, flags, 0);
            p.space.w32(id_base + 128, id_value).unwrap();
        }
        let mut use_attrs = mode >= 5;
        match mode {
            0 | 5 | 28 => {}
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
            6 => oa = 1,
            7 | 27 => {
                oa += offset;
                if repeat == 0 {
                    attributes(p, oa, length, flags, 0);
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
                    attributes(p, oa, length, flags, offset);
                }
            }
            11..=13 | 24..=26 => {
                if repeat == 0 {
                    let field = if arch.is64() {
                        match mode {
                            11 | 24 => 16,
                            12 | 26 => 32,
                            _ => 40,
                        }
                    } else {
                        match mode {
                            11 | 24 => 8,
                            12 | 26 => 16,
                            _ => 20,
                        }
                    };
                    let value = if mode <= 13 { 1 } else { extra_base + 128 };
                    if arch.is64() {
                        p.space.w64(oa + field, value).unwrap();
                    } else {
                        p.space.w32(oa + field, value as u32).unwrap();
                    }
                    if mode >= 24 {
                        p.space.wr(extra_base + 128, &[0; 64]).unwrap();
                        if mode == 25 {
                            p.space.w32(extra_base + 128, 12).unwrap();
                        }
                        if mode == 26 {
                            p.space.wr(extra_base + 128, &[1]).unwrap();
                        }
                    }
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
                attributes(p, oa, length, flags, 0);
                p.vm.protect(attrs_base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            23 => {
                out = output_base + PAGE_SIZE - offset;
                p.vm.protect(output_base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
                use_attrs = false;
            }
            29 => out = id_base + 128 + offset,
            _ => panic!("unknown native mode{mode}"),
        }
        if !use_attrs || matches!(mode, 28 | 29) {
            oa = 0;
        }
        let mut id = id_base + 128;
        match id_mode {
            0 => id = 0,
            1 => {}
            2 => id = 1,
            3 => {
                if repeat == 0 {
                    p.vm.protect(id_base, PAGE_SIZE, prot::READONLY).unwrap();
                }
            }
            4 => {
                if repeat == 0 {
                    p.vm.protect(id_base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
            }
            5 => {
                id += offset;
                p.space.w32(id, id_value).unwrap();
            }
            6 => {
                id = id_base + PAGE_SIZE - offset;
                p.space.w32(id, id_value).unwrap();
                p.vm.protect(id_base + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            7 => id = out,
            8 => id = oa,
            9 => {
                id = out + offset;
                p.space.w32(id, id_value).unwrap();
            }
            _ => panic!("unknown native ID mode{id_mode}"),
        }
        let backing = if matches!(mode, 20 | 21) {
            attrs_base + 128
        } else {
            output_base + 128
        };
        assert_eq!(
            call(p, &mut t, [out, id, oa, n("count", 16), access]),
            Outcome::Continue,
            "{arch} case{}",
            row["case"]
        );
        assert_eq!(
            t.cpu.gpr(0),
            expected_status,
            "{arch} case{} {}",
            row["case"],
            row["label"]
        );
        assert_eq!(n("exception", 16), 0);
        let success = expected_status == 0;
        let handle = if success {
            p.space.ptr(out, arch.ptr_size()).unwrap()
        } else {
            0
        };
        if success {
            assert_eq!(p.objects.get(handle).unwrap().type_name(), "IRTimer");
            assert_eq!(p.objects.access(handle), Some(n("grant", 16) as u32));
            assert_eq!(p.objects.flags(handle), Some(n("handle-flags", 10) as u32));
            assert!(
                matches!(p.objects.get(handle),Some(Object::IrTimer{attributes,timer_id,signaled:false}) if u64::from(*attributes)==n("count",16) && *timer_id==(if n("count",16)&2!=0 {Some(id_value)} else {None}))
            );
            assert_eq!(
                n("query", 16),
                0xC0000024,
                "legacy Timer query must distinguish IRTimer"
            );
            assert_eq!(p.objects.handle_count(), before_handles + 1);
            assert_eq!(p.objects.iter().count(), before_objects + 1);
        }
        for (name, address, size) in [
            ("output", out, 96),
            ("output-backing", backing, 96),
            ("attributes", oa, if arch.is64() { 48 } else { 24 }),
            ("id", id, 16),
            ("id-backing", id_base + 128, 16),
        ] {
            let mut expected = bytes(row[name]);
            let captured_key = format!("{name}-captured");
            let captured = n(&captured_key, 10);
            assert_eq!(expected.len() as u64, captured);
            // The producer stores its VirtualAlloc base + 128 in exactly one
            // input pointer slot. Relocate that address, retaining all other
            // bytes. These modes reject non-null OA before any capture/store.
            if name == "attributes" && (24..=26).contains(&mode) {
                let field = if arch.is64() {
                    match mode {
                        24 => 16,
                        25 => 40,
                        _ => 32,
                    }
                } else {
                    match mode {
                        24 => 8,
                        25 => 20,
                        _ => 16,
                    }
                };
                let width = arch.ptr_size() as usize;
                let mut native = [0; 8];
                native[..width].copy_from_slice(&expected[field..field + width]);
                assert_eq!(u64::from_le_bytes(native) & 0xFFFF, 128);
                expected[field..field + width]
                    .copy_from_slice(&(extra_base + 128).to_le_bytes()[..width]);
            }
            if success {
                let width = arch.ptr_size();
                let first = out.max(address);
                let last = (out + width).min(address + expected.len() as u64);
                if first < last {
                    let src = (first - out) as usize;
                    let dst = (first - address) as usize;
                    let count = (last - first) as usize;
                    assert_eq!(
                        &expected[dst..dst + count],
                        &n("handle", 16).to_le_bytes()[src..src + count]
                    );
                    expected[dst..dst + count]
                        .copy_from_slice(&handle.to_le_bytes()[src..src + count]);
                }
            }
            assert_eq!(
                snapshot(p, address, size),
                expected,
                "{arch} case{} {name}",
                row["case"]
            );
        }
        for (base, key) in [
            (output_base, "output-guard"),
            (attrs_base, "attrs-guard"),
            (id_base, "id-guard"),
        ] {
            assert_eq!(
                p.vm.query(base).unwrap().protect & prot::GUARD != 0,
                n(key, 10) != 0,
                "{arch} case{} {key}",
                row["case"]
            );
        }
        if success {
            p.objects.close(handle).unwrap();
        }
        assert_eq!(n("handles-before", 10), n("handles-after", 10));
        assert_eq!(p.objects.handle_count(), before_handles);
        assert_eq!(p.objects.iter().count(), before_objects);
        checked += 1;
    }
    (checked, excluded)
}
macro_rules! capture {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-timer2-create/",
            $name
        ))
    };
}
#[test]
fn native_timer2_create_replays_original_capture_order_reduced_and_alias_observations() {
    let mut checked = 0;
    let mut excluded = 0;
    for (arch, captures) in [
        (
            WinArch::Arm64,
            [
                capture!("native-timer2-create-probe-arm64.log"),
                capture!("native-timer2-order-probe-arm64.log"),
                capture!("native-timer2-reduced-probe-arm64.log"),
                capture!("native-timer2-alias-probe-arm64.log"),
            ],
        ),
        (
            WinArch::X64,
            [
                capture!("native-timer2-create-probe-x64.log"),
                capture!("native-timer2-order-probe-x64.log"),
                capture!("native-timer2-reduced-probe-x64.log"),
                capture!("native-timer2-alias-probe-x64.log"),
            ],
        ),
        (
            WinArch::X86,
            [
                capture!("native-timer2-create-probe-x86.log"),
                capture!("native-timer2-order-probe-x86.log"),
                capture!("native-timer2-reduced-probe-x86.log"),
                capture!("native-timer2-alias-probe-x86.log"),
            ],
        ),
        (
            WinArch::X86,
            [
                capture!("native-timer2-create-probe-x86-laa.log"),
                capture!("native-timer2-order-probe-x86-laa.log"),
                capture!("native-timer2-reduced-probe-x86-laa.log"),
                capture!("native-timer2-alias-probe-x86-laa.log"),
            ],
        ),
    ] {
        for (text, count) in captures.into_iter().zip([835, 842, 267, 154]) {
            let (a, b) = replay(arch, text, count);
            checked += a;
            excluded += b;
        }
    }
    println!("Timer2 original replays={checked}, privilege exclusions={excluded}");
    assert_eq!(checked + excluded, 8392);
}
