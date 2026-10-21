//! Native v10 boundary observations distinguish probing, redirection and stores.
use super::Harness;
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::memory::{Mem, prot};
use crate::user::windows::nt::status::STATUS_SUCCESS;
use crate::user::windows::objects::Object;

fn oracle() -> serde_json::Value {
    serde_json::from_str(include_str!("boundaries-v10.json")).unwrap()
}

fn protection(h: &Harness, address: u64) -> u32 {
    h.process.state().vm.query(address).map_or(0, |r| r.protect)
}

fn observed(h: &Harness, args: &[u64; 6], code: u32) -> serde_json::Value {
    let width = h.process.state().arch.ptr_size();
    let mut output = serde_json::json!({"status":code,"hprot":protection(h,args[0]),"iprot":protection(h,args[3])});
    for (address, field, length) in [(args[0], "hout", width), (args[3], "iosb", 2 * width)] {
        // Match the producer's non-consuming VirtualQuery observation: the
        // entire record must fit within this one readable protection region.
        let readable = h.process.state().vm.query(address).is_some_and(|region| {
            region.protect != 0
                && region.protect & (prot::GUARD | prot::NOACCESS) == 0
                && address >= region.base
                && address
                    .checked_add(length)
                    .is_some_and(|end| end <= region.base + region.size)
        });
        output[field] = if !readable {
            serde_json::json!("-")
        } else {
            let bytes = h
                .process
                .state()
                .space
                .bytes(address, length as usize)
                .unwrap();
            if field == "hout" {
                let value = h.process.state().space.ptr(address, width).unwrap();
                serde_json::json!(if value == 0 {
                    "zero"
                } else if value == h.process.state().arch.ptr(0xa5a5a5a5a5a5a5a5) {
                    "sentinel"
                } else {
                    "published"
                })
            } else {
                serde_json::json!(bytes.iter().map(|b| format!("{b:02X}")).collect::<String>())
            }
        };
    }
    output
}

#[test]
fn native_file_open_redirection_and_guard_store_boundaries_match_native_v10() {
    let data = oracle();
    let mut failures = Vec::new();
    for arch in WinArch::ALL {
        for row in data["cases"].as_array().unwrap() {
            let role = row["role"].as_u64().unwrap();
            let mut h = Harness::new(arch);
            let width = arch.ptr_size();
            let root = if (404..=419).contains(&role) {
                match (role - 404) / 4 {
                    0 => 0x6ffc,
                    1 | 2 => {
                        let directory = (role - 404) / 4 == 1;
                        if directory {
                            h.name("\\??\\C:\\");
                        }
                        assert_eq!(
                            h.open_with(
                                h.base,
                                h.base + PAGE_SIZE,
                                0x120089,
                                7,
                                if directory { 0x21 } else { 0x60 }
                            ),
                            STATUS_SUCCESS
                        );
                        h.process.state().space.ptr(h.base, width).unwrap()
                    }
                    3 => u64::from(h.process.state_mut().objects.insert(Object::Event {
                        manual: true,
                        signaled: 0,
                    })),
                    _ => unreachable!(),
                }
            } else {
                0
            };
            h.process
                .state()
                .space
                .wr(h.base, &vec![0; 8 * PAGE_SIZE as usize])
                .unwrap();
            h.name("\\??\\C:\\fixture.bin");
            h.process.state().space.wr(h.base, &[0xa5; 32]).unwrap();
            h.process
                .state()
                .space
                .wr(h.base + PAGE_SIZE, &[0xa5; 32])
                .unwrap();
            let attrs = h.attrs();
            let missing = h.base + 4 * PAGE_SIZE;
            let guarded = h.base + 5 * PAGE_SIZE;
            let p = h.process.state_mut();
            p.vm.protect(missing, PAGE_SIZE, prot::NOACCESS).unwrap();
            p.vm.protect(guarded, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                .unwrap();
            p.space.wptr(attrs + width, width, root).unwrap();
            let mut args = [h.base, 0x100021, attrs, h.base + PAGE_SIZE, 5, 0x60];
            match role {
                404..=419 => match (role - 404) % 4 {
                    0 => p.space.wptr(attrs + 64 + width, width, missing).unwrap(),
                    1 => p.space.wptr(attrs + 64 + width, width, guarded).unwrap(),
                    2 => p.space.wptr(attrs + 64 + width, width, 1).unwrap(),
                    3 => p.space.w16(attrs + 64, 3).unwrap(),
                    _ => unreachable!(),
                },
                420..=423 => {
                    args[1] = 0;
                    args[5] = 0x40;
                    match role - 420 {
                        0 => args[0] = guarded,
                        1 => args[3] = guarded,
                        2 => args[2] = 1,
                        3 => p.space.w32(attrs, 0).unwrap(),
                        _ => unreachable!(),
                    }
                }
                424..=425 => {
                    p.vm.protect(
                        h.base + (role - 424) * PAGE_SIZE,
                        PAGE_SIZE,
                        prot::READONLY | prot::GUARD,
                    )
                    .unwrap();
                }
                428..=459 => {
                    let available = (role - 428) % 16;
                    args[3] = missing - available;
                    p.space
                        .wr(args[3], &vec![0xa5; available as usize])
                        .unwrap();
                    p.vm.protect(
                        missing,
                        PAGE_SIZE,
                        if role < 444 {
                            prot::READONLY | prot::GUARD
                        } else {
                            prot::READWRITE | prot::GUARD
                        },
                    )
                    .unwrap();
                }
                _ => panic!("unknown native boundary role {role}"),
            }
            let code = h.invoke(&args);
            let actual = observed(&h, &args, code);
            let expected = &row[if arch == WinArch::X86 {
                "wow"
            } else {
                "native"
            }];
            if &actual != expected {
                failures.push(serde_json::json!({"arch":arch.to_string(),"role":role,"actual":actual,"expected":expected}));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "native file-open boundary mismatches: {}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}

#[test]
fn native_file_open_retained_read_execute_handle_can_query_attributes_after_replacement() {
    for arch in WinArch::ALL {
        let mut h = Harness::new(arch);
        let output = h.base;
        let io = h.base + PAGE_SIZE;
        let width = arch.ptr_size();
        assert_eq!(h.open(output, io), STATUS_SUCCESS);
        let handle = h.process.state().space.ptr(output, width).unwrap();
        std::fs::rename(
            h.directory.0.join("fixture.bin"),
            h.directory.0.join("original.bin"),
        )
        .unwrap();
        std::fs::write(
            h.directory.0.join("fixture.bin"),
            b"replacement larger than fixture",
        )
        .unwrap();
        h.name("");
        let attrs = h.attrs();
        h.process
            .state()
            .space
            .wptr(attrs + width, width, handle)
            .unwrap();
        assert_eq!(
            h.invoke_api("NtQueryFullAttributesFile", &[attrs, output]),
            STATUS_SUCCESS,
            "{arch}"
        );
        assert_eq!(
            h.process.state().space.u64(output + 40).unwrap(),
            7,
            "{arch}"
        );
    }
}

#[test]
fn native_file_open_retained_directory_no_reparse_applies_to_each_child_component() {
    use crate::user::windows::nt::status::STATUS_REPARSE_POINT_ENCOUNTERED;
    for arch in WinArch::ALL {
        let mut h = Harness::new(arch);
        let target = h.directory.0.join("target");
        let alias = h.directory.0.join("alias");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("child.dat"), b"retained").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        #[cfg(windows)]
        {
            // NTFS junction creation does not require symbolic-link privilege.
            // Both paths and every affected object belong to this fixture.
            let result = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&alias)
                .arg(&target)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "private junction creation: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let output = h.base;
        let io = h.base + PAGE_SIZE;
        h.name("\\??\\C:\\");
        assert_eq!(h.open_with(output, io, 0x120089, 7, 0x21), STATUS_SUCCESS);
        let root = h
            .process
            .state()
            .space
            .ptr(output, arch.ptr_size())
            .unwrap();
        for (flags, expected) in [
            (0x840, STATUS_SUCCESS),
            (0x1840, STATUS_REPARSE_POINT_ENCOUNTERED),
        ] {
            h.name("alias\\child.dat");
            let attrs = h.attrs();
            h.process
                .state()
                .space
                .wptr(attrs + arch.ptr_size(), arch.ptr_size(), root)
                .unwrap();
            h.process
                .state()
                .space
                .w32(attrs + 3 * arch.ptr_size(), flags)
                .unwrap();
            assert_eq!(
                h.open_with(output, io, 0x100001, 7, 0x60),
                expected,
                "{arch}/{flags:#x}/open"
            );
            assert_eq!(
                h.invoke_api("NtQueryFullAttributesFile", &[attrs, output]),
                expected,
                "{arch}/{flags:#x}/attributes"
            );
        }
    }
}
