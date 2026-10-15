//! Replay byte-exact native argument captures and failure outputs.
use super::*;
use std::collections::HashMap;

fn num(row: &HashMap<&str, &str>, name: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[name], radix).unwrap()
}
fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
fn replay(arch: WinArch, text: &str, expected_rows: usize) -> (usize, usize, Vec<String>) {
    let blocks: Vec<_> = text.split("case=").skip(1).collect();
    assert_eq!(blocks.len(), expected_rows);
    let (mut process, mut t) = fixture(arch, "NtCreateSection", SERVICE);
    let p = process.state_mut();
    let b = scratch(p);
    let out = b;
    let max = b + 64;
    let oa = b + 128;
    let initial_handles = p.objects.handle_count();
    let initial_objects = p.objects.iter().count();
    let mut differences = Vec::new();
    let mut executed = 0;
    let mut unsupported = 0;
    for block in blocks {
        let mut lines = block.lines();
        let header = lines.next().unwrap();
        let row: HashMap<_, _> = header
            .split_whitespace()
            .skip(1)
            .map(|s| s.split_once('=').unwrap())
            .collect();
        let expected = num(&row, "status", 16) as u32;
        let output = num(&row, "output", 10);
        let maximum_pointer = num(&row, "maximum-pointer", 10);
        let object = num(&row, "object", 10);
        p.space.wr(out, &[0xA5; 32]).unwrap();
        p.space.wr(max, &[0xA5; 16]).unwrap();
        p.space.wr(oa, &[0; 48]).unwrap();
        let requested = row["maximum"].parse::<i64>().unwrap() as u64;
        let maximum = match maximum_pointer {
            1 => 0,
            2 => 1,
            3 => max + 1,
            _ => max,
        };
        p.space
            .wr(
                max + u64::from(maximum_pointer == 3),
                &requested.to_le_bytes(),
            )
            .unwrap();
        let destination = match output {
            1 => 0,
            2 => 1,
            3 => out + 1,
            _ => out,
        };
        let object_address = match object {
            0 => 0,
            4 => 1,
            _ => oa,
        };
        p.space
            .w32(
                oa,
                if object == 2 {
                    0
                } else if arch.is64() {
                    48
                } else {
                    24
                },
            )
            .unwrap();
        p.space
            .w32(
                oa + if arch.is64() { 24 } else { 12 },
                if object == 3 {
                    u32::MAX
                } else {
                    num(&row, "object-flags", 16) as u32
                },
            )
            .unwrap();
        let args = [
            destination,
            num(&row, "access", 16),
            object_address,
            maximum,
            num(&row, "protection", 16),
            num(&row, "attributes", 16),
            arch.ptr(num(&row, "file", 16)),
        ];
        let allocation = num(&row, "attributes", 16);
        let native_success = (expected as i32) >= 0;
        let expected_frontier = (native_success
            && (allocation & 0x0068_0000 != 0
                || num(&row, "access", 16) & 0x0100_0000 != 0
                || num(&row, "object-flags", 16) & 0x10 != 0))
            || (expected == 0xC00000F2 && allocation == 0x8800_0000 && requested != 0);
        let outcome = invoke(p, &mut t, &args);
        assert_eq!(
            matches!(&outcome, Outcome::Fail(_)),
            expected_frontier,
            "admission: {arch} {header}"
        );
        if let Outcome::Fail(reason) = outcome {
            assert!(
                reason.contains("large-page allocation contract")
                    || reason.contains("based, 64K-page or no-change allocation contract")
                    || reason.contains("ACCESS_SYSTEM_SECURITY privilege")
                    || reason.contains("OBJ_PERMANENT lifetime"),
                "{arch} {header}: {reason}"
            );
            assert_eq!(
                p.space.bytes(out, 32).unwrap(),
                vec![0xA5; 32],
                "unsupported publication: {arch} {header}"
            );
            unsupported += 1;
        } else {
            assert_eq!(outcome, Outcome::Continue, "{arch} {header}");
            let actual_status = t.cpu.gpr(0) as u32;
            if actual_status != expected {
                differences.push(format!("{arch} {header}: status={actual_status:08X}"));
            }

            let mut actual = p.space.bytes(out, 32).unwrap();
            let output_line = lines.find(|l| l.starts_with("bytes name=output ")).unwrap();
            let mut native = bytes(output_line.split("hex=").nth(1).unwrap());
            if (actual_status as i32) >= 0 {
                let handle = get(p, destination);
                if (expected as i32) >= 0 {
                    let rights_line = block.lines().find(|l| l.starts_with("grant ")).unwrap();
                    let grant =
                        u32::from_str_radix(rights_line.split("rights=").nth(1).unwrap(), 16)
                            .unwrap();
                    if p.objects.access(handle) != Some(grant) {
                        differences.push(format!(
                            "{arch} {header}: grant={:?},expected={grant:X}",
                            p.objects.access(handle)
                        ));
                    }
                }
                assert!(
                    matches!(
                        p.objects.get(handle),
                        Some(Object::Mapping {
                            file: None,
                            shared: Some(_),
                            ..
                        })
                    ),
                    "{arch} {header}"
                );
                if block
                    .lines()
                    .any(|l| l.starts_with("query status=00000000 "))
                {
                    let record = bytes(
                        block
                            .lines()
                            .find(|l| l.starts_with("bytes name=section-basic "))
                            .unwrap()
                            .split("hex=")
                            .nth(1)
                            .unwrap(),
                    );
                    let offset = if arch.is64() { 8 } else { 4 };
                    let native_attributes =
                        u32::from_le_bytes(record[offset..offset + 4].try_into().unwrap());
                    let size_offset = if arch.is64() { 16 } else { 8 };
                    let native_size = u64::from_le_bytes(
                        record[size_offset..size_offset + 8].try_into().unwrap(),
                    );
                    let Some(Object::Mapping {
                        attributes, size, ..
                    }) = p.objects.get(handle)
                    else {
                        panic!("section metadata")
                    };
                    if (*attributes, *size) != (native_attributes, native_size) {
                        differences.push(format!("{arch} {header}: metadata=({attributes:X},{size}),expected=({native_attributes:X},{native_size})"));
                    }
                }
                let offset = usize::from(output == 3);
                actual[offset..offset + arch.ptr_size() as usize].fill(0);
                if (expected as i32) >= 0 {
                    native[offset..offset + arch.ptr_size() as usize].fill(0);
                }
                p.objects.close(handle).unwrap();
            }
            if actual != native {
                differences.push(format!(
                    "{arch} {header}: output={actual:02X?},expected={native:02X?}"
                ));
            }
            executed += 1;
        }
        let expected_max = bytes(
            block
                .lines()
                .find(|l| l.starts_with("bytes name=maximum "))
                .unwrap()
                .split("hex=")
                .nth(1)
                .unwrap(),
        );
        assert_eq!(
            p.space.bytes(max, 16).unwrap(),
            expected_max,
            "maximum bytes: {arch} {header}"
        );
        assert_eq!(p.objects.handle_count(), initial_handles, "{arch} {header}");
        assert_eq!(p.objects.iter().count(), initial_objects, "{arch} {header}");
    }
    (executed, unsupported, differences)
}
#[test]
fn native_section_create_replays_all_5544_priority_and_copyback_controls() {
    let sources = [
        (
            WinArch::Arm64,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-priority-arm64.log"
            )),
        ),
        (
            WinArch::X64,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-priority-x64.log"
            )),
        ),
        (
            WinArch::X86,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-priority-x86.log"
            )),
        ),
        (
            WinArch::X86,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-priority-x86-laa.log"
            )),
        ),
    ];
    let mut total = 0;
    let mut all_differences = Vec::new();
    for (arch, text) in sources {
        let (executed, unsupported, differences) = replay(arch, text, 1386);
        all_differences.extend(differences);
        assert_eq!(executed + unsupported, 1386);
        println!(
            "section-create {arch}: native status/bytes={executed}, explicit unsupported={unsupported}"
        );
        total += executed + unsupported;
    }
    assert_eq!(total, 5544);
    assert!(
        all_differences.is_empty(),
        "{} native mismatches:\n{}",
        all_differences.len(),
        all_differences
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn native_section_create_replays_all_5376_protection_allocation_cross_controls() {
    let sources = [
        (
            WinArch::Arm64,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-valid-cross-arm64.log"
            )),
        ),
        (
            WinArch::X64,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-valid-cross-x64.log"
            )),
        ),
        (
            WinArch::X86,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-valid-cross-x86.log"
            )),
        ),
        (
            WinArch::X86,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/specifications/windows/native-section-create/native-section-create-valid-cross-x86-laa.log"
            )),
        ),
    ];
    let mut total = 0;
    let mut all_differences = Vec::new();
    for (arch, text) in sources {
        let (executed, unsupported, differences) = replay(arch, text, 1344);
        all_differences.extend(differences);
        assert_eq!(executed + unsupported, 1344);
        println!(
            "section-create {arch}: native status/bytes={executed}, explicit unsupported={unsupported}"
        );
        total += executed + unsupported;
    }
    assert_eq!(total, 5376);
    assert!(
        all_differences.is_empty(),
        "{} native mismatches:\n{}",
        all_differences.len(),
        all_differences
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
