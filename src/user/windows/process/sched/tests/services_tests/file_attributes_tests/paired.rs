//! Independent native v10 fault pairs, reached through the NT dispatcher.

use super::Harness;
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::memory::{Mem, prot};

fn fault(h: &mut Harness, id: u64, output: &mut u64) {
    let attrs = h.attrs();
    let p = h.process.state_mut();
    let width = p.arch.ptr_size();
    let descriptor = attrs + 512;
    let qos = attrs + 768;
    let missing = h.base + 3 * PAGE_SIZE;
    let guarded = h.base + 4 * PAGE_SIZE;
    let security_slot = attrs + if p.arch.is64() { 32 } else { 16 };
    let qos_slot = attrs + if p.arch.is64() { 40 } else { 20 };
    match id {
        0 => p.space.w32(attrs, 0).unwrap(),
        1 => p.space.w32(attrs + 3 * width, u32::MAX).unwrap(),
        2 => p.space.wptr(qos_slot, width, 1).unwrap(),
        3 => p.space.wptr(qos_slot, width, missing).unwrap(),
        4 => {
            p.space.w32(qos + 4, 4).unwrap();
            p.space.wptr(qos_slot, width, qos).unwrap();
        }
        5 => {
            p.space.w8(qos + 8, 2).unwrap();
            p.space.wptr(qos_slot, width, qos).unwrap();
        }
        6 => p.space.wptr(security_slot, width, 1).unwrap(),
        7 => p.space.wptr(security_slot, width, missing).unwrap(),
        8 => {
            p.space.w8(descriptor, 0).unwrap();
            p.space.wptr(security_slot, width, descriptor).unwrap();
        }
        9 => {
            p.space
                .wptr(descriptor + if p.arch.is64() { 8 } else { 4 }, width, 1)
                .unwrap();
            p.space.wptr(security_slot, width, descriptor).unwrap();
        }
        10 => p.space.wptr(attrs + 2 * width, width, 1).unwrap(),
        11 => p.space.wptr(attrs + 64 + width, width, missing).unwrap(),
        12 => {
            let length = usize::from(p.space.u16(attrs + 64).unwrap());
            let bytes = p.space.bytes(attrs + 128, length).unwrap();
            p.space.wr(attrs + 1025, &bytes).unwrap();
            p.space
                .wptr(attrs + 64 + width, width, attrs + 1025)
                .unwrap();
        }
        13 => p.space.w16(attrs + 64, 3).unwrap(),
        14 => p.space.wptr(attrs + width, width, 0x6ffc).unwrap(),
        15 | 16 => {
            *output = h.base + PAGE_SIZE;
            p.vm.protect(
                *output,
                PAGE_SIZE,
                if id == 15 {
                    prot::NOACCESS
                } else {
                    prot::READWRITE | prot::GUARD
                },
            )
            .unwrap();
        }
        17 => p.space.wptr(qos_slot, width, guarded).unwrap(),
        18 => p.space.wptr(security_slot, width, guarded).unwrap(),
        19 => p.space.wptr(attrs + 2 * width, width, guarded).unwrap(),
        20 => p.space.wptr(attrs + 64 + width, width, guarded).unwrap(),
        _ => panic!("unknown independent native fault index {id}"),
    }
}

#[test]
fn native_file_attributes_pairwise_capture_matches_independent_native_oracle() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!("paired-v10.json")).unwrap();
    let rows = oracle["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 231);
    let mut failures = Vec::new();
    for arch in WinArch::ALL {
        for api in ["NtQueryAttributesFile", "NtQueryFullAttributesFile"] {
            for row in rows {
                let mut h = Harness::new(arch);
                let attrs = h.attrs();
                let p = h.process.state_mut();
                p.space.wr(attrs + 512, &[0; 40]).unwrap();
                p.space.w8(attrs + 512, 1).unwrap();
                p.space.wr(attrs + 768, &[0; 12]).unwrap();
                p.space.w32(attrs + 768, 12).unwrap();
                p.space.w32(attrs + 772, 2).unwrap();
                p.vm.protect(h.base + 3 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
                p.vm.protect(
                    h.base + 4 * PAGE_SIZE,
                    PAGE_SIZE,
                    prot::READWRITE | prot::GUARD,
                )
                .unwrap();
                let mut output = h.base;
                fault(&mut h, row["left"].as_u64().unwrap(), &mut output);
                fault(&mut h, row["right"].as_u64().unwrap(), &mut output);
                let status = h.query(api, attrs, output);
                let protection = h
                    .process
                    .state()
                    .vm
                    .query(h.base + PAGE_SIZE)
                    .unwrap()
                    .protect;
                let expected = &row[if arch == WinArch::X86 {
                    "wow"
                } else {
                    "native"
                }];
                if u64::from(status) != expected[0].as_u64().unwrap()
                    || u64::from(protection) != expected[1].as_u64().unwrap()
                {
                    failures.push((
                        arch,
                        api,
                        row["role"].as_u64().unwrap(),
                        status,
                        protection,
                        expected.clone(),
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "independent native pair mismatches: {failures:#x?}"
    );
}
