//! Native subscription captures, pointer writes, identity and pending metadata.
use super::*;
use crate::user::windows::nt::status::*;
use crate::user::windows::wnf::tests::{catalog, rows};
use std::collections::HashMap;

#[cfg(windows)]
#[path = "wnf_subscription_leaf_tests.rs"]
mod leaf_tests;

const KNOWN: u64 = 0x41911A28A3BC10B5;
const SERVICE: u32 = 0x1CD;
const POISON: u64 = 0xA5A5A5A5A5A5A5A5;
fn field(row: &HashMap<&str, &str>, name: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[name], radix).unwrap()
}
pub(super) fn call(p: &mut Proc, t: &mut Thread, args: [u64; 4]) -> Outcome {
    let sp = t.cpu.sp();
    t.cpu.set_pc(0x1234_0004);
    match p.arch {
        WinArch::X86 => {
            for (i, value) in [0x1234_0004, 0x1234_0000]
                .into_iter()
                .chain(args)
                .enumerate()
            {
                p.space.w32(sp + i as u64 * 4, value as u32).unwrap();
            }
            t.cpu.set_gpr(0, SERVICE.into());
        }
        WinArch::X64 => {
            t.cpu.set_gpr(0, SERVICE.into());
            t.cpu.set_gpr(1, 0x1234_0004);
            for (i, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr([10, 2, 8, 9][i], value);
            }
        }
        WinArch::Arm64 => {
            for (i, value) in args.into_iter().enumerate() {
                t.cpu.set_gpr(i, value);
            }
        }
    }
    let outcome = if p.arch == WinArch::X86 {
        super::super::super::super::services::wow64(p, t, 0x1234_0000)
    } else {
        handle_stop(p, t, stop(p.arch, SERVICE))
    };
    if outcome == Outcome::Continue {
        assert_eq!(t.cpu.pc(), 0x1234_0004);
        assert_eq!(t.cpu.sp(), sp + if p.arch == WinArch::X86 { 4 } else { 0 });
        assert!(t.frames.is_empty());
    }
    t.cpu.set_sp(sp);
    outcome
}
fn check(p: &mut Proc, t: &mut Thread, args: [u64; 4], expected: u64) {
    assert_eq!(call(p, t, args), Outcome::Continue);
    assert_eq!(t.cpu.gpr(0), expected, "arch={} args={args:X?}", p.arch);
}
fn profiles() -> [(WinArch, &'static str); 4] {
    [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-boundaries-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-boundaries-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-boundaries-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-boundaries-x86-laa.log"
            ),
        ),
    ]
}
#[test]
fn native_wnf_subscription_replays_all_17152_name_mask_calls() {
    let mut count = 0;
    for (arch, text) in profiles() {
        assert!(text.contains("complete cases=4976"));
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let input = t.cpu.sp() - 128;
        let output = input + 32;
        let before = (p.objects.handle_count(), p.objects.iter().count());
        for row in rows(text)
            .into_iter()
            .filter(|r| r.contains_key("name-case"))
        {
            let name = field(&row, "name", 16);
            let mask = field(&row, "mask", 16);
            p.objects.unsubscribe_wnf(name);
            p.space.w64(input, name).unwrap();
            p.space.w64(output, POISON).unwrap();
            check(
                p,
                &mut t,
                [input, 0, mask, output],
                field(&row, "status", 16),
            );
            let id = p.space.u64(output).unwrap();
            if row["status"] == "00000000" {
                let record = p.objects.wnf_subscription(name).unwrap();
                assert_ne!(id, 0);
                assert_eq!(id, record.id);
                assert_eq!(record.event_mask, mask as u32);
                assert_eq!(record.event_mask & 1 != 0, row["present"] == "00000001");
            } else {
                assert_eq!(id, 0);
                assert!(p.objects.wnf_subscription(name).is_none());
            }
            p.objects.unsubscribe_wnf(name);
            count += 1;
        }
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
    }
    assert_eq!(count, 17152);
}
#[test]
fn native_wnf_subscription_replays_all_2496_pointer_status_and_output_cases() {
    let mut count = 0;
    for (arch, text) in profiles() {
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let base =
            p.vm.allocate(
                None,
                4 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        for page in [1, 3] {
            p.vm.protect(base + page * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                .unwrap();
        }
        for offset in 0..8 {
            p.space.w64(base + 32 + 17 * offset, KNOWN).unwrap();
        }
        p.space.w32(base + PAGE_SIZE - 4, KNOWN as u32).unwrap();
        for row in rows(text)
            .into_iter()
            .filter(|r| r.contains_key("pointer-case"))
        {
            p.objects.unsubscribe_wnf(KNOWN);
            let ik = field(&row, "input", 10);
            let ok = field(&row, "output", 10);
            let input = match ik {
                0 => base + 32,
                1 => 0,
                2 => 1,
                3 => base + PAGE_SIZE,
                4 => base + PAGE_SIZE - 4,
                _ => base + 32 + 17 * (ik - 5),
            };
            let output = match ok {
                0 => base + 2 * PAGE_SIZE + 32,
                1 => 0,
                2 => 1,
                3 => base + 3 * PAGE_SIZE,
                4 => base + 3 * PAGE_SIZE - 4,
                _ => base + 2 * PAGE_SIZE + 64 + 17 * (ok - 4),
            };
            if ok == 0 || ok >= 5 {
                p.space.w64(output, POISON).unwrap();
            }
            if ok == 4 {
                p.space.w32(output, POISON as u32).unwrap();
            }
            check(
                p,
                &mut t,
                [input, 0, field(&row, "mask", 16), output],
                field(&row, "status", 16),
            );
            // The separate guard oracle establishes publication before the
            // failed WoW64 copy across an inaccessible second output page.
            let copy_failed_after_creation = arch == WinArch::X86
                && ok == 4
                && (ik == 0 || ik >= 5)
                && field(&row, "mask", 16) <= 0x1F;
            assert_eq!(
                p.objects.wnf_subscription(KNOWN).is_some(),
                row["status"] == "00000000" || copy_failed_after_creation,
                "{arch}: {row:?}"
            );
            if ok == 0 || ok >= 5 {
                if row["status"] == "00000000" {
                    assert_eq!(
                        p.space.u64(output).unwrap(),
                        p.objects.wnf_subscription(KNOWN).unwrap().id
                    );
                } else {
                    assert_eq!(p.space.u64(output).unwrap(), field(&row, "after", 16));
                }
            }
            if ok == 4 {
                assert_eq!(p.space.u32(output).unwrap(), POISON as u32);
            }
            count += 1;
        }
    }
    assert_eq!(count, 2496);
}
#[test]
fn native_wnf_subscription_replays_all_256_mask_updates_with_stable_identity() {
    let mut count = 0;
    for (arch, text) in profiles() {
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let input = t.cpu.sp() - 128;
        let output = input + 32;
        p.space.w64(input, KNOWN).unwrap();
        for row in rows(text)
            .into_iter()
            .filter(|r| r.contains_key("sequence-case"))
        {
            p.objects.unsubscribe_wnf(KNOWN);
            check(p, &mut t, [input, 0, field(&row, "first", 16), output], 0);
            let id = p.space.u64(output).unwrap();
            check(
                p,
                &mut t,
                [
                    input,
                    u64::from(u32::MAX),
                    field(&row, "second", 16),
                    output,
                ],
                0,
            );
            let sub = p.objects.wnf_subscription(KNOWN).unwrap();
            assert_eq!(p.space.u64(output).unwrap(), id);
            assert_eq!(sub.id, id);
            assert_eq!(sub.change_stamp, u32::MAX);
            assert_eq!(sub.event_mask & 1 != 0, row["present"] == "00000001");
            assert!(p.objects.unsubscribe_wnf(KNOWN));
            assert!(!p.objects.unsubscribe_wnf(KNOWN));
            count += 1;
        }
    }
    assert_eq!(count, 256);
}
#[test]
fn native_wnf_subscription_pending_metadata_matches_128_native_queue_captures_all_abis() {
    let text = include_str!(
        "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-pending-arm64.log"
    );
    let cases = rows(text);
    assert_eq!(cases.len(), 129);
    let descriptors: HashMap<_, _> = text
        .lines()
        .filter(|l| l.starts_with("descriptor "))
        .map(|line| {
            let row: HashMap<_, _> = line
                .split_whitespace()
                .skip(1)
                .map(|f| f.split_once('=').unwrap())
                .collect();
            ((row["case"], row["index"]), row)
        })
        .collect();
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let input = t.cpu.sp() - 128;
        p.space.w64(input, KNOWN).unwrap();
        for row in cases.iter().filter(|r| r["warmup"] == "0") {
            p.objects.unsubscribe_wnf(KNOWN);
            check(p, &mut t, [input, 0, field(row, "mask", 16), 0], 0);
            check(
                p,
                &mut t,
                [input, u64::from(u32::MAX), field(row, "next-mask", 16), 0],
                0,
            );
            let descriptor = &descriptors[&(row["case"], "1")];
            let pending = if descriptor["status"] == "00000000" {
                let raw = crate::user::windows::wnf::tests::hex(descriptor["bytes"]);
                assert_eq!(&raw[16..24], &[0; 8]);
                assert_eq!(&raw[28..44], &[0; 16]);
                assert_eq!(u32::from_le_bytes(raw[44..48].try_into().unwrap()), 48);
                u32::from_le_bytes(raw[24..28].try_into().unwrap())
            } else {
                assert_eq!(descriptor["status"], "8000001A");
                0
            };
            assert_eq!(
                p.objects.wnf_subscription(KNOWN).unwrap().pending_mask,
                pending,
                "{arch}: {row:?}"
            );
        }
    }
}
#[test]
fn native_wnf_subscription_requires_selection_and_preserves_fixed_64_bit_output_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        let input = t.cpu.sp() - 128;
        let output = input + 32;
        p.space.w64(input, KNOWN).unwrap();
        p.space.w64(output, POISON).unwrap();
        assert!(
            matches!(call(p, &mut t, [input, 0, 0x11, output]), Outcome::Fail(reason) if reason.contains("metadata was not selected"))
        );
        assert_eq!(p.space.u64(output).unwrap(), 0);
        p.objects.select_wnf_catalog(catalog());
        check(
            p,
            &mut t,
            [input, 0x123400000000, 0x432100000011, output],
            0,
        );
        let sub = p.objects.wnf_subscription(KNOWN).unwrap();
        assert_eq!(sub.change_stamp, 0);
        assert_eq!(sub.event_mask, 0x11);
        p.objects.drain().for_each(drop);
        assert!(p.objects.wnf_subscription(KNOWN).is_none());
        check(p, &mut t, [input, 0, 0x11, output], 0);
    }
}

fn unsubscribe_call(p: &mut Proc, t: &mut Thread, input: u64, expected: u64) {
    let saved = p.modules.nt_services.take().unwrap();
    p.modules.nt_services = Some((
        saved.0,
        table(p.arch, "NtUnsubscribeWnfStateChange", SERVICE),
    ));
    check(p, t, [input, 0xFFFF0000, 0xFFFF0000, 0xFFFF0000], expected);
    p.modules.nt_services = Some(saved);
}
#[test]
fn native_wnf_subscription_replays_all_228_overlapping_output_cases() {
    let profiles = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-alias-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-alias-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-alias-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-alias-x86-laa.log"
            ),
        ),
    ];
    let mut count = 0;
    for (arch, text) in profiles {
        assert!(text.contains("complete alias-cases=57 data-pairs=8"));
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let base = t.cpu.sp() - 192;
        for row in rows(text) {
            p.objects.unsubscribe_wnf(KNOWN);
            p.space.wr(base, &[0xA5; 64]).unwrap();
            let input = base + 24;
            p.space.w64(input, KNOWN).unwrap();
            let delta: i64 = row["delta"].parse().unwrap();
            let output = input.checked_add_signed(delta).unwrap();
            check(
                p,
                &mut t,
                [input, 0, field(&row, "mask", 16), output],
                field(&row, "status", 16),
            );
            let start = (24 + delta) as usize;
            let expected_name = |id: u64| {
                let mut memory = [0xA5u8; 64];
                memory[24..32].copy_from_slice(&KNOWN.to_le_bytes());
                memory[start..start + 8].copy_from_slice(&id.to_le_bytes());
                u64::from_le_bytes(memory[24..32].try_into().unwrap())
            };
            assert_eq!(
                field(&row, "name-after", 16),
                expected_name(field(&row, "id", 16)),
                "native write footprint"
            );
            assert_eq!(
                p.space.u64(input).unwrap(),
                expected_name(p.space.u64(output).unwrap())
            );
            if row["status"] == "00000000" {
                assert_eq!(
                    p.space.u64(output).unwrap(),
                    p.objects.wnf_subscription(KNOWN).unwrap().id
                );
            } else {
                assert_eq!(p.space.u64(output).unwrap(), field(&row, "id", 16));
            }
            p.space.w64(base + 80, KNOWN).unwrap();
            unsubscribe_call(p, &mut t, base + 80, field(&row, "end", 16));
            count += 1;
        }
    }
    assert_eq!(count, 228);
}
#[test]
fn native_wnf_subscription_fresh_event_order_and_closed_caller_pin_all_abis() {
    let text = include_str!(
        "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-fresh-arm64.log"
    );
    assert!(text.contains("complete cases=64 warmup=1"));
    let observed = rows(text);
    assert_eq!(observed.len(), 65);
    for arch in WinArch::ALL {
        for row in observed.iter().filter(|r| r["warmup"] == "0") {
            let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
            let p = process.state_mut();
            p.objects.select_wnf_catalog(catalog());
            let input = t.cpu.sp() - 128;
            p.space.w64(input, KNOWN).unwrap();
            let handle = p.objects.insert(Object::Event {
                manual: false,
                signaled: 0,
            }) as u64;
            let event = p.objects.id(handle).unwrap();
            let after = row["after"] == "1";
            if after {
                assert_eq!(p.objects.register_wnf_event(event), Ok(true));
                p.objects.close(handle).unwrap();
            }
            check(p, &mut t, [input, 0, field(row, "mask", 16), 0], 0);
            if !after {
                assert_eq!(p.objects.register_wnf_event(event), Ok(true));
                p.objects.close(handle).unwrap();
            }
            let sub = p.objects.wnf_subscription(KNOWN).unwrap().clone();
            let state = |p: &Proc| match p.objects.obj(event).unwrap() {
                Object::Event { manual, signaled } => {
                    assert!(!manual);
                    *signaled
                }
                _ => panic!("event owner changed"),
            };
            assert_eq!(state(p), row["state2"].parse::<i32>().unwrap());
            check(
                p,
                &mut t,
                [input, u64::from(u32::MAX), field(row, "mask", 16), 0],
                0,
            );
            assert_eq!(state(p), row["state3"].parse::<i32>().unwrap());
            assert_eq!(p.objects.wnf_subscription(KNOWN).unwrap().id, sub.id);
            unsubscribe_call(p, &mut t, input, 0);
            assert_eq!(state(p), row["state4"].parse::<i32>().unwrap());
            unsubscribe_call(p, &mut t, input, STATUS_OBJECT_NAME_NOT_FOUND.into());
            unsubscribe_call(p, &mut t, 0, STATUS_ACCESS_VIOLATION.into());
            p.objects.drain().for_each(drop);
            assert!(p.objects.obj(event).is_none());
        }
    }
}
#[test]
fn native_wnf_subscription_guard_consumption_and_denied_update_preserve_record_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let base =
            p.vm.allocate(
                None,
                2 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let output = base + PAGE_SIZE;
        p.space.w64(base, KNOWN).unwrap();
        p.space.w64(output, POISON).unwrap();
        p.vm.protect(output, PAGE_SIZE, prot::READWRITE | prot::GUARD)
            .unwrap();
        check(
            p,
            &mut t,
            [base, 0, 0x11, output],
            STATUS_GUARD_PAGE_VIOLATION.into(),
        );
        assert!(p.objects.wnf_subscription(KNOWN).is_none());
        assert_eq!(p.space.u64(output).unwrap(), POISON);
        check(p, &mut t, [base, 0, 0x11, output], 0);
        let original = p.objects.wnf_subscription(KNOWN).unwrap().clone();
        check(
            p,
            &mut t,
            [base, 17, 0x20, output],
            STATUS_INVALID_PARAMETER.into(),
        );
        assert_eq!(p.objects.wnf_subscription(KNOWN).unwrap(), &original);
        assert_eq!(p.space.u64(output).unwrap(), 0);
        p.space.w64(base, 0x41C61629A3BC30B5).unwrap();
        check(p, &mut t, [base, 0, 4, output], STATUS_ACCESS_DENIED.into());
        assert_eq!(p.objects.wnf_subscription(KNOWN).unwrap(), &original);
        assert!(p.objects.wnf_subscription(0x41C61629A3BC30B5).is_none());
    }
}

#[test]
fn native_wnf_subscription_replays_all_840_guard_and_readonly_copy_cases() {
    let profiles = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-guards-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-guards-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-guards-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-guards-x86-laa.log"
            ),
        ),
    ];
    let mut count = 0;
    for (arch, text) in profiles {
        assert!(text.contains("complete cases=210"));
        let observed = rows(text);
        assert_eq!(observed.len(), 210);
        let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
        let p = process.state_mut();
        p.objects.select_wnf_catalog(catalog());
        let base =
            p.vm.allocate(
                None,
                3 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let valid = t.cpu.sp() - 128;
        let invalid = valid + 16;
        let local = valid + 32;
        p.space.w64(valid, KNOWN).unwrap();
        p.space.w64(invalid, 0).unwrap();
        for row in observed {
            p.vm.protect(base, 3 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.w64(base, KNOWN).unwrap();
            p.space.w64(base + PAGE_SIZE, POISON).unwrap();
            p.space.wr(base + 2 * PAGE_SIZE - 4, &[0xA5; 11]).unwrap();
            p.space.w64(local, POISON).unwrap();
            p.objects.unsubscribe_wnf(KNOWN);
            let ik = field(&row, "input", 10);
            let ok = field(&row, "output", 10);
            let input = match ik {
                0 => valid,
                1 | 2 => base,
                3 => 0,
                4 => invalid,
                _ => unreachable!(),
            };
            let output = match ok {
                0 => local,
                3 => 0,
                5..=13 => {
                    base + 2 * PAGE_SIZE
                        - if ok < 8 {
                            4
                        } else if ok < 11 {
                            1
                        } else {
                            3
                        }
                }
                _ => base + PAGE_SIZE,
            };
            if ik == 1 || ik == 2 {
                p.vm.protect(
                    base,
                    PAGE_SIZE,
                    if ik == 1 {
                        prot::READWRITE | prot::GUARD
                    } else {
                        prot::NOACCESS
                    },
                )
                .unwrap();
            }
            if matches!(ok, 1 | 2 | 4) {
                p.vm.protect(
                    base + PAGE_SIZE,
                    PAGE_SIZE,
                    match ok {
                        1 => prot::READWRITE | prot::GUARD,
                        2 => prot::NOACCESS,
                        _ => prot::READONLY,
                    },
                )
                .unwrap();
            }
            if ok >= 5 {
                p.vm.protect(
                    base + 2 * PAGE_SIZE,
                    PAGE_SIZE,
                    match (ok - 5) % 3 {
                        0 => prot::READONLY,
                        1 => prot::READWRITE | prot::GUARD,
                        _ => prot::NOACCESS,
                    },
                )
                .unwrap();
            }
            check(
                p,
                &mut t,
                [input, 0, field(&row, "mask", 16), output],
                field(&row, "status", 16),
            );
            for (page, label) in [
                (0, "input-protect"),
                (1, "output-protect"),
                (2, "upper-protect"),
            ] {
                assert_eq!(
                    u64::from(p.vm.query(base + page * PAGE_SIZE).unwrap().protect),
                    field(&row, label, 16),
                    "{arch}: {row:?}"
                );
            }
            p.vm.protect(base, 3 * PAGE_SIZE, prot::READWRITE).unwrap();
            let actual = p
                .space
                .u64(if output == 0 { local } else { output })
                .unwrap();
            let expected = field(&row, "after", 16);
            if expected == 0 || expected == POISON {
                assert_eq!(actual, expected, "{arch}: {row:?}");
            } else {
                assert_eq!(actual, p.objects.wnf_subscription(KNOWN).unwrap().id);
                assert_ne!(actual, 0);
            }
            let created = row["end"] == "00000000";
            assert_eq!(
                p.objects.wnf_subscription(KNOWN).is_some(),
                created,
                "{arch}: {row:?}"
            );
            unsubscribe_call(p, &mut t, valid, field(&row, "end", 16));
            count += 1;
        }
    }
    assert_eq!(count, 840);
}

#[test]
fn native_wnf_subscription_replays_288_fresh_process_names_and_metadata_all_abis() {
    let text = include_str!(
        "../../../../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-process-fresh-arm64.log"
    );
    assert!(text.contains("complete cases=288 names=9 warmup=1"));
    let observed = rows(text);
    assert_eq!(observed.len(), 289);
    let descriptors: HashMap<_, _> = text
        .lines()
        .filter(|l| l.starts_with("descriptor "))
        .map(|line| {
            let row: HashMap<_, _> = line
                .split_whitespace()
                .skip(1)
                .map(|f| f.split_once('=').unwrap())
                .collect();
            ((row["case"], row["index"]), row)
        })
        .collect();
    assert_eq!(descriptors.len(), 578);
    for arch in WinArch::ALL {
        for row in observed.iter().filter(|r| r["warmup"] == "0") {
            let (mut process, mut t) = fixture(arch, "NtSubscribeWnfStateChange", SERVICE);
            let p = process.state_mut();
            p.objects.select_wnf_catalog(catalog());
            let input = t.cpu.sp() - 128;
            let output = input + 32;
            let name = field(row, "name", 16);
            p.space.w64(input, name).unwrap();
            p.space.w64(output, POISON).unwrap();
            let handle = p.objects.insert(Object::Event {
                manual: false,
                signaled: 0,
            }) as u64;
            let event = p.objects.id(handle).unwrap();
            assert_eq!(p.objects.register_wnf_event(event), Ok(true));
            p.objects.close(handle).unwrap();
            check(
                p,
                &mut t,
                [input, 0, field(row, "mask", 16), output],
                field(row, "status", 16),
            );
            let descriptor = &descriptors[&(row["case"], "0")];
            let expected_pending = if descriptor["status"] == "00000000" {
                let raw = crate::user::windows::wnf::tests::hex(descriptor["bytes"]);
                assert_eq!(u64::from_le_bytes(raw[8..16].try_into().unwrap()), name);
                assert_eq!(&raw[16..24], &[0; 8]);
                assert_eq!(&raw[28..44], &[0; 16]);
                assert_eq!(u32::from_le_bytes(raw[44..48].try_into().unwrap()), 48);
                u32::from_le_bytes(raw[24..28].try_into().unwrap())
            } else {
                assert_eq!(descriptor["status"], "8000001A");
                0
            };
            let id = p.space.u64(output).unwrap();
            if row["status"] == "00000000" {
                let sub = p.objects.wnf_subscription(name).unwrap();
                assert_eq!(sub.id, id);
                assert_ne!(id, 0);
                assert_eq!(sub.pending_mask, expected_pending);
            } else {
                assert_eq!(id, 0);
                assert_eq!(expected_pending, 0);
                assert!(p.objects.wnf_subscription(name).is_none());
            }
            match p.objects.obj(event).unwrap() {
                Object::Event { manual, signaled } => {
                    assert!(!manual);
                    assert_eq!(*signaled, row["state2"].parse::<i32>().unwrap());
                }
                _ => panic!("event owner changed"),
            }
            check(
                p,
                &mut t,
                [
                    input,
                    u64::from(u32::MAX),
                    field(row, "next-mask", 16),
                    output,
                ],
                field(row, "repeat", 16),
            );
            assert_eq!(p.space.u64(output).unwrap(), id);
            unsubscribe_call(p, &mut t, input, field(row, "end", 16));
            p.objects.drain().for_each(drop);
            assert!(p.objects.obj(event).is_none());
        }
    }
}
