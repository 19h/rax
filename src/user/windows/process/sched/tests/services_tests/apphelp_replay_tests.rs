use super::*;
use std::collections::HashMap;

macro_rules! capture {
    ($stem:literal, $profile:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-apphelp-cache/native-apphelp-",
            $stem,
            "-",
            $profile,
            ".log"
        ))
    };
}

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect()
}

fn value(row: &HashMap<&str, &str>, key: &str) -> u64 {
    row[key].parse().unwrap()
}

fn bytes(text: &str) -> Vec<Option<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|at| {
            if &text[at..at + 2] == "--" {
                None
            } else {
                Some(u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            }
        })
        .collect()
}

fn raw_pointer(data: &[Option<u8>], at: usize, width: usize) -> Option<u64> {
    let mut result = [0; 8];
    for (to, from) in result.iter_mut().zip(&data[at..at + width]) {
        *to = (*from)?;
    }
    Some(u64::from_le_bytes(result))
}

fn protect(p: &mut Proc, base: u64, page: u64, flags: u32) {
    p.vm.protect(base + page * PAGE_SIZE, PAGE_SIZE, flags)
        .unwrap();
}

fn normalize(
    data: &mut [Option<u8>],
    at: usize,
    width: usize,
    old: u64,
    new: u64,
    partial_store: bool,
) {
    if let Some(mut pointer) = raw_pointer(data, at, width) {
        if partial_store && width == 8 {
            pointer = (new & 0xffff_ffff_0000_0000) | (pointer & 0xffff_ffff);
        } else if pointer >= old && pointer - old < 65_536 {
            pointer = new + pointer - old;
        }
        for (slot, byte) in data[at..at + width].iter_mut().zip(pointer.to_le_bytes()) {
            *slot = Some(byte);
        }
    }
}

fn buffer_capture(arch: WinArch, capture: &str, ordering: bool) -> usize {
    let mut count = 0;
    for line in capture.lines().filter(|line| line.starts_with("case=")) {
        let row = fields(line);
        let mode = value(&row, "case");
        let (mut process, mut thread) = fixture(arch, "NtApphelpCacheControl", SERVICE);
        let p = process.state_mut();
        p.appcompat = catalog();
        let arena = scratch(p);
        p.space.wr(arena, &vec![0xa5; 32768]).unwrap();
        let input = arena + value(&row, "input");
        let width = arch.ptr_size() as usize;
        let mask = arch.ptr(u64::MAX);
        let name_at = if width == 8 { 248 } else { 132 };
        let name_ptr = name_at + if width == 8 { 8 } else { 4 };
        let out_at = if width == 8 { 376 } else { 196 };
        let mut expected = bytes(row["input-readable"]);
        let native_base = [(name_ptr, "name"), (out_at, "output")]
            .into_iter()
            .find_map(|(at, key)| {
                raw_pointer(&expected, at, width)
                    .map(|ptr| ptr.wrapping_sub(value(&row, key)) & mask)
                    .filter(|base| *base != 0 && *base % 65536 == 0)
            })
            .unwrap_or(0);
        let translate = |relative: u64| {
            if relative < 65536 {
                arena + relative
            } else {
                native_base.wrapping_add(relative) & mask
            }
        };
        let name = translate(value(&row, "name"));
        let output = translate(value(&row, "output"));
        p.space
            .wr(
                arena + 16384,
                &"apphelp.dll\0"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        p.space
            .wr(input, &[if ordering && mode == 20 { 0xa5 } else { 0 }; 512])
            .unwrap();
        packet(
            p,
            input,
            name,
            output,
            value(&row, "length") as u16,
            value(&row, "maximum") as u16,
            value(&row, "size") as u32,
        );
        if !ordering {
            if matches!(mode, 20 | 23 | 24) {
                p.space
                    .wr(
                        name,
                        &"apphelp.dll\0"
                            .encode_utf16()
                            .flat_map(u16::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            }
            if (50..69).contains(&mode) {
                protect(p, arena, 1, prot::NOACCESS);
            }
            if matches!(mode, 23 | 24) {
                protect(p, arena, 5, prot::NOACCESS);
            }
            if matches!(mode, 25 | 26 | 44) {
                protect(p, arena, 7, prot::NOACCESS);
            }
            if (27..=30).contains(&mode) {
                protect(
                    p,
                    arena,
                    0,
                    match mode {
                        27 => prot::READONLY,
                        28 => prot::EXECUTE_READ,
                        29 => prot::READWRITE | prot::GUARD,
                        _ => prot::READONLY | prot::GUARD,
                    },
                );
            }
            if matches!(mode, 31 | 32) {
                protect(
                    p,
                    arena,
                    4,
                    if mode == 31 {
                        prot::READONLY
                    } else {
                        prot::READWRITE | prot::GUARD
                    },
                );
            }
            if matches!(mode, 33 | 34) {
                protect(
                    p,
                    arena,
                    6,
                    if mode == 33 {
                        prot::READONLY
                    } else {
                        prot::READWRITE | prot::GUARD
                    },
                );
            }
            if (35..=39).contains(&mode) {
                protect(p, arena, 6, prot::NOACCESS);
            }
        } else {
            if mode == 12 {
                p.space
                    .wr(
                        arena + 16384,
                        &"APPHELP.DLL\0"
                            .encode_utf16()
                            .flat_map(u16::to_le_bytes)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            }
            if mode == 13 {
                p.space.w16(arena + 16388, 0).unwrap();
            }
            if mode == 14 {
                p.space.w16(arena + 16384, 0).unwrap();
            }
            if mode == 15 {
                p.space.w16(arena + 16384, 0xd800).unwrap();
            }
            if mode == 16 {
                p.space.wr(arena + 16384, &[0; 24]).unwrap();
            }
            if matches!(mode, 3 | 4 | 32) {
                protect(p, arena, 4, prot::READWRITE | prot::GUARD);
            }
            if matches!(mode, 4 | 17) {
                protect(p, arena, 6, prot::READWRITE | prot::GUARD);
            }
            if (21..=27).contains(&mode) || (32..=35).contains(&mode) {
                protect(p, arena, 0, prot::READONLY);
            }
            if (28..=31).contains(&mode) {
                protect(p, arena, 1, prot::READONLY);
            }
        }
        let status = call(p, &mut thread, 6, input);
        assert_eq!(
            status,
            u32::from_str_radix(row["status"], 16).unwrap(),
            "{arch:?} ordering={ordering} mode={mode}"
        );
        assert_eq!(row["escaped"], "00000000");
        normalize(
            &mut expected,
            name_ptr,
            width,
            native_base,
            arena,
            !ordering && mode == 16,
        );
        normalize(
            &mut expected,
            out_at,
            width,
            native_base,
            arena,
            !ordering && mode == 17,
        );
        for (at, byte) in expected.iter().enumerate() {
            if let Some(expected) = byte {
                assert_eq!(
                    p.space.u8(input + at as u64).unwrap(),
                    *expected,
                    "{arch:?} ordering={ordering} mode={mode} input+{at}"
                );
            }
        }
        let output_expected = if output >= input && output + 8 <= input + 512 {
            expected[(output - input) as usize..(output - input) as usize + 8].to_vec()
        } else {
            bytes(row["output-bytes"])
        };
        for (at, byte) in output_expected.iter().enumerate() {
            if let Some(expected) = byte {
                assert_eq!(
                    p.space.u8(output + at as u64).unwrap(),
                    *expected,
                    "{arch:?} ordering={ordering} mode={mode} output+{at}"
                );
            }
        }
        let protections: Vec<_> = row["protections"]
            .trim_end_matches(',')
            .split(',')
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        for (page, expected) in protections.into_iter().enumerate() {
            assert_eq!(
                p.vm.query(arena + page as u64 * PAGE_SIZE).unwrap().protect,
                expected,
                "{arch:?} ordering={ordering} mode={mode} page={page}"
            );
        }
        count += 1;
    }
    count
}

#[test]
fn native_apphelp_cache_548_buffer_and_ordering_controls_match_bytes_and_guards() {
    let mut count = 0;
    for (arch, buffer, ordering) in [
        (
            WinArch::Arm64,
            capture!("buffers", "arm64"),
            capture!("ordering", "arm64"),
        ),
        (
            WinArch::X64,
            capture!("buffers", "x64"),
            capture!("ordering", "x64"),
        ),
        (
            WinArch::X86,
            capture!("buffers", "x86"),
            capture!("ordering", "x86"),
        ),
        (
            WinArch::X86,
            capture!("buffers", "x86-laa"),
            capture!("ordering", "x86-laa"),
        ),
    ] {
        count += buffer_capture(arch, buffer, false);
        count += buffer_capture(arch, ordering, true);
    }
    assert_eq!(count, 548);
}

#[test]
fn native_apphelp_cache_768_counted_name_controls_match_output_and_packet() {
    let mut count = 0;
    for (arch, ordinary, candidates) in [
        (
            WinArch::Arm64,
            capture!("names", "arm64"),
            capture!("cdb-names", "arm64"),
        ),
        (
            WinArch::X64,
            capture!("names", "x64"),
            capture!("cdb-names", "x64"),
        ),
        (
            WinArch::X86,
            capture!("names", "x86"),
            capture!("cdb-names", "x86"),
        ),
        (
            WinArch::X86,
            capture!("names", "x86-laa"),
            capture!("cdb-names", "x86-laa"),
        ),
    ] {
        let (mut process, mut thread) = fixture(arch, "NtApphelpCacheControl", SERVICE);
        let p = process.state_mut();
        p.appcompat = catalog();
        let base = scratch(p);
        for line in ordinary
            .lines()
            .chain(candidates.lines())
            .filter(|l| l.starts_with("case="))
        {
            let row = fields(line);
            let units: Vec<_> = (0..row["name-utf16"].len())
                .step_by(4)
                .map(|at| u16::from_str_radix(&row["name-utf16"][at..at + 4], 16).unwrap())
                .collect();
            let data: Vec<_> = units
                .into_iter()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect();
            p.space.wr(base, &[0; 512]).unwrap();
            p.space.wr(base + 4096, &data).unwrap();
            p.space.w32(base + 8192, 0xa5a5a5a5).unwrap();
            packet(
                p,
                base,
                base + 4096,
                base + 8192,
                value(&row, "length") as u16,
                value(&row, "maximum") as u16,
                4,
            );
            let mut before = [0; 512];
            p.space.rd(base, &mut before).unwrap();
            assert_eq!(
                call(p, &mut thread, 6, base),
                u32::from_str_radix(row["status"], 16).unwrap()
            );
            assert_eq!(
                p.space.u32(base + 8192).unwrap(),
                u32::from_str_radix(row["value"], 16).unwrap(),
                "{}",
                row["name-utf16"]
            );
            let mut after = [0; 512];
            p.space.rd(base, &mut after).unwrap();
            assert_eq!(after, before);
            assert_eq!(row["changes"], "");
            count += 1;
        }
    }
    assert_eq!(count, 768);
}
