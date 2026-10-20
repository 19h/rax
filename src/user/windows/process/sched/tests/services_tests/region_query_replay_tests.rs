//! Replay native copy/fault observations. Allocation coordinates are masked;
//! independent allocation/image lifecycle tests validate their quantities.
use super::{Outcome, WinArch, arguments, dispatch, read_ptr, setup};
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::memory::{AllocKind, Mem, prot};
use crate::user::windows::objects::Object;

fn number(row: &serde_json::Value, key: &str) -> u64 {
    row[key].as_u64().unwrap()
}

fn decode(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn region_queries_replay_1608_native_prefix_fault_guard_and_rights_observations() {
    let data: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-region-information/modelled-query-cases.json"
    )))
    .unwrap();
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1608);
    for row in rows {
        let profile = row["profile"].as_str().unwrap();
        let arch = match profile {
            "arm64" => WinArch::Arm64,
            "x64" => WinArch::X64,
            "x86" | "x86-laa" => WinArch::X86,
            _ => panic!("native profile"),
        };
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        let image = p.modules.exe().base;
        let reserved =
            p.vm.reserve(None, 8192, prot::READWRITE, AllocKind::Private, false, None)
                .unwrap();
        let mut handle = arch.ptr(u64::MAX);
        let mut address = image;
        let mut output = base + 128;
        let mut returned = base + 256;
        let length = number(row, "length");
        let group = row["group"].as_str().unwrap();
        if group == "matrix" {
            address = match number(row, "target") {
                0 | 1 => image,
                2 => image + 1,
                3 => base + 8192,
                4 => 0,
                5 => 1,
                6 => arch.ptr(u64::MAX),
                _ => panic!("native matrix target"),
            };
            p.vm.poke(
                returned,
                &u64::MAX.to_le_bytes()[..arch.ptr_size() as usize],
            )
            .unwrap();
        } else if group == "rights" {
            let object = p.objects.create(Object::Process {
                pid: p.pid,
                exit_code: None,
            });
            handle = p
                .objects
                .open_access(object, false, number(row, "role") as u32)
                .unwrap()
                .into();
        } else {
            match number(row, "role") {
                0 => {}
                1 => output = base + 132,
                2 => output = base + 130,
                3 | 4 => {
                    output = base + 4096 - if arch == WinArch::X86 { 28 } else { 48 };
                    let protection = if number(row, "role") == 3 {
                        prot::NOACCESS
                    } else {
                        prot::READWRITE | prot::GUARD
                    };
                    p.vm.protect(base + 4096, PAGE_SIZE, protection).unwrap();
                }
                5 => output = 0,
                6 => {
                    output = 0;
                    handle = 0;
                }
                7 => address = 0,
                8 => address = reserved,
                9 => {
                    address = reserved;
                    output = 0;
                }
                10 => address = arch.ptr(u64::MAX),
                11 => {
                    address = arch.ptr(u64::MAX);
                    returned = 1;
                }
                12 => {
                    address = 0;
                    returned = 1;
                }
                13 => {
                    address = 0;
                    output = 1;
                }
                14 => {
                    handle = 0;
                    output = base + 4096;
                    p.vm.protect(output, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                15 => {
                    handle = 0;
                    returned = base + 4096;
                    p.vm.protect(returned, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                16 | 17 => {
                    output = base + 4096;
                    returned = base + 8192;
                    p.vm.protect(output, 2 * PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                18 => {
                    output = base + 4096 - length;
                    p.vm.protect(base + 4096, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                19 => {
                    returned = base + 4096 - arch.ptr_size() + 1;
                    p.vm.protect(base + 4096, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                20 | 21 => {
                    address = base + 4096;
                    let protection = if number(row, "role") == 20 {
                        prot::NOACCESS
                    } else {
                        prot::READWRITE | prot::GUARD
                    };
                    p.vm.protect(address, PAGE_SIZE, protection).unwrap();
                }
                22 => returned = output,
                23 | 24 | 25 | 26 => {
                    match number(row, "role") {
                        23 => output = 1,
                        24 => output = 0,
                        25 => address = 0,
                        26 => address = arch.ptr(u64::MAX),
                        _ => unreachable!(),
                    }
                    returned = base + 4096;
                    p.vm.protect(returned, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                27 => {
                    handle = 0;
                    returned = base + 4096;
                    output = base + 8192;
                    p.vm.protect(returned, 2 * PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                28 | 29 => {}
                30 => address = 1,
                31 => address = image,
                32 => address = base + 12288,
                33 => address = reserved + 4096,
                34 => {
                    address = base + 8192;
                    returned = 0;
                }
                35 => {
                    output = base + 4096 - length + 1;
                    p.vm.protect(base + 4096, PAGE_SIZE, prot::NOACCESS)
                        .unwrap();
                }
                36 | 37 => {
                    output = base + 4096;
                    returned = output + if number(row, "role") == 36 { 64 } else { 0 };
                    p.vm.protect(output, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                _ => panic!("native boundary role"),
            }
        }
        arguments(
            p,
            &mut thread,
            &[
                handle,
                address,
                number(row, "class"),
                output,
                length,
                returned,
            ],
        );
        assert_eq!(dispatch(p, &mut thread), Outcome::Continue, "{row}");
        assert_eq!(
            thread.cpu.gpr(0) as u32,
            number(row, "status") as u32,
            "{row}"
        );
        if group == "matrix" {
            assert_eq!(
                read_ptr(p, base + 256),
                number(row, "returned-value"),
                "{row}"
            );
        } else {
            let guards: String = [4096, 8192]
                .into_iter()
                .map(|offset| {
                    if p.vm.query(base + offset).unwrap().protect & prot::GUARD != 0 {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect();
            assert_eq!(guards, row["guards"].as_str().unwrap(), "{row}");
            assert_eq!(
                p.space.bytes(base + 256, 16).unwrap(),
                decode(row["returned-bytes"].as_str().unwrap()),
                "{row}"
            );
        }
        let expected = decode(row["bytes"].as_str().unwrap());
        let actual = p.space.bytes(base + 128, 64).unwrap();
        let mut masked = [false; 64];
        for index in row["masked-byte-indices"].as_array().unwrap() {
            masked[index.as_u64().unwrap() as usize] = true;
        }
        for index in 0..64 {
            if !masked[index] {
                assert_eq!(actual[index], expected[index], "byte {index}: {row}");
            }
        }
    }
}
