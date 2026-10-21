//! Compiled two-fault capture replay against four independent native ABI runs.
use super::Harness;
use crate::user::mm::PAGE_SIZE;
use crate::user::windows::arch::WinArch;
use crate::user::windows::memory::{Mem, prot};

fn fault(h: &mut Harness, args: &mut [u64; 6], id: u64) {
    let attrs = h.attrs();
    let p = h.process.state_mut();
    let width = p.arch.ptr_size();
    let missing = h.base + 4 * PAGE_SIZE;
    let guarded = h.base + 5 * PAGE_SIZE;
    match id {
        0 => p.space.w32(attrs, 0).unwrap(),
        1 => p.space.w32(attrs + 3 * width, u32::MAX).unwrap(),
        2 => p.space.wptr(attrs + 5 * width, width, 1).unwrap(),
        3 => p.space.wptr(attrs + 4 * width, width, missing).unwrap(),
        4 => p.space.wptr(attrs + 2 * width, width, 1).unwrap(),
        5 => p.space.wptr(attrs + 64 + width, width, missing).unwrap(),
        6 => p.space.w16(attrs + 64, 3).unwrap(),
        7 => p.space.wptr(attrs + width, width, 0x6ffc).unwrap(),
        8 => args[1] = 0,
        9 => args[1] = 0x80000000,
        10 => args[4] = 8,
        11 => args[5] = u64::from(u32::MAX),
        12 => args[0] = 0,
        13 => args[3] = 0,
        14 => args[0] = missing,
        15 => args[3] = missing,
        16 => args[0] = guarded,
        17 => args[3] = guarded,
        18 => args[2] = 1,
        19 => args[2] = missing,
        20 => args[2] = guarded,
        _ => panic!("unknown independent native fault {id}"),
    }
}

fn protection(h: &Harness, address: u64) -> u32 {
    h.process
        .state()
        .vm
        .query(address)
        .map_or(0, |region| region.protect)
}

fn readable(h: &Harness, address: u64) -> bool {
    let protect = protection(h, address);
    protect != 0 && protect & (prot::GUARD | prot::NOACCESS) == 0
}

#[test]
fn native_file_open_pairwise_capture_and_copyback_match_independent_native_runs() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!("paired-v7.json")).unwrap();
    let rows = oracle["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 210);
    let mut failures = Vec::new();
    for arch in WinArch::ALL {
        for row in rows {
            let mut h = Harness::new(arch);
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
            let p = h.process.state_mut();
            p.vm.protect(h.base + 4 * PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                .unwrap();
            p.vm.protect(
                h.base + 5 * PAGE_SIZE,
                PAGE_SIZE,
                prot::READWRITE | prot::GUARD,
            )
            .unwrap();
            let error = h.thread.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
            p.space.w32(error, 0x12345678).unwrap();
            let mut args = [h.base, 0x100021, h.attrs(), h.base + PAGE_SIZE, 5, 0x60];
            fault(&mut h, &mut args, row["left"].as_u64().unwrap());
            fault(&mut h, &mut args, row["right"].as_u64().unwrap());
            let code = h.invoke(&args);
            let expected = &row[if arch == WinArch::X86 {
                "wow"
            } else {
                "native"
            }];
            let width = arch.ptr_size();
            let handle = if readable(&h, args[0]) {
                let value = h.process.state().space.ptr(args[0], width).unwrap();
                if value == 0 {
                    "zero"
                } else if value == arch.ptr(0xa5a5a5a5a5a5a5a5) {
                    "sentinel"
                } else {
                    "published"
                }
            } else {
                "-"
            };
            let valid = readable(&h, args[0])
                && h.process
                    .state()
                    .space
                    .ptr(args[0], width)
                    .ok()
                    .is_some_and(|handle| h.process.state().objects.get(handle).is_some());
            let iosb = if readable(&h, args[3]) {
                h.process
                    .state()
                    .space
                    .bytes(args[3], (2 * width) as usize)
                    .unwrap()
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<String>()
            } else {
                "-".to_owned()
            };
            let actual = serde_json::json!({"status":code,"hprot":protection(&h,args[0]),"iprot":protection(&h,args[3]),"handle":handle,"valid":valid,"iosb":iosb});
            assert_eq!(h.process.state().space.u32(error).unwrap(), 0x12345678);
            if &actual != expected {
                failures.push(serde_json::json!({"arch":arch.to_string(),"role":row["role"],"actual":actual,"expected":expected}));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "native file-open pair mismatches: {}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}
