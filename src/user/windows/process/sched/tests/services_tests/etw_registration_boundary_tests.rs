//! Independent native protection/probe ordering for private function15.
use super::*;
#[test]
fn native_etw_registration_replays_all_22400_fault_guard_and_alignment_rows() {
    let profiles = [
        (
            WinArch::Arm64,
            corpus!("native-etw-register-guids-boundaries-arm64.log"),
        ),
        (
            WinArch::X64,
            corpus!("native-etw-register-guids-boundaries-x64.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-etw-register-guids-boundaries-x86.log"),
        ),
        (
            WinArch::X86,
            corpus!("native-etw-register-guids-boundaries-x86-laa.log"),
        ),
    ];
    let mut total = 0;
    for (arch, text) in profiles {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 5600);
        assert!(text.contains("complete cases=5600"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 6);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        let request = header();
        for row in observed {
            p.vm.protect(b, 6 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space
                .wr(b, &vec![0xA5; (6 * PAGE_SIZE) as usize])
                .unwrap();
            let ik = field(&row, "input", 10);
            let rk = field(&row, "returned", 10);
            let ok = field(&row, "output", 10);
            let local = b + 5 * PAGE_SIZE;
            let input = match ik {
                0 => local,
                1 => 0,
                2 => 1,
                6 => b + PAGE_SIZE - 80,
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
            p.space.wr(b + PAGE_SIZE - 80, &request).unwrap();
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
                    15,
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
                    "{key} row={row:?}"
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
            if expected == 0 {
                let h = p.space.u64(output + 24).unwrap();
                registration(p, h, &request);
                p.objects.close(h).unwrap();
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 22400);
}
