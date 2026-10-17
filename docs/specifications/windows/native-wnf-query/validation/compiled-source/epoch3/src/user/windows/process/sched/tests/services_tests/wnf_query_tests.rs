//! Replays native query rows, including every captured byte and protection.
use super::*;
use crate::user::windows::nt::status::*;
use crate::user::windows::process::{WindowsConfig, WindowsProcess};
use crate::user::windows::wnf::{Data, Definition, Queries, Record};
use std::sync::{Arc, OnceLock};

const SYSTEM: u64 = 0x0280_032E_A3BC_0875;
const PROCESS: u64 = 0x4191_1A28_A3BC_10B5;
const TYPED: u64 = 0x41C6_4E6D_A3BE_0045;
const DENIED: u64 = 0x41C6_4E6D_A3BF_0045;
fn data() -> &'static serde_json::Value {
    static DATA: OnceLock<serde_json::Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../../../../docs/specifications/windows/native-wnf-query/cases.json"
        ))
        .unwrap()
    })
}
fn hex(s: &str) -> Vec<u8> {
    crate::user::windows::wnf::tests::hex(s)
}
fn number(row: &serde_json::Value, key: &str) -> u32 {
    u32::from_str_radix(row[key].as_str().unwrap(), 16).unwrap()
}
fn guid() -> Vec<u8> {
    hex("781b344a2c2b4e4d8182838485868788")
}
fn system_guid() -> Vec<u8> {
    hex("6364f948d652854e9c02f2c14e17264e")
}
fn names(profile: &str) -> [u64; 2] {
    let rows = data()["boundaries"][profile].as_array().unwrap();
    ["owned-empty", "owned-payload"].map(|name| {
        let row = rows
            .iter()
            .find(|row| row["name"] == name && row["role"] == "0")
            .unwrap();
        u64::from_le_bytes(hex(row["p0-cells"].as_str().unwrap()).try_into().unwrap())
    })
}
fn state_name(role: &str, names: [u64; 2]) -> u64 {
    match role {
        "owned-empty" => names[0],
        "owned-payload" => names[1],
        "owned-typed" => TYPED,
        "owned-denied" => DENIED,
        "known-process" => PROCESS,
        "captured-system" => SYSTEM,
        "zero" => 0,
        "all-ones" => u64::MAX,
        "unselected" => 0x4191_1A28_63BC_10B5,
        _ => panic!("unknown native name"),
    }
}
fn selected(names: [u64; 2]) -> crate::user::windows::wnf::Catalog {
    let payload = hex("52415800010203048090a0b0c0d0e0ff");
    let rows = [
        (
            names[0],
            4096,
            None,
            Record::Data(Data {
                stamp: 0,
                bytes: Arc::from([]),
            }),
        ),
        (
            names[1],
            4096,
            None,
            Record::Data(Data {
                stamp: 1,
                bytes: payload.clone().into(),
            }),
        ),
        (
            TYPED,
            4096,
            Some(guid().try_into().unwrap()),
            Record::Data(Data {
                stamp: 1,
                bytes: payload.into(),
            }),
        ),
        (DENIED, 4096, None, Record::Status(STATUS_ACCESS_DENIED)),
        (
            PROCESS,
            0,
            None,
            Record::Data(Data {
                stamp: 0,
                bytes: Arc::from([]),
            }),
        ),
        (
            SYSTEM,
            8,
            Some(system_guid().try_into().unwrap()),
            Record::Data(Data {
                stamp: 1,
                bytes: Arc::from([1, 0, 0, 0, 0, 0, 0, 0]),
            }),
        ),
    ]
    .into_iter()
    .map(|(name, maximum, type_id, record)| {
        (
            Definition {
                name,
                maximum,
                type_id,
            },
            record,
        )
    })
    .collect();
    crate::user::windows::wnf::tests::empty_subject_catalog(false)
        .with_queries(Queries::selected(rows, 0).unwrap())
}
fn setup(
    arch: WinArch,
    laa: bool,
    native: bool,
    profile: &str,
) -> (WindowsProcess, Thread, u64, [u64; 2]) {
    let mut image = match arch {
        WinArch::Arm64 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe")
                .to_vec()
        }
        WinArch::X64 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
                .to_vec()
        }
        WinArch::X86 => {
            include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
                .to_vec()
        }
    };
    if laa {
        let pe = u32::from_le_bytes(image[60..64].try_into().unwrap()) as usize;
        let flags = u16::from_le_bytes(image[pe + 22..pe + 24].try_into().unwrap()) | 0x20;
        image[pe + 22..pe + 24].copy_from_slice(&flags.to_le_bytes());
    }
    let mut cfg = WindowsConfig::embedded("C:\\wnf-query.exe", vec![], vec![], 4096).unwrap();
    cfg.arena_bytes = 64 << 20;
    cfg.native_libraries = native;
    let mut process = WindowsProcess::spawn_image(cfg, image).unwrap();
    let p = process.state_mut();
    let tid = *p.threads.keys().next().unwrap();
    let mut thread = p.threads.remove(&tid).unwrap();
    thread.attached = true;
    thread.main = false;
    thread.cpu.set_sp(thread.cpu.sp() - 0x100);
    if !native {
        let n = p.modules.by_name("ntdll.dll").unwrap();
        p.modules.nt_services = Some((n, table(arch, "NtQueryWnfStateData", 0x21)));
    }
    let ns = names(profile);
    p.objects.select_wnf_catalog(selected(ns));
    let base =
        p.vm.allocate(
            None,
            PAGE_SIZE * 12,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    (process, thread, base, ns)
}
fn invoke(p: &mut Proc, t: &mut Thread, args: &[u64]) -> u32 {
    let sp = t.cpu.sp();
    let rng = p.rng;
    let handles = p.objects.handle_count();
    super::nls_tests::arguments(p, t, args);
    if p.arch != WinArch::Arm64 {
        t.cpu.set_gpr(0, 0x21);
    }
    let out = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, 0x21))
    };
    assert_eq!(out, Outcome::Continue, "{} {args:x?}", p.arch);
    assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
    t.cpu.set_sp(sp);
    assert!(t.frames.is_empty());
    assert_eq!(p.rng, rng);
    assert_eq!(p.objects.handle_count(), handles);
    t.cpu.gpr(0) as u32
}
fn reset(p: &mut Proc, b: u64) {
    p.vm.protect(b, PAGE_SIZE * 12, prot::READWRITE).unwrap();
    p.space
        .wr(b, &vec![0xA5; (PAGE_SIZE * 12) as usize])
        .unwrap();
}
fn logical_input(p: &mut Proc, b: u64, row: &serde_json::Value, ns: [u64; 2]) -> [u64; 6] {
    reset(p, b);
    let mut f = std::array::from_fn(|i| b + PAGE_SIZE * 2 * i as u64);
    p.space
        .w64(f[0], state_name(row["name"].as_str().unwrap(), ns))
        .unwrap();
    let ty = row["type"].as_str().unwrap().parse::<u32>().unwrap();
    let raw = match ty {
        0 => None,
        1 => Some(guid()),
        2 => Some(hex("781b347a2c2b4e4d8182838485868789")),
        3 => Some(vec![0; 16]),
        4 => Some(system_guid()),
        _ => unreachable!(),
    };
    if let Some(raw) = raw {
        p.space.wr(f[1], &raw).unwrap();
    } else {
        f[1] = 0;
    }
    let scope = row["scope"].as_str().unwrap().parse::<u32>().unwrap();
    let raw = match scope {
        0 => None,
        1 => Some(p.objects.primary_token_subject().unwrap().user.clone()),
        2 => Some(hex("010100000000000100000000")),
        3 => Some(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        _ => unreachable!(),
    };
    if let Some(raw) = raw {
        p.space.wr(f[2], &raw).unwrap();
    } else {
        f[2] = 0;
    }
    p.space.w32(f[5], number(row, "capacity")).unwrap();
    f
}
fn logical_check(p: &Proc, f: [u64; 6], row: &serde_json::Value) {
    assert_eq!(p.space.u32(f[3]).unwrap(), number(row, "stamp"));
    assert_eq!(p.space.u32(f[5]).unwrap(), number(row, "length"));
    assert_eq!(
        p.space.bytes(f[4], 64).unwrap(),
        hex(row["bytes"].as_str().unwrap())
    );
}
fn boundary_input(p: &mut Proc, b: u64, row: &serde_json::Value, ns: [u64; 2]) -> [u64; 6] {
    reset(p, b);
    let mut f = std::array::from_fn(|i| b + PAGE_SIZE * 2 * i as u64);
    let name = state_name(row["name"].as_str().unwrap(), ns);
    p.space.w64(f[0], name).unwrap();
    let ty = if name == SYSTEM {
        system_guid()
    } else {
        guid()
    };
    p.space.wr(f[1], &ty).unwrap();
    let user = p.objects.primary_token_subject().unwrap().user.clone();
    p.space.wr(f[2], &user).unwrap();
    let scope_storage = f[2];
    f[2] = 0;
    let role = row["role"].as_str().unwrap().parse::<u32>().unwrap();
    let mut moved = None;
    if (1..=24).contains(&role) {
        let field = ((role - 1) / 4) as usize;
        let kind = (role - 1) % 4;
        if field == 2 {
            f[field] = scope_storage;
        }
        if kind == 0 {
            f[field] = 0;
        } else {
            f[field] += if kind == 3 { 4 } else { u64::from(kind) };
            moved = Some(field);
        }
    } else if (25..=48).contains(&role) {
        if (role - 25) / 4 == 2 {
            f[2] = scope_storage;
        }
    } else if (49..=54).contains(&role) {
        let field = (role - 49) as usize;
        if field == 2 {
            f[field] = scope_storage;
        }
        f[field] += PAGE_SIZE - 2;
        moved = Some(field);
    } else {
        match role {
            55 => f[5] = f[3],
            56 => f[5] = f[4],
            57 => f[3] = f[4],
            58 => f[3] = f[0],
            59 => f[5] = f[0],
            60 => {
                f[3] = f[4];
                f[5] = f[4];
            }
            0 | 61..=64 => {}
            _ => panic!("native boundary role"),
        }
    }
    match moved {
        Some(0) => p.space.w64(f[0], name).unwrap(),
        Some(1) => p.space.wr(f[1], &ty).unwrap(),
        Some(2) => p.space.wr(f[2], &user).unwrap(),
        _ => {}
    }
    if f[5] != 0 {
        p.space.w32(f[5], number(row, "capacity")).unwrap();
    }
    if (25..=48).contains(&role) {
        let field = ((role - 25) / 4) as usize;
        let protections = [
            prot::NOACCESS,
            prot::READONLY,
            prot::READWRITE | prot::GUARD,
            prot::READONLY | prot::GUARD,
        ];
        p.vm.protect(f[field], PAGE_SIZE, protections[((role - 25) % 4) as usize])
            .unwrap();
    }
    if (49..=54).contains(&role) {
        p.vm.protect(
            b + PAGE_SIZE * (2 * u64::from(role - 49) + 1),
            PAGE_SIZE,
            prot::NOACCESS,
        )
        .unwrap();
    }
    if role >= 61 {
        let first = if role == 61 || role == 62 { 0 } else { 3 };
        let second = if role == 61 || role == 63 { 5 } else { 4 };
        for field in [first, second] {
            p.vm.protect(
                b + PAGE_SIZE * 2 * field,
                PAGE_SIZE,
                prot::READWRITE | prot::GUARD,
            )
            .unwrap();
        }
    }
    f
}
fn boundary_check(p: &Proc, b: u64, f: [u64; 6], row: &serde_json::Value) {
    for (i, address) in f.into_iter().enumerate() {
        if (b..b + PAGE_SIZE * 12).contains(&address) {
            assert_eq!(
                p.vm.query(address).unwrap().protect,
                number(row, &format!("p{i}-protect")),
                "field{i} protection"
            );
        }
        let cells = row[format!("p{i}-cells")].as_str().unwrap();
        assert_eq!(cells.len(), 16);
        for (j, value) in cells.as_bytes().chunks_exact(2).enumerate() {
            if value != b"??" {
                let expected = u8::from_str_radix(std::str::from_utf8(value).unwrap(), 16).unwrap();
                assert_eq!(
                    p.space.u8(address + j as u64).unwrap(),
                    expected,
                    "field{i} byte{j}"
                );
            }
        }
    }
}

#[test]
fn native_wnf_query_replays_8640_logical_calls_with_type_scope_and_denial() {
    let rows = data()["logical"].as_array().unwrap();
    assert_eq!(rows.len(), 2160);
    for (arch, laa, profile) in [
        (WinArch::Arm64, false, "arm64"),
        (WinArch::X64, false, "x64"),
        (WinArch::X86, false, "x86"),
        (WinArch::X86, true, "x86-laa"),
    ] {
        let (mut process, mut thread, b, ns) = setup(arch, laa, false, profile);
        let p = process.state_mut();
        for (i, row) in rows.iter().enumerate() {
            let f = logical_input(p, b, row, ns);
            assert_eq!(
                invoke(p, &mut thread, &f),
                number(row, "return"),
                "{profile} logical{i} {row}"
            );
            logical_check(p, f, row);
        }
    }
}
#[test]
fn native_wnf_query_replays_all_6240_boundary_calls_and_generated_name_bytes() {
    for (arch, laa, profile) in [
        (WinArch::Arm64, false, "arm64"),
        (WinArch::X64, false, "x64"),
        (WinArch::X86, false, "x86"),
        (WinArch::X86, true, "x86-laa"),
    ] {
        let rows = data()["boundaries"][profile].as_array().unwrap();
        assert_eq!(rows.len(), 1560);
        let (mut process, mut thread, b, ns) = setup(arch, laa, false, profile);
        let p = process.state_mut();
        for (i, row) in rows.iter().enumerate() {
            let f = boundary_input(p, b, row, ns);
            assert_eq!(
                invoke(p, &mut thread, &f),
                number(row, "return"),
                "{profile} boundary{i} {row}"
            );
            boundary_check(p, b, f, row);
        }
    }
}

#[test]
fn native_wnf_query_explicit_process_scope_accepts_zero_rights_and_keeps_ownership() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, b, ns) = setup(
            arch,
            false,
            false,
            if arch == WinArch::Arm64 {
                "arm64"
            } else if arch == WinArch::X64 {
                "x64"
            } else {
                "x86"
            },
        );
        let p = process.state_mut();
        let own = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        for access in [0, 1, 0x10, 0x400, 0x1000, 0x1FFFFF] {
            let h = u64::from(p.objects.open_access(own, false, access).unwrap());
            let mut row = data()["logical"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| {
                    r["name"] == "known-process"
                        && r["type"] == "0"
                        && r["scope"] == "0"
                        && r["capacity"] == "00000010"
                })
                .unwrap()
                .clone();
            let f = logical_input(p, b, &row, ns);
            let mut args = f;
            args[2] = h;
            assert_eq!(invoke(p, &mut thread, &args), 0);
            row["stamp"] = "00000000".into();
            row["length"] = "00000000".into();
            logical_check(p, f, &row);
        }
        for (scope, status) in [
            (arch.ptr(u64::MAX), 0),
            (arch.ptr(u64::MAX - 1), STATUS_OBJECT_TYPE_MISMATCH),
            (0xDEAD, STATUS_INVALID_HANDLE),
        ] {
            reset(p, b);
            p.space.w64(b, PROCESS).unwrap();
            p.space.w32(b + 40, 0).unwrap();
            assert_eq!(
                invoke(p, &mut thread, &[b, 0, scope, b + 24, 0, b + 40]),
                status
            );
        }
    }
}

#[test]
fn native_wnf_query_snapshot_is_immutable_across_output_changes_and_empty_capacity() {
    for arch in WinArch::ALL {
        let profile = if arch == WinArch::Arm64 {
            "arm64"
        } else if arch == WinArch::X64 {
            "x64"
        } else {
            "x86"
        };
        let (mut process, mut thread, b, ns) = setup(arch, false, false, profile);
        let p = process.state_mut();
        let query = p.objects.wnf_queries().unwrap();
        for capacity in [0, 1, 8, 16] {
            reset(p, b);
            p.space.w64(b, SYSTEM).unwrap();
            p.space.wr(b + 32, &system_guid()).unwrap();
            p.space.w32(b + 80, capacity).unwrap();
            let args = [b, b + 32, 0, b + 64, b + 128, b + 80];
            assert_eq!(
                invoke(p, &mut thread, &args),
                if capacity < 8 {
                    STATUS_BUFFER_TOO_SMALL
                } else {
                    0
                }
            );
            assert_eq!(p.space.u32(b + 64).unwrap(), 1);
            assert_eq!(p.space.u32(b + 80).unwrap(), 8);
            p.space.wr(b + 128, &[0x5A; 16]).unwrap();
            assert_eq!(
                p.objects.wnf_queries().unwrap().get(SYSTEM),
                query.get(SYSTEM)
            );
        }
        let (mut second, _, _, _) = setup(arch, false, false, profile);
        let q = second.state_mut().objects.wnf_queries().unwrap();
        assert!(!Arc::ptr_eq(&q, &query));
        assert_eq!(q.get(PROCESS), query.get(PROCESS));
        assert_ne!(ns[0], ns[1]);
    }
}

#[test]
fn native_wnf_query_absent_and_unavailable_captures_refuse_without_output_fabrication() {
    for arch in WinArch::ALL {
        let profile = if arch == WinArch::Arm64 {
            "arm64"
        } else if arch == WinArch::X64 {
            "x64"
        } else {
            "x86"
        };
        for unavailable in [false, true] {
            let (mut process, mut thread, b, _) = setup(arch, false, false, profile);
            let p = process.state_mut();
            let catalog = if unavailable {
                crate::user::windows::wnf::tests::empty_subject_catalog(false).with_queries(
                    Queries::selected(
                        vec![(
                            Definition {
                                name: SYSTEM,
                                maximum: 8,
                                type_id: None,
                            },
                            Record::Unavailable("controlled missing snapshot"),
                        )],
                        0,
                    )
                    .unwrap(),
                )
            } else {
                Default::default()
            };
            p.objects.select_wnf_catalog(catalog);
            reset(p, b);
            p.space.w64(b, SYSTEM).unwrap();
            p.space.w32(b + 40, 0).unwrap();
            super::nls_tests::arguments(p, &mut thread, &[b, 0, 0, b + 24, 0, b + 40]);
            if arch != WinArch::Arm64 {
                thread.cpu.set_gpr(0, 0x21);
            }
            let outcome = if arch == WinArch::X86 {
                super::super::super::super::services::wow64(p, &mut thread, 0x1234_0000)
            } else {
                handle_stop(p, &mut thread, stop(arch, 0x21))
            };
            let reason = if unavailable {
                "controlled missing snapshot"
            } else {
                "metadata was not selected"
            };
            assert!(
                matches!(outcome,Outcome::Fail(ref m) if m.contains("NtQueryWnfStateData")&&m.contains(reason)),
                "{outcome:?}"
            );
            assert_eq!(p.space.u32(b + 24).unwrap(), 0xA5A5_A5A5);
            assert_eq!(p.space.u32(b + 40).unwrap(), 0);
        }
    }
}

#[cfg(windows)]
#[test]
fn native_wnf_query_installed_leaves_replay_logical_and_boundary_matrices() {
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let mut profiles = vec![(
        host,
        false,
        if host == WinArch::Arm64 {
            "arm64"
        } else if host == WinArch::X64 {
            "x64"
        } else {
            "x86"
        },
    )];
    if host != WinArch::X86 {
        profiles.push((WinArch::X86, false, "x86"));
    }
    profiles.push((WinArch::X86, true, "x86-laa"));
    for (arch, laa, profile) in profiles {
        let (mut process, mut thread, b, ns) = setup(arch, laa, true, profile);
        let p = process.state_mut();
        let module = crate::user::windows::loader::load_dll(p, "ntdll.dll").unwrap();
        for (i, row) in data()["logical"].as_array().unwrap().iter().enumerate() {
            let f = logical_input(p, b, row, ns);
            let got = super::virtual_memory_tests::installed::invoke(
                p,
                &mut thread,
                module,
                "NtQueryWnfStateData",
                &f,
            );
            assert_eq!(got, number(row, "return"), "{profile} installed logical{i}");
            logical_check(p, f, row);
        }
        for (i, row) in data()["boundaries"][profile]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let f = boundary_input(p, b, row, ns);
            let got = super::virtual_memory_tests::installed::invoke(
                p,
                &mut thread,
                module,
                "NtQueryWnfStateData",
                &f,
            );
            assert_eq!(
                got,
                number(row, "return"),
                "{profile} installed boundary{i}"
            );
            boundary_check(p, b, f, row);
        }
    }
}

#[test]
fn native_wnf_query_replays_session_user_machine_scope_and_typed_denial_controls() {
    let control:serde_json::Value=serde_json::from_str(include_str!("../../../../../../../docs/specifications/windows/native-wnf-query/controls/wnf-private-scope-order.json")).unwrap();
    assert_eq!(control["rows"].as_array().unwrap().len(), 168);
    for (arch, laa, profile) in [
        (WinArch::Arm64, false, "arm64"),
        (WinArch::X64, false, "x64"),
        (WinArch::X86, false, "x86"),
        (WinArch::X86, true, "x86-laa"),
    ] {
        let (mut process, mut thread, b, _) = setup(arch, laa, false, profile);
        let p = process.state_mut();
        let mut records = Vec::new();
        for scope in [0u64, 1, 2, 4] {
            for denied in [false, true] {
                let name =
                    0x41C6_4E6D_A3BC_0074 ^ (0x31 | (scope << 6) | (u64::from(denied) << 11));
                let record = if denied {
                    Record::Status(STATUS_ACCESS_DENIED)
                } else {
                    Record::Data(Data {
                        stamp: 0,
                        bytes: Arc::from([]),
                    })
                };
                records.push((
                    Definition {
                        name,
                        maximum: 64,
                        type_id: Some(guid().try_into().unwrap()),
                    },
                    record,
                ));
            }
        }
        p.objects.select_wnf_catalog(
            crate::user::windows::wnf::tests::empty_subject_catalog(false)
                .with_queries(Queries::selected(records, 0).unwrap()),
        );
        for (i, row) in control["rows"].as_array().unwrap().iter().enumerate() {
            reset(p, b);
            let scope = row["scope"].as_u64().unwrap();
            let denied = row["denied"].as_bool().unwrap();
            let name = 0x41C6_4E6D_A3BC_0074 ^ (0x31 | (scope << 6) | (u64::from(denied) << 11));
            p.space.w64(b, name).unwrap();
            let type_ptr = match row["type"].as_str().unwrap() {
                "null" => 0,
                "match" => {
                    p.space.wr(b + 64, &guid()).unwrap();
                    b + 64
                }
                "wrong" => {
                    p.space
                        .wr(b + 64, &hex("781b347a2c2b4e4d8182838485868789"))
                        .unwrap();
                    b + 64
                }
                _ => unreachable!(),
            };
            let explicit = match row["explicit"].as_str().unwrap() {
                "null" => 0,
                "current-process" => arch.ptr(u64::MAX),
                "current-thread" => arch.ptr(u64::MAX - 1),
                "user-sid" => {
                    p.space
                        .wr(b + 128, &p.objects.primary_token_subject().unwrap().user)
                        .unwrap();
                    b + 128
                }
                "world-sid" => {
                    p.space
                        .wr(b + 128, &hex("010100000000000100000000"))
                        .unwrap();
                    b + 128
                }
                "session-id" => {
                    p.space.w32(b + 128, 0).unwrap();
                    b + 128
                }
                "pid" => {
                    p.space.w32(b + 128, p.pid).unwrap();
                    b + 128
                }
                _ => unreachable!(),
            };
            p.space.w32(b + 768, 64).unwrap();
            let args = [b, type_ptr, explicit, b + 256, b + 512, b + 768];
            assert_eq!(
                invoke(p, &mut thread, &args),
                row["status"].as_u64().unwrap() as u32,
                "{profile} scope control{i}/{row}"
            );
            assert_eq!(
                p.space.u32(b + 256).unwrap(),
                row["stamp"].as_u64().unwrap() as u32
            );
            assert_eq!(
                p.space.u32(b + 768).unwrap(),
                row["length"].as_u64().unwrap() as u32
            );
            assert_eq!(
                p.space.bytes(b + 512, 64).unwrap(),
                hex(row["bytes"].as_str().unwrap())
            );
        }
    }
}
