//! Build29683 capture-driven status, pointer, access and output replay.
//! Image contents are synthetic; source geometry/machine are native captures.
use super::*;
use crate::user::windows::memory::AllocKind;
use crate::user::windows::object_namespace::{
    ImageRegion, ImageTemplate, Namespace, SelectedDirectory,
};
use std::collections::HashMap;
use std::sync::Arc;
type Row<'a> = HashMap<&'a str, &'a str>;
fn rows<'a>(text: &'a str, prefix: &str) -> Vec<Row<'a>> {
    text.lines()
        .filter(|l| l.starts_with(prefix))
        .map(|l| {
            l.split_whitespace()
                .filter_map(|v| v.split_once('='))
                .collect()
        })
        .collect()
}
fn n(row: &Row, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}
fn image(text: &str, section: u64) -> Arc<ImageTemplate> {
    let source_section = if section <= 6 { 1 } else { section };
    let calls = rows(text, "case=");
    let views = rows(text, "view case=");
    let call = calls
        .iter()
        .find(|r| {
            n(r, "section", 10) == source_section
                && r["allocation"] == "00000000"
                && views.iter().any(|v| v["case"] == r["case"])
        })
        .unwrap();
    let view = views.iter().find(|v| v["case"] == call["case"]).unwrap();
    let size = n(view, "size", 10);
    let preferred = n(view, "preferred", 16);
    let machine = n(view, "machine", 16) as u16;
    let mut bytes = vec![0u8; size as usize];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3C..0x40].copy_from_slice(&128u32.to_le_bytes());
    bytes[128..132].copy_from_slice(b"PE\0\0");
    bytes[132..134].copy_from_slice(&machine.to_le_bytes());
    bytes[152..154]
        .copy_from_slice(&(if machine == 0x14C { 0x10Bu16 } else { 0x20Bu16 }).to_le_bytes());
    if machine == 0x14C {
        bytes[180..184].copy_from_slice(&(preferred as u32).to_le_bytes());
    } else {
        bytes[176..184].copy_from_slice(&preferred.to_le_bytes());
    }
    bytes[208..212].copy_from_slice(&(size as u32).to_le_bytes());
    let regions = rows(text, "view-region ")
        .into_iter()
        .filter(|r| r["case"] == call["case"])
        .filter_map(|r| {
            let offset = n(&r, "offset", 16);
            if offset >= size {
                return None;
            }
            Some(ImageRegion {
                offset,
                size: n(&r, "size", 10).min(size - offset),
                state: n(&r, "state", 16) as u32,
                protection: n(&r, "protect", 16) as u32,
            })
        })
        .collect();
    ImageTemplate::selected(bytes, regions).unwrap()
}
fn pointer(mode: u64, page: u64, width: u64, b: u64) -> u64 {
    let ordinary = b + page * 4096 + 128;
    match mode {
        1 => 0,
        2 => 1,
        3 => ordinary + 1,
        4 => ordinary + 2,
        5 | 6 | 7 | 9 => b + page * 4096,
        8 => b + (page + 1) * 4096 - width / 2,
        10 => ordinary + 3,
        11 => ordinary + 4,
        _ => ordinary,
    }
}
fn protect(p: &mut Proc, b: u64, mode: u64, page: u64) {
    let flags = match mode {
        5 => prot::READWRITE | prot::GUARD,
        6 | 8 => prot::NOACCESS,
        7 => prot::READONLY,
        9 => prot::READONLY | prot::GUARD,
        _ => prot::READWRITE,
    };
    if flags != prot::READWRITE {
        p.vm.protect(b + (page + u64::from(mode == 8)) * 4096, 4096, flags)
            .unwrap();
    }
}
fn store(p: &Proc, b: u64, at: u64, value: u64, width: u64) {
    if at >= b && at - b <= 8 * 4096 - width {
        p.space
            .wr(at, &value.to_le_bytes()[..width as usize])
            .unwrap();
    }
}
fn load(p: &Proc, b: u64, at: u64, width: u64) -> u64 {
    if at < b || at - b > 8 * 4096 - width {
        return u64::MAX;
    }
    let mut out = [0u8; 8];
    p.space.rd(at, &mut out[..width as usize]).unwrap();
    u64::from_le_bytes(out)
}
macro_rules! capture {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-section-map/",
            $name
        ))
    };
}
#[test]
fn native_section_map_replays_all_4236_native_access_and_low_address_observations() {
    let mut checked = 0;
    let mut mismatches = 0usize;
    let mut details = Vec::new();
    macro_rules! same {
        ($actual:expr, $expected:expr, $context:expr) => {{
            let (actual, expected) = ($actual, $expected);
            if actual != expected {
                mismatches += 1;
                if details.len() < 64 {
                    details.push(format!(
                        "{} actual={actual:X?} expected={expected:X?}",
                        $context
                    ));
                }
            }
        }};
    }
    for (arch, laa, text, low) in [
        (
            WinArch::Arm64,
            false,
            capture!("native-section-map-access-arm64.log"),
            false,
        ),
        (
            WinArch::X64,
            false,
            capture!("native-section-map-access-x64.log"),
            false,
        ),
        (
            WinArch::X86,
            false,
            capture!("native-section-map-access-x86.log"),
            false,
        ),
        (
            WinArch::X86,
            true,
            capture!("native-section-map-access-x86-laa.log"),
            false,
        ),
        (
            WinArch::Arm64,
            false,
            capture!("native-section-map-low-address-arm64.log"),
            true,
        ),
        (
            WinArch::X64,
            false,
            capture!("native-section-map-low-address-x64.log"),
            true,
        ),
        (
            WinArch::X86,
            false,
            capture!("native-section-map-low-address-x86.log"),
            true,
        ),
        (
            WinArch::X86,
            true,
            capture!("native-section-map-low-address-x86-laa.log"),
            true,
        ),
    ] {
        let (mut process, mut t) = if laa {
            use crate::user::windows::process::{WindowsConfig, WindowsProcess};
            let mut bytes = include_bytes!(
                "../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe"
            )
            .to_vec();
            let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
            let flags = u16::from_le_bytes(bytes[pe + 22..pe + 24].try_into().unwrap()) | 0x20;
            bytes[pe + 22..pe + 24].copy_from_slice(&flags.to_le_bytes());
            let mut config =
                WindowsConfig::embedded("C:\\section-map-laa.exe", vec![], vec![], 1).unwrap();
            config.arena_bytes = 64 << 20;
            let mut process = WindowsProcess::spawn_image(config, bytes).unwrap();
            let p = process.state_mut();
            let module = p.modules.by_name("ntdll.dll").unwrap();
            p.modules.nt_services = Some((module, table(arch, "NtMapViewOfSection", SERVICE)));
            let tid = *p.threads.keys().next().unwrap();
            let mut thread = p.threads.remove(&tid).unwrap();
            thread.attached = true;
            thread.main = false;
            (process, thread)
        } else {
            fixture(arch, "NtMapViewOfSection", SERVICE)
        };
        let p = process.state_mut();
        if arch == WinArch::X86 {
            assert_eq!(p.vm.high(), if laa { 0xFFFF0000 } else { 0x7FFF0000 });
        }
        let b =
            p.vm.allocate(
                Some(0x60000000),
                8 * 4096,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let captures = rows(text, "case=");
        assert_eq!(captures.len(), if low { 145 } else { 914 });
        if low {
            // Preserve occupied intervals relative to the translated explicit
            // 64 KiB candidate. Native layout, not expected mapping status,
            // supplies these independent conflict inputs.
            let mut occupied: Vec<(u64, u64)> = rows(text, "low-layout ")
                .into_iter()
                .filter(|r| r["state"] != "10000")
                .filter_map(|r| {
                    let start = n(&r, "base", 16).max(65536);
                    let end = n(&r, "base", 16)
                        .saturating_add(n(&r, "size", 10))
                        .min(0x500000);
                    (start < end).then_some((start & !65535, (end + 65535) & !65535))
                })
                .collect();
            occupied.sort_unstable();
            let mut merged: Vec<(u64, u64)> = Vec::new();
            for (start, end) in occupied {
                if let Some(last) = merged.last_mut()
                    && start <= last.1
                {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
            for (start, end) in merged {
                p.vm.reserve(
                    Some(0x800000 + start - 65536),
                    end - start,
                    prot::READONLY,
                    AllocKind::Private,
                    false,
                    None,
                )
                .unwrap();
            }
        }
        let mut hs = [0u64; 13];
        for source in [1, 7, 8, 9] {
            let img = image(text, source);
            if source == 1 || source == 9 || source == 8 && arch == WinArch::X86 {
                if img.preferred < p.vm.high()
                    && p.vm.is_free(img.preferred, img.bytes.len() as u64)
                {
                    p.vm.reserve(
                        Some(img.preferred),
                        img.bytes.len() as u64,
                        prot::READONLY,
                        AllocKind::Private,
                        false,
                        None,
                    )
                    .unwrap();
                }
            }
            let section = ImageSection::with_test_image(img);
            if source == 1 {
                // Replay the independently observed randomized low candidate
                // from the preceding successful ZeroBits=1 call, not from the
                // later ZeroBits=8 success/failure expectation.
                let choice = captures
                    .iter()
                    .find(|r| r["case"] == if low { "7" } else { "500" })
                    .unwrap();
                assert_eq!(choice["zero-bits"], "1");
                assert_eq!(choice["added"], "1");
                p.vm.record_section_low_placement(section.clone(), n(choice, "base-out", 16));
            }
            if source == 1 {
                for (i, grant) in [(1, 0xD), (2, 1), (3, 4), (4, 8), (5, 2), (6, 0x2000F)] {
                    hs[i] = p
                        .objects
                        .open_image_section(section.clone(), false, grant)
                        .unwrap()
                        .into();
                }
            } else {
                hs[source as usize] = p
                    .objects
                    .open_image_section(section, false, 0xD)
                    .unwrap()
                    .into();
            }
        }
        hs[10] = p
            .objects
            .insert(Object::Event {
                manual: false,
                signaled: 0,
            })
            .into();
        let upcase: Arc<[u16]> = (0..=u16::MAX).collect::<Vec<_>>().into();
        let ns = Namespace::selected(
            upcase,
            vec![SelectedDirectory {
                path: vec![92],
                maximum_access: 1,
                system_security_status: STATUS_PRIVILEGE_NOT_HELD,
                force_case_insensitive: true,
                entries: vec![],
            }],
        )
        .unwrap();
        hs[11] = p
            .objects
            .open_directory(ns.lookup(None, &[92], 0).unwrap(), false, 1)
            .unwrap()
            .into();
        hs[12] = arch.ptr(u64::MAX);
        let mut processes = [arch.ptr(u64::MAX), 0, arch.ptr(u64::MAX - 1), 0, 0, 1, 0];
        let pid = p.pid;
        for (i, access) in [(3, 8), (4, 0x1400), (6, 0x1FFFFF)] {
            let id = p.objects.create(Object::Process {
                pid,
                exit_code: None,
            });
            processes[i] = p.objects.open_access(id, false, access).unwrap().into();
        }
        let (native_memory, native_upper) = if low {
            (0, 0)
        } else {
            (
                n(
                    captures.iter().find(|r| r["case"] == "574").unwrap(),
                    "base-in",
                    16,
                ),
                n(
                    captures.iter().find(|r| r["case"] == "576").unwrap(),
                    "base-in",
                    16,
                ),
            )
        };
        for r in captures {
            p.vm.protect(b, 8 * 4096, prot::READWRITE).unwrap();
            p.space.wr(b, &[0xA5; 8 * 4096]).unwrap();
            let (bm, sm, om) = (
                n(&r, "base-mode", 10),
                n(&r, "size-mode", 10),
                n(&r, "offset-mode", 10),
            );
            let width = arch.ptr_size();
            let (bp, sp, op) = (
                pointer(bm, 0, width, b),
                pointer(sm, 2, width, b),
                pointer(om, 4, width, b),
            );
            let mut base = n(&r, "base-in", 16);
            // Native memory occupancy/ASLR is represented by equivalent guest
            // scratch/free/upper-bound placements; pointer fault modes stay exact.
            if low && base == 65536 {
                base = 0x00800000;
            }
            if r["family"] == "14" {
                if base == native_memory {
                    base = b;
                } else if base == native_upper {
                    base = p.vm.high() - 65536;
                } else if base == 65536 {
                    base = 0x40010000;
                }
            }
            store(p, b, bp, base, width);
            store(p, b, sp, n(&r, "size-in", 10), width);
            store(p, b, op, n(&r, "offset-in", 16), 8);
            protect(p, b, bm, 0);
            protect(p, b, sm, 2);
            let use_offset = r["use-offset"] == "1";
            if use_offset {
                protect(p, b, om, 4);
            }
            let before = p.vm.committed_bytes();
            let status = call(
                p,
                &mut t,
                &[
                    hs[n(&r, "section", 10) as usize],
                    processes[n(&r, "process", 10) as usize],
                    bp,
                    n(&r, "zero-bits", 16),
                    n(&r, "commit", 10),
                    if use_offset { op } else { 0 },
                    sp,
                    n(&r, "inherit", 10),
                    n(&r, "allocation", 16),
                    n(&r, "protect", 16),
                ],
            );
            same!(
                status,
                n(&r, "status", 16) as u32,
                format!("{arch} LAA={laa} status {r:?}")
            );
            for (page, key) in [(0, "p0"), (2, "p2"), (4, "p4")] {
                same!(
                    p.vm.query(b + page * 4096).unwrap().protect,
                    n(&r, key, 16) as u32,
                    format!("{arch} {key} {r:?}")
                );
            }
            p.vm.protect(b, 8 * 4096, prot::READWRITE).unwrap();
            let out = load(p, b, bp, width);
            same!(
                load(p, b, sp, width),
                n(&r, "size-out", 10),
                format!("{arch} size {r:?}")
            );
            same!(
                load(p, b, op, 8),
                n(&r, "offset-out", 16),
                format!("{arch} offset {r:?}")
            );
            let views: Vec<_> =
                p.vm.allocations()
                    .filter(|a| p.vm.section_view(a.base).is_some())
                    .map(|a| a.base)
                    .collect();
            same!(
                views.len() as u64,
                n(&r, "added", 10),
                format!("{arch} added {r:?}")
            );
            if n(&r, "added", 10) == 0 {
                same!(
                    out,
                    if bp >= b && bp - b <= 8 * 4096 - width {
                        base
                    } else {
                        u64::MAX
                    },
                    format!("{arch} base {r:?}")
                );
                same!(
                    p.vm.committed_bytes(),
                    before,
                    format!("{arch} rejected mutation {r:?}")
                );
            } else if out != 0 && out != u64::MAX {
                same!(
                    p.vm.section_view(out).is_some(),
                    true,
                    format!("{arch} returned view {r:?}")
                );
            }
            // Restore every owned view even on a failed comparison so the next
            // independent observation cannot inherit a mismatching prior state.
            for allocation in views {
                p.vm.release(allocation).unwrap();
            }
            same!(
                p.vm.committed_bytes(),
                before,
                format!("{arch} restored {r:?}")
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 4236);
    assert_eq!(
        mismatches,
        0,
        "all {checked} native rows ran; first mismatches:\n{}",
        details.join("\n")
    );
}
