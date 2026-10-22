//! Replay the independent native producer, retaining its pointer-fault order,
//! untouched bytes and protection transitions. Only host identity/topology
//! fields are projected onto the existing virtual process.

use super::*;
use std::collections::BTreeMap;

type Row<'a> = BTreeMap<&'a str, &'a str>;

macro_rules! capture {
    ($profile:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/specifications/windows/native-process-basic-information/native/",
            "native-process-basic-information-20261022-v6-four-views-",
            $profile,
            ".log"
        ))
    };
}

fn row(line: &str) -> Row<'_> {
    line.split_whitespace()
        .skip(1)
        .filter_map(|s| s.split_once('='))
        .collect()
}

fn decimal(r: &Row<'_>, key: &str) -> u64 {
    r[key].parse().unwrap()
}
fn hex(r: &Row<'_>, key: &str) -> u64 {
    u64::from_str_radix(r[key], 16).unwrap()
}
fn bytes(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|n| u8::from_str_radix(&value[n..n + 2], 16).unwrap())
        .collect()
}

fn arena(p: &mut Proc) -> u64 {
    let base =
        p.vm.allocate(
            None,
            3 * PAGE_SIZE,
            mem::RESERVE | mem::COMMIT,
            prot::READWRITE,
        )
        .unwrap()
        .0;
    p.vm.protect(base, PAGE_SIZE, prot::NOACCESS).unwrap();
    p.vm.protect(base + 2 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
        .unwrap();
    base + PAGE_SIZE
}

// A successful native 64-bit copy publishes ReturnLength last, possibly
// overwriting part of a field. Do not normalize those four overwritten bytes.
fn project(
    expected: &mut [u8],
    origin: u64,
    out: u64,
    returned: u64,
    length: u32,
    status: u64,
    meta: &Row<'_>,
    p: &Proc,
) {
    let width = p.arch.ptr_size() as usize;
    let basic = if width == 4 { 24 } else { 48 };
    let extended = if width == 4 { 32 } else { 64 };
    if length != basic && length != extended {
        return;
    }
    let record = out + if length == extended { width as u64 } else { 0 };
    let late = width == 8 && status == 0 && returned != 0;
    for (word, native, virtual_value) in [
        (1, hex(meta, "peb"), p.peb),
        (2, hex(meta, "affinity"), 1),
        (4, hex(meta, "pid"), u64::from(p.pid)),
        (5, hex(meta, "parent"), 0),
    ] {
        let address = record + word * width as u64;
        let Some(offset) = address.checked_sub(origin).map(|v| v as usize) else {
            continue;
        };
        if offset
            .checked_add(width)
            .is_none_or(|end| end > expected.len())
        {
            continue;
        }
        let n = native.to_le_bytes();
        let v = virtual_value.to_le_bytes();
        let indices: Vec<usize> = (0..width)
            .filter(|i| {
                let a = address + *i as u64;
                !(late && a >= returned && a < returned + 4)
            })
            .collect();
        if !indices.is_empty() && indices.iter().all(|i| expected[offset + i] == n[*i]) {
            for i in indices {
                expected[offset + i] = v[i];
            }
        }
    }
}

fn call(
    p: &mut Proc,
    t: &mut Thread,
    sp: u64,
    handle: u64,
    out: u64,
    length: u32,
    returned: u64,
    r: &Row<'_>,
    context: &str,
) {
    let last_error = t.teb + crate::user::windows::layout::offsets(p.arch).teb_last_error;
    p.space.w32(last_error, 0x1234_5678).unwrap();
    t.cpu.set_sp(sp);
    arguments(p, t, handle, 0, out, length, returned);
    assert_eq!(dispatch(p, t), Outcome::Continue, "{context}");
    assert_eq!(
        hex(r, "exception"),
        0,
        "native escaped exception: {context}"
    );
    assert_eq!(t.cpu.gpr(0), hex(r, "status"), "status: {context}");
    assert_eq!(
        u64::from(p.space.u32(last_error).unwrap()),
        hex(r, "last-error"),
        "LastError: {context}"
    );
    assert!(t.frames.is_empty(), "frames: {context}");
}

fn regular(
    p: &mut Proc,
    t: &mut Thread,
    sp: u64,
    scratch: u64,
    r: &Row<'_>,
    meta: &Row<'_>,
    context: &str,
) {
    let arch = p.arch;
    p.space.wr(scratch, &[0xA5; 256]).unwrap();
    if decimal(r, "seeded") != 0 {
        let seed = hex(r, "input-size").to_le_bytes();
        p.space
            .wr(scratch, &seed[..arch.ptr_size() as usize])
            .unwrap();
    }
    let kind = r["kind"];
    let real = if kind.starts_with("self-real-") {
        let object = p.objects.create(Object::Process {
            pid: p.pid,
            exit_code: None,
        });
        Some(
            p.objects
                .open_access(object, false, hex(r, "grant") as u32)
                .unwrap(),
        )
    } else {
        None
    };
    let handle = match kind {
        "self-pseudo" => arch.ptr(u64::MAX),
        "null" => 0,
        "invalid-one" => 1,
        "thread-pseudo" => arch.ptr(u64::MAX - 1),
        _ => u64::from(real.unwrap() | kind.rsplit_once("tag").unwrap().1.parse::<u32>().unwrap()),
    };
    let returned = if decimal(r, "returned-pointer") == 0 {
        0
    } else {
        scratch + 192
    };
    let length = decimal(r, "length") as u32;
    call(p, t, sp, handle, scratch, length, returned, r, context);
    let mut expected = bytes(r["bytes"]);
    project(
        &mut expected,
        scratch,
        scratch,
        returned,
        length,
        hex(r, "status"),
        meta,
        p,
    );
    assert_eq!(
        p.space.bytes(scratch, expected.len()).unwrap(),
        expected,
        "bytes: {context}"
    );
    assert_eq!(
        u64::from(p.space.u32(scratch + 192).unwrap()),
        hex(r, "returned"),
        "ReturnLength: {context}"
    );
    if let Some(handle) = real {
        p.objects.close(u64::from(handle)).unwrap();
    }
}

fn fault(
    p: &mut Proc,
    t: &mut Thread,
    sp: u64,
    output_page: u64,
    returned_page: u64,
    event: u64,
    r: &Row<'_>,
    meta: &Row<'_>,
    context: &str,
) {
    let width = p.arch.ptr_size() as u64;
    let basic = if width == 4 { 24 } else { 48 };
    let extended = if width == 4 { 32 } else { 64 };
    let upper = if width == 4 {
        0xFFFF_FFF0
    } else {
        0x7FFF_FFFF_FFF0
    };
    for page in [output_page, returned_page] {
        p.vm.protect(page, PAGE_SIZE, prot::READWRITE).unwrap();
        p.space
            .wr(page, &vec![hex(r, "pattern") as u8; PAGE_SIZE as usize])
            .unwrap();
    }
    let out = match decimal(r, "out-kind") {
        0 => output_page + 16,
        1 => 0,
        2 => 1,
        3 => output_page + 17,
        4 => output_page + 20,
        5 => output_page + PAGE_SIZE - basic,
        6 => output_page + PAGE_SIZE - basic + 4,
        7 => output_page + PAGE_SIZE - extended + 4,
        8 => upper,
        _ => unreachable!(),
    };
    let returned = match decimal(r, "ret-kind") {
        0 => 0,
        1 => returned_page + 16,
        2 => returned_page + 17,
        3 => 1,
        4 => out,
        5 => out + 4,
        6 => out + 8,
        7 => out + basic - 4,
        8 => returned_page + PAGE_SIZE - 2,
        9 => upper,
        _ => unreachable!(),
    };
    let handle = match decimal(r, "handle-kind") {
        0 => p.arch.ptr(u64::MAX),
        1 => 0,
        2 => p.arch.ptr(u64::MAX - 1),
        3 => event,
        _ => unreachable!(),
    };
    p.vm.protect(output_page, PAGE_SIZE, hex(r, "out-protect") as u32)
        .unwrap();
    p.vm.protect(returned_page, PAGE_SIZE, hex(r, "ret-protect") as u32)
        .unwrap();
    let length = decimal(r, "length") as u32;
    call(p, t, sp, handle, out, length, returned, r, context);
    assert_eq!(
        u64::from(p.vm.query(output_page).unwrap().protect),
        hex(r, "out-after"),
        "output protection: {context}"
    );
    assert_eq!(
        u64::from(p.vm.query(returned_page).unwrap().protect),
        hex(r, "ret-after"),
        "ReturnLength protection: {context}"
    );
    for page in [output_page, returned_page] {
        p.vm.protect(page, PAGE_SIZE, prot::READWRITE).unwrap();
    }
    for (key, address) in [
        ("out-head", output_page),
        ("out-tail", output_page + PAGE_SIZE - 128),
        ("ret-head", returned_page),
        ("ret-tail", returned_page + PAGE_SIZE - 16),
    ] {
        let mut expected = bytes(r[key]);
        project(
            &mut expected,
            address,
            out,
            returned,
            length,
            hex(r, "status"),
            meta,
            p,
        );
        assert_eq!(
            p.space.bytes(address, expected.len()).unwrap(),
            expected,
            "{key}: {context}"
        );
    }
}

#[test]
fn native_process_basic_information_replays_19852_captured_calls() {
    let mut total = 0;
    for (arch, profile, log) in [
        (WinArch::Arm64, "arm64", capture!("arm64")),
        (WinArch::X64, "x64", capture!("x64")),
        (WinArch::X86, "x86", capture!("x86")),
        (WinArch::X86, "x86-laa", capture!("x86-laa")),
    ] {
        let meta = row(log.lines().next().unwrap());
        assert_eq!(decimal(&meta, "pointer-bytes"), arch.ptr_size() as u64);
        let (mut process, mut t, scratch) = setup(arch);
        let p = process.state_mut();
        let sp = t.cpu.sp();
        let output_page = arena(p);
        let returned_page = arena(p);
        let event = u64::from(p.objects.insert(Object::Event {
            manual: true,
            signaled: 0,
        }));
        let mut queries = 0;
        let mut faults = 0;
        for (index, line) in log.lines().enumerate() {
            let context = format!("{profile} line {}", index + 1);
            let r = row(line);
            if line.starts_with("QUERY ") {
                regular(p, &mut t, sp, scratch, &r, &meta, &context);
                queries += 1;
            } else if line.starts_with("FAULT ") {
                fault(
                    p,
                    &mut t,
                    sp,
                    output_page,
                    returned_page,
                    event,
                    &r,
                    &meta,
                    &context,
                );
                faults += 1;
            }
        }
        p.objects.close(event).unwrap();
        assert_eq!((queries, faults), (163, 4800), "{profile}");
        total += queries + faults;
    }
    assert_eq!(total, 19852);
}
