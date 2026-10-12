//! Native function30 capture, format, lifetime and failure replay.
use super::*;
macro_rules! profiles {
    ($family:literal) => {[
        (WinArch::Arm64, include_str!(concat!("../../../../../../../docs/specifications/windows/native-etw-provider-traits/native-etw-provider-traits-",$family,"-arm64.log"))),
        (WinArch::X64, include_str!(concat!("../../../../../../../docs/specifications/windows/native-etw-provider-traits/native-etw-provider-traits-",$family,"-x64.log"))),
        (WinArch::X86, include_str!(concat!("../../../../../../../docs/specifications/windows/native-etw-provider-traits/native-etw-provider-traits-",$family,"-x86.log"))),
        (WinArch::X86, include_str!(concat!("../../../../../../../docs/specifications/windows/native-etw-provider-traits/native-etw-provider-traits-",$family,"-x86-laa.log"))),
    ]};
}
#[path = "etw_provider_traits_boundary_tests.rs"]
mod boundary_tests;
#[cfg(windows)]
#[path = "etw_provider_traits_leaf_tests.rs"]
mod leaf_tests;
fn own(p: &mut Proc) -> u64 {
    p.objects
        .register_etw_provider(Box::new(header()))
        .unwrap()
        .into()
}
fn packet(h: u64, pointer: u64, length: u32, pad: u32) -> [u8; 24] {
    let mut b = [0; 24];
    b[..8].copy_from_slice(&h.to_le_bytes());
    b[8..16].copy_from_slice(&pointer.to_le_bytes());
    b[16..20].copy_from_slice(&length.to_le_bytes());
    b[20..].copy_from_slice(&pad.to_le_bytes());
    b
}
fn stored(p: &Proc, h: u64) -> Option<&[u8]> {
    match p.objects.get(h) {
        Some(Object::EtwRegistration { traits, .. }) => traits.as_deref(),
        other => panic!("registration {h:x}: {other:?}"),
    }
}
fn name_blob(length: usize, embedded: u16, name: u64) -> Vec<u8> {
    let mut b = vec![0; 65536];
    if name == 2 {
        b[2..].fill(0x41);
    }
    b[..2].copy_from_slice(&embedded.to_le_bytes());
    if name == 1 || name == 3 {
        b[2..7].copy_from_slice(b"name\0");
    }
    if name == 3 {
        b[7..9].copy_from_slice(&3u16.to_le_bytes());
        b[9] = 99;
    }
    b.truncate(length);
    b
}
#[test]
fn native_etw_provider_traits_replays_all_4128_lengths_fields_handles_and_repeats() {
    let mut total = 0;
    for (arch, text) in profiles!("matrix") {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 1032);
        assert!(text.contains("complete cases=1032"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 20);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed {
            p.space.wr(b, &[0; 256]).unwrap();
            p.space.wr(b + 512, &[0xA5; 256]).unwrap();
            p.space.w32(b + 256, 0xA5A5A5A5).unwrap();
            p.space.w32(b + 260, 0xB6B6B6B6).unwrap();
            let h = own(p);
            let hm = field(&row, "handle-mode", 10);
            let mut selected = h;
            let mut other = None;
            let high = field(&row, "high", 16);
            match hm {
                1 => selected = 0,
                2 => selected = 1,
                3 => {
                    p.objects.close(h).unwrap();
                }
                4 => {
                    let e = p.objects.insert(Object::Event {
                        manual: false,
                        signaled: 0,
                    });
                    selected = e.into();
                    other = Some(selected);
                }
                5 => selected |= high,
                6 => selected = 0xFFFFFFFC,
                _ => {}
            }
            let raw_length = field(&row, "length", 10) as u32;
            let length = raw_length as u16 as usize;
            let blob = name_blob(
                length,
                field(&row, "embedded", 10) as u16,
                field(&row, "name", 10),
            );
            p.space.wr(b + 4096, &blob).unwrap();
            let request = packet(selected, b + 4096, raw_length, high as u32);
            p.space.wr(b, &request).unwrap();
            let before = p.space.bytes(b, 32).unwrap();
            let mode = field(&row, "mode", 10);
            let args = [
                30,
                if mode == 1 { 0 } else { b },
                field(&row, "input-length", 10),
                if mode == 2 {
                    0
                } else if mode == 3 {
                    b
                } else {
                    b + 512
                },
                field(&row, "output-length", 10),
                if mode == 4 { 0 } else { b + 256 },
            ];
            check(p, &mut t, args, field(&row, "status", 16));
            assert_eq!(
                p.space.u32(b + 256).unwrap() as u64,
                field(&row, "returned", 16)
            );
            if hm != 3 {
                assert_eq!(
                    stored(p, h),
                    if row["status"] == "00000000" {
                        Some(blob.as_slice())
                    } else {
                        None
                    }
                );
            }
            check(p, &mut t, args, field(&row, "repeat", 16));
            assert_eq!(
                p.space.u32(b + 256).unwrap() as u64,
                field(&row, "repeat-returned", 16)
            );
            assert_eq!(p.space.bytes(b, 32).unwrap(), before);
            assert_eq!(p.space.bytes(b + 512, 160).unwrap(), [0xA5; 160]);
            assert_eq!(p.space.u32(b + 260).unwrap(), 0xB6B6B6B6);
            if hm != 3 {
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
    assert_eq!(total, 4128);
}
#[test]
fn native_etw_provider_traits_replays_all_840_high_length_bits_as_measured_16_bit_captures() {
    let mut total = 0;
    for (arch, text) in profiles!("length-high") {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 210);
        assert!(text.contains("complete cases=210"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 20);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed {
            p.space.wr(b, &[0; 256]).unwrap();
            p.space.wr(b + 512, &[0xA5; 256]).unwrap();
            p.space.w32(b + 256, 0xA5A5A5A5).unwrap();
            p.space.w32(b + 260, 0xB6B6B6B6).unwrap();
            let h = own(p);
            let hm = field(&row, "handle-mode", 10);
            let mut selected = h;
            let mut other = None;
            let high = field(&row, "high", 16);
            match hm {
                1 => selected = 0,
                2 => selected = 1,
                3 => {
                    p.objects.close(h).unwrap();
                }
                4 => {
                    let e = p.objects.insert(Object::Event {
                        manual: false,
                        signaled: 0,
                    });
                    selected = e.into();
                    other = Some(selected);
                }
                5 => selected |= high,
                6 => selected = 0xFFFFFFFC,
                _ => {}
            }
            let raw_length = field(&row, "length", 10) as u32;
            let length = raw_length as u16 as usize;
            let blob = name_blob(
                length,
                field(&row, "embedded", 10) as u16,
                field(&row, "name", 10),
            );
            p.space.wr(b + 4096, &blob).unwrap();
            let request = packet(selected, b + 4096, raw_length, high as u32);
            p.space.wr(b, &request).unwrap();
            let before = p.space.bytes(b, 32).unwrap();
            let mode = field(&row, "mode", 10);
            let args = [
                30,
                if mode == 1 { 0 } else { b },
                field(&row, "input-length", 10),
                if mode == 2 {
                    0
                } else if mode == 3 {
                    b
                } else {
                    b + 512
                },
                field(&row, "output-length", 10),
                if mode == 4 { 0 } else { b + 256 },
            ];
            check(p, &mut t, args, field(&row, "status", 16));
            assert_eq!(
                p.space.u32(b + 256).unwrap() as u64,
                field(&row, "returned", 16)
            );
            if hm != 3 {
                assert_eq!(
                    stored(p, h),
                    if row["status"] == "00000000" {
                        Some(blob.as_slice())
                    } else {
                        None
                    }
                );
            }
            check(p, &mut t, args, field(&row, "repeat", 16));
            assert_eq!(
                p.space.u32(b + 256).unwrap() as u64,
                field(&row, "repeat-returned", 16)
            );
            assert_eq!(p.space.bytes(b, 32).unwrap(), before);
            assert_eq!(p.space.bytes(b + 512, 160).unwrap(), [0xA5; 160]);
            assert_eq!(p.space.u32(b + 260).unwrap(), 0xB6B6B6B6);
            if hm != 3 {
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
    assert_eq!(total, 840);
}
fn format_blob(row: &HashMap<&str, &str>) -> Vec<u8> {
    let len = field(row, "length", 10) as usize;
    let family = field(row, "family", 10);
    let name = field(row, "name", 10);
    let rec = field(row, "record", 10) as usize;
    let kind = field(row, "type", 10);
    let tail = field(row, "tail", 10);
    let delta = row["delta"].parse::<i32>().unwrap();
    let mut b = vec![0; 65536];
    b[..2].copy_from_slice(&(len as u16).to_le_bytes());
    if family == 0 {
        if name == 1 || name == 2 {
            b[2..len].fill(if name == 1 { 0x41 } else { 0xFF });
            b[len - 1] = 0;
        } else if name == 3 {
            b[2..len].fill(0x41);
        }
    } else if family == 1 {
        b[3..5].copy_from_slice(&(rec as u16).to_le_bytes());
        b[5] = kind as u8;
        if rec >= 19 && kind == 1 && tail != 0 {
            for i in 0..16 {
                b[6 + i] = (i + 1) as u8;
            }
        }
        if tail == 2 && delta >= 3 {
            b[3 + rec..5 + rec].copy_from_slice(&3u16.to_le_bytes());
            b[5 + rec] = 99;
        }
    } else {
        b[2..7].copy_from_slice(b"name\0");
        b[7..9].copy_from_slice(&(rec as u16).to_le_bytes());
        b[9] = kind as u8;
        if rec >= 19 && kind == 1 {
            for i in 0..16 {
                b[10 + i] = (i + 1) as u8;
            }
        }
    }
    b.truncate(len);
    b
}
#[test]
fn native_etw_provider_traits_replays_all_2844_names_records_and_65535_byte_captures() {
    let mut total = 0;
    for (arch, text) in profiles!("format") {
        let observed = rows(text, "case=");
        assert_eq!(observed.len(), 711);
        assert!(text.contains("complete cases=711"));
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 20);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in observed {
            let blob = format_blob(&row);
            let hash = blob.iter().fold(14695981039346656037u64, |h, x| {
                (h ^ u64::from(*x)).wrapping_mul(1099511628211)
            });
            assert_eq!(hash, field(&row, "payload-hash", 16));
            assert_eq!(&blob[..blob.len().min(64)], hex(row["prefix"]));
            assert_eq!(&blob[blob.len().saturating_sub(64)..], hex(row["suffix"]));
            let h = own(p);
            p.space
                .wr(b, &packet(h, b + 4096, blob.len() as u32, 0))
                .unwrap();
            p.space.wr(b + 4096, &blob).unwrap();
            p.space.wr(b + 128, &[0xA5; 120]).unwrap();
            p.space.w32(b + 256, 0xA5A5A5A5).unwrap();
            check(
                p,
                &mut t,
                [30, b, 24, b + 128, 120, b + 256],
                field(&row, "status", 16),
            );
            assert_eq!(
                p.space.u32(b + 256).unwrap() as u64,
                field(&row, "returned", 16)
            );
            assert_eq!(
                stored(p, h),
                if row["status"] == "00000000" {
                    Some(blob.as_slice())
                } else {
                    None
                }
            );
            p.space.wr(b + 4096, &[3, 0, 0]).unwrap();
            p.space.wr(b, &packet(h, b + 4096, 3, 0)).unwrap();
            p.space.w32(b + 256, 0xA5A5A5A5).unwrap();
            check(
                p,
                &mut t,
                [30, b, 24, b + 128, 120, b + 256],
                field(&row, "retry", 16),
            );
            assert_eq!(
                p.space.u32(b + 256).unwrap() as u64,
                field(&row, "retry-returned", 16)
            );
            assert_eq!(
                stored(p, h),
                Some(if row["status"] == "00000000" {
                    blob.as_slice()
                } else {
                    &[3, 0, 0]
                })
            );
            assert_eq!(p.space.bytes(b + 128, 120).unwrap(), [0xA5; 120]);
            p.objects.close(h).unwrap();
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            total += 1;
        }
    }
    assert_eq!(total, 2844);
}
#[test]
fn native_etw_provider_traits_captured_state_survives_guest_writes_and_handle_reuse() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 1);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        let h = own(p);
        let blob = b"\x07\0name\0";
        p.space.wr(b, &packet(h, b + 512, 7, 0)).unwrap();
        p.space.wr(b + 512, blob).unwrap();
        check(p, &mut t, [30, b, 24, b + 128, 120, b + 256], 0);
        p.space.wr(b + 512, &[0xCC; 7]).unwrap();
        assert_eq!(stored(p, h), Some(blob.as_slice()));
        p.objects.close(h).unwrap();
        let reused = own(p);
        assert_eq!(reused, h);
        assert_eq!(stored(p, reused), None);
        p.space.wr(b, &packet(reused, b + 512, 3, 0)).unwrap();
        p.space.wr(b + 512, &[3, 0, 0]).unwrap();
        check(p, &mut t, [30, b, 24, b + 128, 120, b + 256], 0);
        assert_eq!(stored(p, reused), Some(&[3, 0, 0][..]));
        p.objects.close(reused).unwrap();
        assert_eq!(
            (p.objects.handle_count(), p.objects.iter().count()),
            baseline
        );
        p.objects.drain().for_each(drop);
        assert_eq!((p.objects.handle_count(), p.objects.iter().count()), (0, 0));
    }
}
#[test]
fn native_etw_provider_traits_other_functions_stay_explicit_and_nonmutating() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        let (mut process, mut t) = fixture(arch, "NtTraceControl", SERVICE);
        let p = process.state_mut();
        let b = scratch(p, 1);
        let h = own(p);
        for function in [0, 14, 16, 26, 28, 29, 31, u32::MAX] {
            assert!(
                matches!(call(p,&mut t,[function.into(),1,u32::MAX.into(),1,u32::MAX.into(),b]),Outcome::Fail(m) if m.contains("NtTraceControl function"))
            );
            assert_eq!(stored(p, h), None);
        }
        p.objects.close(h).unwrap();
    }
}
