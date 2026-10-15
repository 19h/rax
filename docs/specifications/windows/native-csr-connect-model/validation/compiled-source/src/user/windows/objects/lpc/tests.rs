use super::*;
use crate::user::mm::{AddressSpace, SharedObject, SpaceConfig};
use crate::user::windows::memory::{Mem, mem, prot};
use std::sync::Arc;

fn memory() -> VirtualMemory {
    VirtualMemory::new(
        AddressSpace::new(SpaceConfig {
            va_limit: 1 << 47,
            arena_bytes: 16 << 20,
            reserved_phys: vec![],
        })
        .unwrap(),
        65536,
        1 << 47,
    )
}

#[test]
fn lpc_final_handle_retires_local_view_despite_internal_pins_and_protected_close() {
    let mut vm = memory();
    let mut objects = Objects::default();
    let object = Arc::new(SharedObject::anonymous(65536).unwrap());
    let first = vm.map_shared_view(object.clone(), None, 0, 65536).unwrap();
    let peer = vm.map_shared_view(object, None, 0, 65536).unwrap();
    let id = objects.create(Object::LpcPort(LpcPort::new(Some(first.clone()))));
    objects.retain_many(&[id]).unwrap();
    let handle = objects.open_access(id, false, 0x001F_0001).unwrap();
    let duplicate = objects.duplicate(handle.into(), true).unwrap();
    assert_eq!(objects.get(handle.into()).unwrap().type_name(), "ALPC Port");
    assert_eq!(objects.access(duplicate.into()), Some(0x001F_0001));
    objects.set_flags(handle.into(), 2, 2);
    assert!(objects.close(handle.into()).is_err());
    objects.reclaim_lpc_views(&mut vm).unwrap();
    assert!(vm.allocation(first.base).is_some());
    objects.set_flags(handle.into(), 2, 0);
    objects.close(handle.into()).unwrap();
    objects.reclaim_lpc_views(&mut vm).unwrap();
    assert!(vm.allocation(first.base).is_some());
    objects.close(duplicate.into()).unwrap();
    assert!(objects.obj(id).is_some(), "independent pin retains object");
    objects.reclaim_lpc_views(&mut vm).unwrap();
    assert!(
        vm.allocation(first.base).is_none(),
        "last local handle retires view"
    );
    assert!(
        vm.allocation(peer.base).is_some(),
        "peer view has independent lifetime"
    );
    objects.release(id);
    objects.reclaim_lpc_views(&mut vm).unwrap();
    vm.retire_shared_view(&peer).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
}

#[test]
fn lpc_process_drain_and_unpublished_rollback_retire_every_view_once() {
    let mut vm = memory();
    let mut objects = Objects::default();
    let object = Arc::new(SharedObject::anonymous(65536).unwrap());
    for _ in 0..3 {
        let owner = vm.map_shared_view(object.clone(), None, 0, 65536).unwrap();
        let handle = objects.insert(Object::LpcPort(LpcPort::new(Some(owner))));
        objects.set_flags(handle.into(), 2, 2);
    }
    // The drain must enqueue all views even if its consumer stops early.
    drop(objects.drain().next());
    assert_eq!(objects.iter().count(), 0);
    objects.reclaim_lpc_views(&mut vm).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
    let owner = vm.map_shared_view(object, None, 0, 65536).unwrap();
    let id = objects.create(Object::LpcPort(LpcPort::new(Some(owner))));
    objects.release(id);
    objects.reclaim_lpc_views(&mut vm).unwrap();
    objects.reclaim_lpc_views(&mut vm).unwrap();
    assert_eq!(vm.committed_bytes(), 0);
}

fn fields(line: &str) -> std::collections::BTreeMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|field| field.split_once('='))
        .collect()
}

fn hex(value: &str) -> u64 {
    u64::from_str_radix(value, 16).unwrap()
}

/// Replays only the measured local VM/handle lifetime. It does not synthesize
/// native connection replies, remote-process views or biased PointerCount.
#[test]
fn lpc_owned_views_replay_all_30_native_lifecycle_and_private_reuse_traces() {
    const ROOT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-lpc-owned-views"
    );
    let mut count = 0;
    for fixture in [
        "native-lpc-view-lifetime2-identities.json",
        "native-lpc-view32-lifetime2-identities.json",
    ] {
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::path::Path::new(ROOT).join(fixture)).unwrap(),
        )
        .unwrap();
        for producer in manifest["producers"].as_array().unwrap() {
            for case in producer["cases"].as_array().unwrap() {
                let name = case["stdout"]["path"]
                    .as_str()
                    .unwrap()
                    .rsplit('\\')
                    .next()
                    .unwrap();
                let raw = std::fs::read_to_string(std::path::Path::new(ROOT).join(name)).unwrap();
                let connected: Vec<_> = raw
                    .lines()
                    .filter(|line| line.starts_with("region phase=connected "))
                    .map(fields)
                    .collect();
                assert_eq!(connected.len(), 2);
                let client = hex(connected[0]["address"]);
                let server = hex(connected[1]["address"]);
                let mut vm = memory();
                let mut objects = Objects::default();
                let backing = Arc::new(SharedObject::anonymous(65536).unwrap());
                let local = vm
                    .map_shared_view(backing.clone(), Some(client), 0, 65536)
                    .unwrap();
                let remote = vm
                    .map_shared_view(backing.clone(), Some(server), 0, 65536)
                    .unwrap();
                let section = objects.insert(Object::Mapping {
                    file: None,
                    size: 65536,
                    protect: prot::READWRITE,
                    attributes: 0x0800_0000,
                    shared: Some(backing),
                });
                let client_handle = objects.insert(Object::LpcPort(LpcPort::new(Some(local))));
                let peer_handle = objects.insert(Object::LpcPort(LpcPort::new(Some(remote))));
                let mut duplicate = None;
                let mut canary = false;
                for line in raw.lines() {
                    if line.starts_with("region phase=canary ") && !canary {
                        vm.space().write(client, &[0x69, 0x96]).unwrap();
                        canary = true;
                    }
                    if line.starts_with("object name=duplicate-client-port basic-status=00000000") {
                        assert!(line.contains("handle-count=2"));
                        duplicate = Some(objects.duplicate(client_handle.into(), false).unwrap());
                    }
                    if line.starts_with("manual-unmap status=00000000") {
                        vm.release(client).unwrap();
                    }
                    if line.starts_with("region phase=private-reused ") {
                        vm.allocate(
                            Some(client),
                            65536,
                            mem::RESERVE | mem::COMMIT,
                            prot::READWRITE,
                        )
                        .unwrap();
                        vm.space().write(client, &[0x12, 0x34]).unwrap();
                    }
                    if let Some(name) = line.strip_prefix("closed name=") {
                        let handle = match name {
                            "section" => Some(section),
                            "client-port" => Some(client_handle),
                            "server-peer" => Some(peer_handle),
                            "client-duplicate" => Some(duplicate.take().expect("native duplicate")),
                            "listening-port" => None,
                            other => panic!("unknown native close {other}"),
                        };
                        if let Some(handle) = handle {
                            objects.close(handle.into()).unwrap();
                            objects.reclaim_lpc_views(&mut vm).unwrap();
                        }
                    }
                    if line.starts_with("region ") {
                        let row = fields(line);
                        let address = hex(row["address"]);
                        let query = vm.query(address).unwrap();
                        assert_eq!(
                            (query.state, query.kind),
                            (hex(row["state"]) as u32, hex(row["type"]) as u32),
                            "{name}: {line}"
                        );
                        if row["read"] == "1" {
                            assert_eq!(query.protect, hex(row["protect"]) as u32, "{name}: {line}");
                            let data = hex(row["data"]);
                            let mut bytes = [0; 2];
                            vm.space().read(address, &mut bytes).unwrap();
                            assert_eq!(u16::from_be_bytes(bytes), data as u16, "{name}: {line}");
                        } else {
                            assert!(vm.space().u8(address).is_err(), "{name}: {line}");
                        }
                    }
                    if line == "cleanup-private present=1 freed=1" {
                        vm.release(client).unwrap();
                    }
                }
                assert_eq!(objects.handle_count(), 0, "{name}");
                assert_eq!(vm.committed_bytes(), 0, "{name}");
                count += 1;
            }
        }
    }
    assert_eq!(count, 30);
}
