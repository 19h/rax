//! Replay the 176 original native controls against each argument transport.
use super::*;
use std::collections::HashMap;

const CAPTURES: [(WinArch, &str); 4] = [
    (
        WinArch::Arm64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-process-tls/native-process-tls-control-arm64.log"
        ),
    ),
    (
        WinArch::X64,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-process-tls/native-process-tls-control-x64.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-process-tls/native-process-tls-control-x86.log"
        ),
    ),
    (
        WinArch::X86,
        include_str!(
            "../../../../../../../docs/specifications/windows/native-process-tls/native-process-tls-control-x86-laa.log"
        ),
    ),
];

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|part| part.split_once('='))
        .collect()
}

fn number(row: &HashMap<&str, &str>, key: &str, radix: u32) -> u64 {
    u64::from_str_radix(row[key], radix).unwrap()
}

fn decode(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&value[at..at + 2], 16).unwrap())
        .collect()
}

fn translated(value: u64, row: &HashMap<&str, &str>, old: u64, new: u64, second: u64) -> u64 {
    if value == number(row, "before-vector", 16) {
        old
    } else if value == number(row, "new", 16) {
        new
    } else if value == number(row, "second", 16) {
        second
    } else {
        value
    }
}

fn normalize_array(
    data: &mut [u8],
    ptr: usize,
    row: &HashMap<&str, &str>,
    old: u64,
    new: u64,
    second: u64,
) {
    for bytes in data.chunks_exact_mut(ptr) {
        let value = if ptr == 4 {
            u64::from(u32::from_le_bytes(bytes.try_into().unwrap()))
        } else {
            u64::from_le_bytes(bytes.try_into().unwrap())
        };
        let mapped = translated(value, row, old, new, second).to_le_bytes();
        bytes.copy_from_slice(&mapped[..ptr]);
    }
}

fn normalize_packet(
    data: &mut [u8],
    ptr: usize,
    row: &HashMap<&str, &str>,
    old: u64,
    new: u64,
    second: u64,
    tid: u32,
) {
    for entry in 0..2 {
        let at = 16 + entry * ptr * 3;
        if at + ptr * 2 > data.len() {
            break;
        }
        normalize_array(
            &mut data[at + ptr..at + ptr * 2],
            ptr,
            row,
            old,
            new,
            second,
        );
        if at + ptr * 3 <= data.len() && number(row, "operation", 10) == 1 {
            let bytes = &mut data[at + ptr * 2..at + ptr * 3];
            let value = if ptr == 4 {
                u64::from(u32::from_le_bytes(bytes.try_into().unwrap()))
            } else {
                u64::from_le_bytes(bytes.try_into().unwrap())
            };
            if value == number(row, "thread", 10) {
                bytes.copy_from_slice(&u64::from(tid).to_le_bytes()[..ptr]);
            }
        }
    }
}

#[test]
fn native_process_tls_original_176_controls_match_status_packet_vector_and_guards() {
    let mut total = 0;
    let mut packet_checks = 0;
    for (arch, capture) in CAPTURES {
        let mut before_input = Vec::new();
        let mut before_vector = Vec::new();
        let mut row = HashMap::new();
        let mut result = HashMap::new();
        let mut after_input = Vec::new();
        let mut after_vector = Vec::new();
        for line in capture.lines() {
            let f = fields(line);
            if line.starts_with("bytes ") {
                let data = decode(f["hex"]);
                match f["name"] {
                    "input-before" => before_input = data,
                    "vector-before" => before_vector = data,
                    "input-after" => after_input = data,
                    "vector-after" => after_vector = data,
                    _ => unreachable!(),
                }
            } else if line.starts_with("case=") {
                row = f;
            } else if line.starts_with("result ") {
                result = f;
            } else if line.starts_with("child-exit ") {
                assert_eq!(f["code"], "77");
                let ordinal = number(&row, "case", 10);
                let (mut process, mut t) = fixture(arch, "NtSetInformationProcess", SERVICE);
                let p = process.state_mut();
                let input_base =
                    p.vm.allocate(None, PAGE_SIZE * 4, mem::RESERVE, prot::READWRITE)
                        .unwrap()
                        .0;
                p.vm.commit(input_base, PAGE_SIZE * 2, prot::READWRITE)
                    .unwrap();
                let arrays = scratch(p);
                let old = arrays;
                let new = arrays + PAGE_SIZE;
                let second = arrays + PAGE_SIZE * 2;
                let ptr = arch.ptr_size() as usize;
                normalize_array(&mut before_vector, ptr, &row, old, new, second);
                for address in [old, new, second] {
                    p.space.wr(address, &before_vector).unwrap();
                }
                publish(p, &mut t, old);
                let input = match ordinal {
                    28 => 0,
                    29 => 1,
                    30 => input_base + 129,
                    31 => input_base + 132,
                    34 => input_base + PAGE_SIZE * 2 - 20,
                    35 => input_base + PAGE_SIZE * 2,
                    _ => input_base + 128,
                };
                let storage = if matches!(ordinal, 28 | 29 | 35) {
                    input_base + 128
                } else {
                    input
                };
                let template = input_base + 256;
                packet(
                    p,
                    template,
                    number(&row, "operation", 10) as u32,
                    number(&row, "count", 10) as u32,
                    number(&row, "index", 10) as u32,
                    &[if matches!(ordinal, 14 | 15) { 0 } else { new }, second],
                );
                p.space
                    .w32(template, number(&row, "flags", 16) as u32)
                    .unwrap();
                for entry in 0..2 {
                    p.space
                        .w32(
                            template + 16 + entry * arch.ptr_size() * 3,
                            number(&row, "entry-flags", 16) as u32,
                        )
                        .unwrap();
                }
                let available = (input_base + PAGE_SIZE * 2 - storage).min(128) as usize;
                let initialized = p.space.bytes(template, available).unwrap();
                p.space.wr(storage, &initialized).unwrap();
                if !before_input.is_empty() {
                    normalize_packet(&mut before_input, ptr, &row, old, new, second, t.tid);
                    p.space.wr(storage, &before_input).unwrap();
                }
                if ordinal == 32 {
                    p.vm.protect(input_base, PAGE_SIZE, prot::READONLY).unwrap();
                }
                if ordinal == 33 {
                    p.vm.protect(input_base, PAGE_SIZE, prot::READWRITE | prot::GUARD)
                        .unwrap();
                }
                let status = call(
                    p,
                    &mut t,
                    number(&row, "process", 16),
                    input,
                    number(&row, "length", 10) as u32,
                );
                assert_eq!(
                    status,
                    number(&result, "status", 16) as u32,
                    "{arch:?} case {ordinal}"
                );
                assert_eq!(
                    p.vm.query(input_base).unwrap().protect,
                    number(&result, "input-protect", 16) as u32,
                    "{arch:?} case {ordinal}"
                );
                let expected =
                    translated(number(&result, "after-vector", 16), &row, old, new, second);
                assert_eq!(
                    p.space
                        .ptr(
                            t.teb + layout::offsets(arch).teb_tls_pointer,
                            arch.ptr_size()
                        )
                        .unwrap(),
                    expected,
                    "{arch:?} case {ordinal}"
                );
                if !after_input.is_empty() {
                    normalize_packet(&mut after_input, ptr, &row, old, new, second, t.tid);
                    assert_eq!(
                        p.space.bytes(input, after_input.len()).unwrap(),
                        after_input,
                        "packet {arch:?} case {ordinal}"
                    );
                    packet_checks += 1;
                }
                if !after_vector.is_empty() {
                    normalize_array(&mut after_vector, ptr, &row, old, new, second);
                    assert_eq!(
                        p.space.bytes(expected, after_vector.len()).unwrap(),
                        after_vector,
                        "vector {arch:?} case {ordinal}"
                    );
                }
                total += 1;
                before_input.clear();
                before_vector.clear();
                after_input.clear();
                after_vector.clear();
            }
        }
    }
    assert_eq!(total, 176);
    assert_eq!(packet_checks, 160);
}
