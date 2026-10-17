//! Replay the native records without normalizing statuses or lifetime effects.

use super::*;
use serde_json::Value;

const CORPUS: &str = include_str!(
    "../../../../../../../docs/specifications/windows/native-dynamic-function-table/native/corpus.json"
);

fn executions(kind: &str, profile: &str) -> Vec<Value> {
    let value: Value = serde_json::from_str(CORPUS).unwrap();
    let rows = value["executions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == kind && row["profile"] == profile && row["contract"] == true);
    let result: Vec<_> = rows.cloned().collect();
    assert!(!result.is_empty(), "missing {kind} {profile} native inputs");
    result
}

fn field<'a>(row: &'a Value, name: &str) -> &'a str {
    row[name]
        .as_str()
        .unwrap_or_else(|| panic!("missing {name}: {row}"))
}

fn decimal(row: &Value, name: &str) -> u64 {
    field(row, name).parse().unwrap()
}

fn hexadecimal(row: &Value, name: &str) -> u64 {
    u64::from_str_radix(field(row, name), 16).unwrap()
}

fn expected(actual: u32, row: &Value, name: &str) {
    assert_eq!(actual, hexadecimal(row, name) as u32, "{name}: {row}");
}

fn profiles() -> [(&'static str, WinArch); 4] {
    [
        ("arm64", WinArch::Arm64),
        ("x64", WinArch::X64),
        ("x86", WinArch::X86),
        ("x86-laa", WinArch::X86),
    ]
}

fn rows(execution: &Value) -> impl Iterator<Item = &Value> {
    execution["records"].as_array().unwrap().iter()
}

#[test]
fn native_dynamic_function_table_replays_pointer_guard_and_length_records() {
    let mut total = 0;
    for (profile, arch) in profiles() {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let safe = scratch(p);
        let fault = scratch(p);
        let safe_table = safe + 0x400;
        record(p, safe_table, 0, 0);
        let input = safe + 0x100;
        for execution in executions("boundary", profile) {
            for row in rows(&execution).filter(|r| r["record"] == "case" || r["record"] == "length")
            {
                p.vm.protect(fault, 8192, prot::READWRITE).unwrap();
                let table;
                if row["record"] == "case" {
                    let axis = decimal(row, "axis");
                    let offset = decimal(row, "offset");
                    p.vm.poke(fault, &vec![if axis == 0 { 0xA5 } else { 0 }; 8192])
                        .unwrap();
                    let caller = if axis == 0 { fault + offset } else { input };
                    table = if axis == 0 {
                        safe_table
                    } else {
                        fault + offset
                    };
                    packet(p, caller, table, 0);
                    let before = p.space.bytes(fault, 8192).unwrap();
                    p.vm.protect(fault, 4096, hexadecimal(row, "p0") as u32)
                        .unwrap();
                    p.vm.protect(fault + 4096, 4096, hexadecimal(row, "p1") as u32)
                        .unwrap();
                    expected(
                        call(
                            p,
                            &mut thread,
                            arch.ptr(u64::MAX),
                            caller,
                            arch.ptr_size() as u32 * 2,
                        ),
                        row,
                        "status",
                    );
                    assert_eq!(
                        p.vm.query(fault).unwrap().protect,
                        hexadecimal(row, "after0") as u32,
                        "{row}"
                    );
                    assert_eq!(
                        p.vm.query(fault + 4096).unwrap().protect,
                        hexadecimal(row, "after1") as u32,
                        "{row}"
                    );
                    p.vm.protect(fault, 8192, prot::READWRITE).unwrap();
                    assert_eq!(
                        p.space.bytes(fault, 8192).unwrap(),
                        before,
                        "caller write: {row}"
                    );
                } else {
                    table = safe_table;
                    let invalid = decimal(row, "invalid");
                    packet(p, input, table, 0);
                    let target = if invalid >= 2 { 0 } else { arch.ptr(u64::MAX) };
                    let pointer = if invalid & 1 != 0 { 0 } else { input };
                    expected(
                        call(
                            p,
                            &mut thread,
                            target,
                            pointer,
                            decimal(row, "length") as u32,
                        ),
                        row,
                        "status",
                    );
                }
                expected(operation(p, &mut thread, input, table, 1), row, "cleanup");
                assert_eq!(p.seh.kernel_dynamic_tables.len(), 0, "{row}");
                total += 1;
            }
        }
    }
    assert_eq!(total, 3864);
}

#[test]
fn native_dynamic_function_table_replays_initial_and_handle_boundaries() {
    let mut total = 0;
    for (profile, arch) in profiles() {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, table) = (memory, memory + 0x100);
        for kind in ["initial", "handle-boundary"] {
            for execution in executions(kind, profile) {
                p.seh.kernel_dynamic_tables = Default::default();
                let row = rows(&execution).find(|r| r.get("mode").is_some()).unwrap();
                let mode = decimal(row, "mode");
                record(p, table, 0, 0);
                let inner = if mode < 2 { 0 } else { table };
                let remove = match mode {
                    1 | 3 => 1,
                    4 => 255,
                    _ => 0,
                };
                packet(p, input, inner, remove);
                let target = if mode >= 9 {
                    arch.ptr(
                        [
                            0xFFFF_FFFF,
                            u64::MAX,
                            u64::MAX - 1,
                            0xFFFF_FFFF_DEAD_BEEF,
                            1,
                            0,
                        ][(mode - 9) as usize],
                    )
                } else if mode >= 7 {
                    0xDEAD_BEEC
                } else {
                    arch.ptr(u64::MAX)
                };
                let pointer = if mode == 5 || mode == 7 { 0 } else { input };
                let length = if mode == 6 {
                    0
                } else {
                    arch.ptr_size() as u32 * 2
                };
                let before = p.space.bytes(input, 512).unwrap();
                expected(call(p, &mut thread, target, pointer, length), row, "status");
                assert_eq!(p.space.bytes(input, 512).unwrap(), before);
                total += 1;
            }
        }
    }
    assert_eq!(total, 60);
}

#[test]
fn native_dynamic_function_table_replays_duplicate_and_value_key_lifetime() {
    let mut total = 0;
    for (profile, arch) in profiles() {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, a, b) = (memory, memory + 0x100, memory + 0x300);
        record(p, a, 0, 0);
        record(p, b, 0, 0);
        for execution in executions("lifecycle", profile) {
            for row in rows(&execution).filter(|r| r["record"] == "case") {
                let label = field(row, "label");
                if label == "remove-A-mutated" {
                    p.vm.poke(a, &[0xA5; 256]).unwrap();
                } else if label == "null" {
                    record(p, a, 0, 0);
                }
                let table = if label.contains("-B") { b } else { a };
                let remove = if label.contains("remove") || label == "cleanup-self" {
                    if label.ends_with("255") {
                        255
                    } else if label.ends_with("-2") {
                        2
                    } else {
                        1
                    }
                } else {
                    0
                };
                let target = match label {
                    "null" | "self-zero-rights" => 0,
                    "current-thread" => arch.ptr(u64::MAX - 1),
                    "self-set-information" | "self-query-limited" => 4,
                    _ => arch.ptr(u64::MAX),
                };
                packet(p, input, table, remove);
                expected(
                    call(p, &mut thread, target, input, arch.ptr_size() as u32 * 2),
                    row,
                    "status",
                );
                total += 1;
            }
        }
        assert_eq!(p.seh.kernel_dynamic_tables.len(), 0);
    }
    assert_eq!(total, 84);
}

#[test]
fn native_dynamic_function_table_replays_all_key_byte_perturbations() {
    let mut total = 0;
    for (profile, arch) in profiles() {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, a, b) = (memory, memory + 0x100, memory + 0x300);
        for execution in executions("key", profile) {
            for row in rows(&execution).filter(|r| r["record"] == "key") {
                record(p, a, 0, 0);
                record(p, b, 0, 0);
                let byte: i32 = field(row, "byte").parse().unwrap();
                if byte >= 0 {
                    p.space.w8(b + byte as u64, 1).unwrap();
                }
                expected(operation(p, &mut thread, input, a, 0), row, "add");
                expected(operation(p, &mut thread, input, b, 1), row, "remove");
                expected(operation(p, &mut thread, input, a, 1), row, "cleanup");
                assert_eq!(p.seh.kernel_dynamic_tables.len(), 0, "{row}");
                total += 1;
            }
        }
    }
    assert_eq!(total, 580);
}

#[test]
fn native_dynamic_function_table_replays_isolated_ranges_and_overlap() {
    let mut total = 0;
    for (profile, arch) in profiles() {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, a, b) = (memory, memory + 0x100, memory + 0x300);
        for kind in ["ranges-isolated", "overflow", "overlap", "zero-overlap"] {
            for execution in executions(kind, profile) {
                p.seh.kernel_dynamic_tables = Default::default();
                let row = rows(&execution)
                    .find(|r| r.get("add").is_some() || r.get("add-A").is_some())
                    .unwrap();
                let overlap = kind == "overlap" || kind == "zero-overlap";
                let (base, maximum, other_base, other_maximum) = if overlap {
                    (
                        hexadecimal(row, "A-base"),
                        hexadecimal(row, "A-max"),
                        hexadecimal(row, "B-base"),
                        hexadecimal(row, "B-max"),
                    )
                } else {
                    (
                        hexadecimal(row, "base"),
                        hexadecimal(row, "maximum"),
                        hexadecimal(row, "remove-base"),
                        hexadecimal(row, "remove-maximum"),
                    )
                };
                record(p, a, base, maximum);
                record(p, b, other_base, other_maximum);
                if !overlap {
                    for table in [a, b] {
                        p.space
                            .w64(table + 16, hexadecimal(row, "functions"))
                            .unwrap();
                        p.space
                            .w64(table + 32, hexadecimal(row, "minimum"))
                            .unwrap();
                    }
                    if row["axis"] == "remove-key" {
                        match decimal(row, "ordinal") {
                            44 => p.space.w64(b + 32, u64::MAX).unwrap(),
                            45 => p.space.w64(b + 16, u64::MAX).unwrap(),
                            _ => {}
                        }
                    }
                    expected(operation(p, &mut thread, input, a, 0), row, "add");
                    expected(operation(p, &mut thread, input, b, 1), row, "remove");
                    expected(operation(p, &mut thread, input, a, 1), row, "cleanup");
                } else {
                    let scenario = decimal(row, "scenario");
                    let remove_only = kind == "overlap" && matches!(scenario, 1 | 2);
                    expected(operation(p, &mut thread, input, a, 0), row, "add-A");
                    expected(
                        operation(p, &mut thread, input, b, u8::from(remove_only)),
                        row,
                        "operation",
                    );
                    expected(operation(p, &mut thread, input, b, 1), row, "remove-B");
                    expected(operation(p, &mut thread, input, a, 1), row, "remove-A");
                    expected(operation(p, &mut thread, input, a, 1), row, "again-A");
                }
                total += 1;
            }
        }
    }
    assert_eq!(total, 524);
}

#[test]
fn native_dynamic_function_table_replays_ordered_sequences_and_native_status_mappings() {
    let mut calls = 0;
    let mut mappings = 0;
    for (profile, arch) in profiles() {
        let (mut process, mut thread) = fixture(arch, "NtSetInformationProcess", SERVICE);
        let p = process.state_mut();
        let memory = scratch(p);
        let (input, table) = (memory, memory + 0x100);
        for execution in executions("sequence", profile) {
            p.seh.kernel_dynamic_tables = Default::default();
            for row in rows(&execution).filter(|r| r["record"] == "operation") {
                let delta: i64 = field(row, "delta").parse().unwrap();
                let base = (0x400_0000_0000i64 + delta) as u64;
                record(p, table, base, base + decimal(row, "size"));
                expected(
                    operation(p, &mut thread, input, table, decimal(row, "remove") as u8),
                    row,
                    "status",
                );
                calls += 1;
            }
            assert_eq!(p.seh.kernel_dynamic_tables.len(), 0);
        }
        for execution in executions("status-map", profile) {
            for row in rows(&execution) {
                assert_eq!(
                    nt::status_to_error(hexadecimal(row, "status") as u32),
                    decimal(row, "error") as u32,
                    "{row}"
                );
                assert_eq!(hexadecimal(row, "last-error"), u64::from(LAST_ERROR));
                mappings += 1;
            }
        }
    }
    assert_eq!(calls, 3016);
    assert_eq!(mappings, 88);
}
