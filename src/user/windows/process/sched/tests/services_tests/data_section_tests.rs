//! Replay of independently executed native non-image section controls.
use super::section_map_tests::{call, get, ptr, scratch};
use super::*;
use crate::user::windows::object_namespace::data::{DataRegion, DataSection};
use std::collections::HashMap;
use std::sync::Arc;
const SERVICE: u32 = 0x28;
macro_rules! corpus {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-shared-section/",
            $file
        ))
    };
}
fn rows<'a>(raw: &'a str, prefix: &str) -> Vec<HashMap<&'a str, &'a str>> {
    raw.lines()
        .filter(|line| line.starts_with(prefix))
        .map(|line| {
            line.split_whitespace()
                .filter_map(|word| word.split_once('='))
                .collect()
        })
        .collect()
}
fn number(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}
fn section(raw: &str) -> Arc<DataSection> {
    let row = rows(raw, "bytes name=committed-snapshot ")[0].clone();
    let bytes: Vec<_> = row["hex"]
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    assert_eq!(bytes.len(), 20480);
    DataSection::selected(
        0x20004,
        STATUS_PRIVILEGE_NOT_HELD,
        false,
        1 << 20,
        vec![
            DataRegion {
                offset: 0,
                size: bytes.len() as u64,
                bytes: Some(bytes.into()),
            },
            DataRegion {
                offset: 20480,
                size: 1028096,
                bytes: None,
            },
        ],
    )
    .unwrap()
}
fn data_fixture(
    arch: WinArch,
    laa: bool,
) -> (crate::user::windows::process::WindowsProcess, Thread) {
    if !laa {
        return fixture(arch, "NtMapViewOfSection", SERVICE);
    }
    use crate::user::windows::process::{WindowsConfig, WindowsProcess};
    let mut bytes =
        include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
            .to_vec();
    let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
    let flags = u16::from_le_bytes(bytes[pe + 22..pe + 24].try_into().unwrap()) | 0x20;
    bytes[pe + 22..pe + 24].copy_from_slice(&flags.to_le_bytes());
    let mut config =
        WindowsConfig::embedded("C:\\data-section-laa.exe", vec![], vec![], 1).unwrap();
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
}

#[test]
fn native_data_section_replays_160_installed_geometry_status_and_region_controls() {
    let sources = [
        (
            WinArch::Arm64,
            false,
            corpus!("native-windows-shared-section-geometry-arm64.log"),
        ),
        (
            WinArch::X64,
            false,
            corpus!("native-windows-shared-section-geometry-x64.log"),
        ),
        (
            WinArch::X86,
            false,
            corpus!("native-windows-shared-section-geometry-x86.log"),
        ),
        (
            WinArch::X86,
            true,
            corpus!("native-windows-shared-section-geometry-x86-laa.log"),
        ),
    ];
    replay_geometry(&sources, 40);
}

fn replay_geometry(sources: &[(WinArch, bool, &str)], cases_per_profile: usize) {
    let mut count = 0;
    for &(arch, laa, raw) in sources {
        let selected = section(raw);
        let cases = rows(raw, "case=");
        assert_eq!(cases.len(), cases_per_profile);
        for row in cases {
            let (mut process, mut thread) = data_fixture(arch, laa);
            let p = process.state_mut();

            let handle = u64::from(
                p.objects
                    .open_data_section(selected.clone(), false, 4)
                    .unwrap(),
            );
            let b = scratch(p);
            ptr(p, b, 0);
            ptr(p, b + 16, number(&row, "size-in", 10));
            p.space.w64(b + 32, number(&row, "offset-in", 10)).unwrap();
            let offset = if row["offset-present"] == "1" {
                b + 32
            } else {
                0
            };
            let got = call(
                p,
                &mut thread,
                &[
                    handle,
                    arch.ptr(u64::MAX),
                    b,
                    number(&row, "zero", 10),
                    number(&row, "commit", 10),
                    offset,
                    b + 16,
                    number(&row, "inherit", 10),
                    number(&row, "flags", 16),
                    number(&row, "protect", 16),
                ],
            );
            assert_eq!(
                got,
                number(&row, "status", 16) as u32,
                "{arch} laa={laa} {row:?}"
            );
            assert_eq!(get(p, b + 16), number(&row, "size-out", 10), "{row:?}");
            assert_eq!(
                p.space.u64(b + 32).unwrap(),
                number(&row, "offset-out", 16),
                "{row:?}"
            );
            if got == STATUS_SUCCESS {
                let base = get(p, b);
                assert!(base != 0 && p.vm.is_data_view(base));
                assert_eq!(p.vm.query(base).unwrap().kind, mem::MAPPED);
                assert_eq!(
                    p.vm.query(base).unwrap().allocation_protect,
                    number(&row, "protect", 16) as u32
                );
                let index = raw.find(&format!("case={} ", row["case"])).unwrap();
                let block = raw[index..].split("unmap status=").next().unwrap();
                for region in rows(block, "region ") {
                    let address = base + number(&region, "offset", 10);
                    let observed = p.vm.query(address).unwrap();
                    assert_eq!(
                        (
                            observed.state,
                            observed.protect,
                            observed.kind,
                            observed.size
                        ),
                        (
                            number(&region, "state", 16) as u32,
                            number(&region, "protection", 16) as u32,
                            number(&region, "type", 16) as u32,
                            number(&region, "bytes", 10)
                        ),
                        "{row:?}"
                    );
                }
                p.objects.close(handle).unwrap();
                p.modules.nt_services = Some((
                    p.modules.nt_services.as_ref().unwrap().0,
                    table(arch, "NtUnmapViewOfSection", SERVICE),
                ));
                assert_eq!(
                    call(p, &mut thread, &[arch.ptr(u64::MAX), base + 1]),
                    STATUS_SUCCESS
                );
                assert!(!p.vm.is_data_view(base));
            } else {
                assert_eq!(get(p, b), 0);
            }
            count += 1;
        }
    }
    assert_eq!(count, cases_per_profile * sources.len());
}

fn namespace(selected: Arc<DataSection>) -> crate::user::windows::object_namespace::Namespace {
    use crate::user::windows::object_namespace::{CapturedEntry, Namespace, SelectedDirectory};
    let entry = |name: &str, kind: &str| CapturedEntry {
        name: name.encode_utf16().collect(),
        kind: kind.encode_utf16().collect(),
        target: None,
        link_access: None,
        section: None,
    };
    let directory = |path: &str, entries| SelectedDirectory {
        path: path.encode_utf16().collect(),
        maximum_access: 0xF000F,
        force_case_insensitive: false,
        system_security_status: STATUS_PRIVILEGE_NOT_HELD,
        entries,
    };
    let table: Arc<[u16]> = (0..=u16::MAX)
        .map(|u| if (97..=122).contains(&u) { u - 32 } else { u })
        .collect::<Vec<_>>()
        .into();
    let mut namespace = Namespace::selected(
        table,
        vec![
            directory("\\", vec![entry("Windows", "Directory")]),
            directory("\\Windows", vec![entry("SharedSection", "Section")]),
        ],
    )
    .unwrap();
    namespace.select_data_section(selected).unwrap();
    namespace
}
fn attributes(p: &Proc, b: u64, flags: u32) -> u64 {
    let name: Vec<_> = "\\Windows\\SharedSection"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    p.space.wr(b + 1024, &name).unwrap();
    p.space.w16(b + 256, name.len() as u16).unwrap();
    p.space.w16(b + 258, name.len() as u16).unwrap();
    ptr(p, b + 256 + if p.arch.is64() { 8 } else { 4 }, b + 1024);
    p.space
        .w32(b + 512, if p.arch.is64() { 48 } else { 24 })
        .unwrap();
    ptr(p, b + 512 + if p.arch.is64() { 16 } else { 8 }, b + 256);
    p.space
        .w32(b + 512 + if p.arch.is64() { 24 } else { 12 }, flags)
        .unwrap();
    b + 512
}
#[test]
fn native_data_section_replays_128_actual_open_and_granted_access_controls() {
    let sources = [
        (
            WinArch::Arm64,
            corpus!("native-windows-shared-section-grants2-arm64.log"),
        ),
        (
            WinArch::X64,
            corpus!("native-windows-shared-section-grants2-x64.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-windows-shared-section-grants2-x86.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-windows-shared-section-grants2-x86-laa.log"),
        ),
    ];
    let mut count = 0;
    for (arch, raw) in sources {
        let (mut process, mut thread) = fixture(arch, "NtOpenSection", SERVICE);
        let p = process.state_mut();
        p.object_namespace = namespace(section(corpus!(
            "native-windows-shared-section-geometry-arm64.log"
        )));
        let b = scratch(p);
        let cases = rows(raw, "case=");
        assert_eq!(cases.len(), 32);
        for row in cases {
            let attrs = attributes(p, b, number(&row, "attributes", 16) as u32);
            ptr(p, b, 0xA5A5A5A5);
            let result = call(p, &mut thread, &[b, number(&row, "access", 16), attrs]);
            assert_eq!(result, number(&row, "open", 16) as u32, "{arch} {row:?}");
            if result == STATUS_SUCCESS {
                let at = raw.find(&format!("case={} ", row["case"])).unwrap();
                let grant = rows(&raw[at..], "grant ")[0].clone();
                let handle = get(p, b);
                assert_eq!(
                    p.objects.access(handle),
                    Some(number(&grant, "rights", 16) as u32)
                );
                assert_eq!(p.objects.get(handle).unwrap().type_name(), "Section");
                p.objects.close(handle).unwrap();
            } else {
                assert_eq!(get(p, b), 0);
            }
            count += 1;
        }
    }
    assert_eq!(count, 128);
}
#[test]
fn native_data_section_replays_88_actual_protection_status_and_output_controls() {
    let sources = [
        (
            WinArch::Arm64,
            corpus!("native-windows-shared-section-protect-arm64.log"),
        ),
        (
            WinArch::X64,
            corpus!("native-windows-shared-section-protect-x64.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-windows-shared-section-protect-x86.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-windows-shared-section-protect-x86-laa.log"),
        ),
    ];
    let mut count = 0;
    for (arch, raw) in sources {
        let cases = rows(raw, "case=");
        assert_eq!(cases.len(), 22);
        for row in cases {
            let (mut process, mut thread) = fixture(arch, "NtProtectVirtualMemory", SERVICE);
            let p = process.state_mut();
            let selected = section(corpus!("native-windows-shared-section-geometry-arm64.log"));
            let base = 0x5000_0000;
            p.vm.map_data_section(
                selected.clone(),
                base,
                0,
                selected.size,
                number(&row, "initial", 16) as u32,
            )
            .unwrap();
            let b = scratch(p);
            ptr(p, b, base);
            ptr(p, b + 16, 4096);
            p.space.w32(b + 32, 0xA5A5A5A5).unwrap();
            let result = call(
                p,
                &mut thread,
                &[
                    arch.ptr(u64::MAX),
                    b,
                    b + 16,
                    number(&row, "replacement", 16),
                    b + 32,
                ],
            );
            assert_eq!(result, number(&row, "status", 16) as u32, "{arch} {row:?}");
            assert_eq!(
                p.space.u32(b + 32).unwrap(),
                number(&row, "old", 16) as u32,
                "{row:?}"
            );
            assert_eq!(get(p, b + 16), number(&row, "size-out", 10));
            let region = p.vm.query(base).unwrap();
            assert_eq!(region.protect, number(&row, "reported", 16) as u32);
            assert_eq!(
                region.allocation_protect,
                number(&row, "allocation-protection", 16) as u32
            );
            count += 1;
        }
    }
    assert_eq!(count, 88);
}

#[test]
fn native_data_section_replays_156_flag_controls_including_original_startup_flags() {
    replay_geometry(
        &[
            (
                WinArch::Arm64,
                false,
                corpus!("native-windows-shared-section-flags-arm64.log"),
            ),
            (
                WinArch::X64,
                false,
                corpus!("native-windows-shared-section-flags-x64.log"),
            ),
            (
                WinArch::X86,
                false,
                corpus!("native-windows-shared-section-flags-x86.log"),
            ),
            (
                WinArch::X86,
                true,
                corpus!("native-windows-shared-section-flags-x86-laa.log"),
            ),
        ],
        39,
    );
}
