//! Native base and referenced pointer faults, guard order and aliased snapshots.
use super::*;
#[test]
fn native_etw_provider_traits_replays_all_22400_base_fault_guard_and_alignment_rows() {
    let mut total = 0;
    for (arch, text) in profiles!("boundaries") {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 5600);
        assert!(text.contains("complete cases=5600"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 6);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed {
            p.vm.protect(b, 6 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(b, &vec![0xA5; 6 * PAGE_SIZE as usize]).unwrap();
            let local = b + 5 * PAGE_SIZE;
            let h = own(p);
            let request = packet(h, local + 1024, 7, 0);
            p.space.wr(local + 1024, b"\x07\0name\0").unwrap();
            let ik = field(&row, "input", 10);
            let rk = field(&row, "returned", 10);
            let ok = field(&row, "output", 10);
            let input = match ik {
                0 => local,
                1 => 0,
                2 => 1,
                6 => b + PAGE_SIZE - 12,
                7..=9 => local + ik - 6,
                _ => b,
            };
            let returned = match rk {
                0 => local + 256,
                1 => 0,
                2 => 1,
                6 => b + 3 * PAGE_SIZE - 2,
                _ => b + 2 * PAGE_SIZE,
            };
            let output = match ok {
                0 => 0,
                1 => local + 512,
                _ => b + 4 * PAGE_SIZE,
            };
            p.space
                .wr(local + if ik >= 7 { ik - 6 } else { 0 }, &request)
                .unwrap();
            p.space.wr(b, &request).unwrap();
            p.space.wr(b + PAGE_SIZE - 12, &request).unwrap();
            for (kind, at) in [(ik, b), (rk, b + 2 * PAGE_SIZE)] {
                if (3..=5).contains(&kind) {
                    p.vm.protect(
                        at,
                        PAGE_SIZE,
                        match kind {
                            3 => prot::READWRITE | prot::GUARD,
                            4 => prot::NOACCESS,
                            _ => prot::READONLY,
                        },
                    )
                    .unwrap();
                }
            }
            if ik == 6 {
                p.vm.protect(b + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            if rk == 6 {
                p.vm.protect(b + 3 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            if ok >= 2 {
                p.vm.protect(
                    b + 4 * PAGE_SIZE,
                    PAGE_SIZE,
                    match ok {
                        2 => prot::NOACCESS,
                        3 => prot::READWRITE | prot::GUARD,
                        _ => prot::READONLY,
                    },
                )
                .unwrap();
            }
            let expected = field(&row, "status", 16);
            check(
                p,
                &mut t,
                [
                    30,
                    input,
                    field(&row, "input-length", 10),
                    output,
                    field(&row, "output-length", 10),
                    returned,
                ],
                expected,
            );
            for (key, at) in [
                ("input-protect", b),
                ("returned-protect", b + 2 * PAGE_SIZE),
                ("returned-upper-protect", b + 3 * PAGE_SIZE),
                ("output-protect", b + 4 * PAGE_SIZE),
            ] {
                assert_eq!(
                    p.vm.query(at).unwrap().protect as u64,
                    field(&row, key, 16),
                    "{key} {row:?}"
                );
            }
            p.vm.protect(b, 6 * PAGE_SIZE, prot::READWRITE).unwrap();
            if rk != 1 && rk != 2 {
                assert_eq!(
                    p.space.u32(returned).unwrap() as u64,
                    field(&row, "return-after", 16)
                );
            }
            if ok != 0 {
                assert_eq!(
                    p.space.u32(output).unwrap() as u64,
                    field(&row, "output-after", 16)
                );
            }
            assert_eq!(
                stored(p, h),
                if expected == 0 {
                    Some(&b"\x07\0name\0"[..])
                } else {
                    None
                }
            );
            p.space.wr(local, &request).unwrap();
            p.space.w32(local + 256, 0xA5A5A5A5).unwrap();
            check(
                p,
                &mut t,
                [30, local, 24, local + 512, 120, local + 256],
                field(&row, "retry", 16),
            );
            assert_eq!(
                p.space.u32(local + 256).unwrap() as u64,
                field(&row, "retry-returned", 16)
            );
            assert_eq!(stored(p, h), Some(&b"\x07\0name\0"[..]));
            p.objects.close(h).unwrap();
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 22400);
}
#[test]
fn native_etw_provider_traits_replays_all_3960_referenced_faults_type_busy_and_status_translations()
{
    let mut total = 0;
    for (arch, text) in profiles!("reference") {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 990);
        assert!(text.contains("complete cases=990"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 24);
        let local = b + 21 * PAGE_SIZE;
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed {
            p.vm.protect(b, 24 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(b, &vec![0; 20 * PAGE_SIZE as usize]).unwrap();
            p.space.wr(local + 512, &[3, 0, 0]).unwrap();
            p.space.wr(local + 128, &[0xA5; 120]).unwrap();
            let h = own(p);
            let busy = field(&row, "busy", 10);
            if busy != 0 {
                p.space.wr(local, &packet(h, local + 512, 3, 0)).unwrap();
                check(p, &mut t, [30, local, 24, local + 128, 120, local + 256], 0);
            }
            let hm = field(&row, "handle-mode", 10);
            let mut selected = h;
            let mut other = None;
            match hm {
                1 => selected = 0,
                2 => {
                    selected = p
                        .objects
                        .insert(Object::Event {
                            manual: false,
                            signaled: 0,
                        })
                        .into();
                    other = Some(selected);
                }
                3 => {
                    p.objects.close(h).unwrap();
                }
                4 => selected |= 1u64 << 32,
                _ => {}
            }
            let tk = field(&row, "traits", 10);
            let length = field(&row, "length", 10);
            let address = b + if tk == 6 {
                PAGE_SIZE - 2
            } else if (7..=9).contains(&tk) {
                tk - 6
            } else {
                0
            };
            p.space.w16(address, length as u16).unwrap();
            if (3..=65535).contains(&length) {
                p.space.w8(address + length - 1, 0).unwrap();
            }
            if (3..=5).contains(&tk) {
                p.vm.protect(
                    b,
                    PAGE_SIZE,
                    match tk {
                        3 => prot::READWRITE | prot::GUARD,
                        4 => prot::NOACCESS,
                        _ => prot::READONLY,
                    },
                )
                .unwrap();
            }
            if tk == 6 {
                p.vm.protect(b + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            }
            let mut pointer = match tk {
                1 => 0,
                2 => 1,
                _ => address,
            };
            if tk == 10 {
                pointer |= 1u64 << 48;
            }
            p.space
                .wr(local, &packet(selected, pointer, length as u32, 0))
                .unwrap();
            p.space.w32(local + 256, 0xA5A5A5A5).unwrap();
            let status = field(&row, "status", 16);
            assert_eq!(
                call(p, &mut t, [30, local, 24, local + 128, 120, local + 256]),
                Outcome::Continue,
                "{row:?}"
            );
            assert_eq!(t.cpu.gpr(0), status, "{row:?}");
            assert_eq!(
                p.space.u32(local + 256).unwrap() as u64,
                field(&row, "returned", 16)
            );
            assert_eq!(
                p.vm.query(b).unwrap().protect as u64,
                field(&row, "protect", 16)
            );
            assert_eq!(
                crate::user::windows::nt::status_to_error(status as u32) as u64,
                field(&row, "error", 10)
            );
            p.vm.protect(b, 20 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(local, &packet(h, local + 512, 3, 0)).unwrap();
            p.space.w32(local + 256, 0xA5A5A5A5).unwrap();
            check(
                p,
                &mut t,
                [30, local, 24, local + 128, 120, local + 256],
                field(&row, "retry", 16),
            );
            assert_eq!(
                p.space.u32(local + 256).unwrap() as u64,
                field(&row, "retry-returned", 16)
            );
            assert_eq!(p.space.bytes(local + 128, 120).unwrap(), [0xA5; 120]);
            if hm != 3 {
                assert_eq!(stored(p, h), Some(&[3, 0, 0][..]));
                p.objects.close(h).unwrap();
            }
            if let Some(h) = other {
                p.objects.close(h).unwrap();
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 3960);
}
#[test]
fn native_etw_provider_traits_replays_all_3456_aliases_and_retains_pre_store_snapshot() {
    let mut total = 0;
    for (arch, text) in profiles!("alias") {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 864);
        assert!(text.contains("complete cases=864"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 1);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed {
            let h = own(p);
            let mut before = hex(row["before"]);
            let native_before = before.clone();
            let mut native_expected = native_before.clone();
            let rb = field(&row, "returned-base", 10);
            let rd = row["returned-delta"].parse::<i64>().unwrap();
            let od = row["output-delta"].parse::<i64>().unwrap();
            let at = ((if rb == 0 { 512 } else { 256 }) + rd) as usize;
            native_expected[at - 224..at - 220].fill(0);
            assert_eq!(native_expected, hex(row["after"]));
            before[288..296].copy_from_slice(&h.to_le_bytes());
            before[296..304].copy_from_slice(&(b + 256).to_le_bytes());
            let length = field(&row, "length", 10) as usize;
            let captured = before[32..32 + length].to_vec();
            p.space.wr(b + 224, &before).unwrap();
            let request = packet(h, b + 256, length as u32, 0);
            check(
                p,
                &mut t,
                [
                    30,
                    b + 512,
                    24,
                    (b as i64 + 512 + od) as u64,
                    120,
                    b + at as u64,
                ],
                0,
            );
            let mut expected = before;
            expected[at - 224..at - 220].fill(0);
            assert_eq!(p.space.bytes(b + 224, 576).unwrap(), expected);
            assert_eq!(stored(p, h), Some(captured.as_slice()));
            p.space.wr(b + 1024, &request).unwrap();
            p.space.w32(b + 1536, 0xA5A5A5A5).unwrap();
            check(
                p,
                &mut t,
                [30, b + 1024, 24, b + 1280, 120, b + 1536],
                field(&row, "retry", 16),
            );
            assert_eq!(
                p.space.u32(b + 1536).unwrap() as u64,
                field(&row, "retry-returned", 16)
            );
            p.objects.close(h).unwrap();
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 3456);
}
