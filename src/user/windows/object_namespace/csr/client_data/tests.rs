use super::*;
use crate::user::mm::{AddressSpace, SpaceConfig};
use crate::user::windows::memory::Mem;
use crate::user::windows::object_namespace::data::DataRegion;

const LOGS: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-csr-client-connect-v6-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-csr-client-connect-v6-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-csr-client-connect-v6-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-alpc-csr/native/native-csr-client-connect-v6-four-views-x86-laa.log"
    )),
];

fn field<'a>(line: &'a str, name: &str) -> &'a str {
    line.split_ascii_whitespace()
        .find_map(|part| part.strip_prefix(name))
        .unwrap()
}
fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn capture(log: &str) -> ([u8; INFORMATION_BYTES], [ClientView; 2]) {
    let query = log
        .lines()
        .find(|line| line.starts_with("CONNECT sample=0 "))
        .unwrap();
    assert_eq!(field(query, "status="), "00000000");
    assert_eq!(field(query, "exception="), "00000000");
    let bytes = hex(field(query, "data="));
    assert_eq!(bytes.len(), 1024);
    assert!(bytes[INFORMATION_BYTES..].iter().all(|&byte| byte == 0xA5));
    let views = [16, 40].map(|slot| {
        let prefix = format!("MAP slot={slot} ");
        let lines: Vec<_> = log
            .lines()
            .filter(|line| line.starts_with(&prefix))
            .collect();
        let base = u64::from_str_radix(field(lines[0], "base="), 16).unwrap();
        let mut size = 0;
        let regions = lines
            .iter()
            .map(|line| {
                let offset: u64 = field(line, "offset=").parse().unwrap();
                let span: u64 = field(line, "bytes=").parse().unwrap();
                assert_eq!(offset, size);
                size += span;
                let committed = field(line, "state=") == "00001000";
                assert_eq!(field(line, "type="), "00040000");
                let bytes = if committed {
                    assert_eq!(field(line, "protect="), "00000002");
                    assert_eq!(field(line, "stable="), "1");
                    Some(hex(field(line, "hex=")).into())
                } else {
                    assert_eq!(field(line, "state="), "00002000");
                    assert_eq!(field(line, "protect="), "00000000");
                    None
                };
                DataRegion {
                    offset,
                    size: span,
                    bytes,
                }
            })
            .collect();
        ClientView {
            base,
            section: DataSection::private_readonly(size, regions).unwrap(),
        }
    });
    (bytes[..INFORMATION_BYTES].try_into().unwrap(), views)
}
fn vm() -> VirtualMemory {
    VirtualMemory::new(
        AddressSpace::new(SpaceConfig {
            va_limit: 1 << 47,
            arena_bytes: 1 << 20,
            reserved_phys: Vec::new(),
        })
        .unwrap(),
        65536,
        1 << 47,
    )
}

pub(super) fn fixture(index: usize) -> Arc<CsrClientData> {
    let (information, views) = capture(LOGS[index]);
    CsrClientData::selected(information, views).unwrap()
}

#[test]
fn native_csr_client_data_replays_all_four_selected_snapshots() {
    for log in LOGS {
        let (information, views) = capture(log);
        let data = CsrClientData::selected(information, views).unwrap();
        let mut vm = vm();
        data.map(&mut vm).unwrap();
        assert_eq!(vm.committed_bytes(), 40960);
        assert_eq!(data.information, information);
        for view in &data.views {
            assert!(view.section.path.is_empty());
            for region in &view.section.regions {
                let address = view.base + region.offset;
                if let Some(bytes) = &region.bytes {
                    assert_eq!(
                        vm.space().bytes(address, bytes.len()).unwrap(),
                        bytes.as_ref()
                    );
                    assert!(vm.space().w8(address, 1).is_err());
                    assert_eq!(vm.query(address).unwrap().protect, prot::READONLY);
                } else {
                    assert!(vm.space().u8(address).is_err());
                }
            }
        }
        let bases = data.views.each_ref().map(|view| view.base);
        drop(data);
        assert!(vm.space().u8(bases[0]).is_ok());
        for base in bases {
            vm.release(base).unwrap();
        }
        assert_eq!(vm.committed_bytes(), 0);
    }
}

#[test]
fn native_csr_client_data_mapping_collision_rolls_back_only_new_views() {
    for index in 0..2 {
        let (information, views) = capture(LOGS[0]);
        let data = CsrClientData::selected(information, views).unwrap();
        let mut vm = vm();
        let occupied = data.views[index].base;
        vm.map_data_section(
            data.views[index].section.clone(),
            occupied,
            0,
            data.views[index].section.size,
            prot::WRITECOPY,
        )
        .unwrap();
        vm.space().w32(occupied, 0x9669).unwrap();
        let committed = vm.committed_bytes();
        assert!(data.map(&mut vm).is_err());
        assert_eq!(vm.committed_bytes(), committed);
        assert_eq!(vm.space().u32(occupied).unwrap(), 0x9669);
        assert!(vm.allocation(data.views[1 - index].base).is_none());
    }
}

#[test]
fn native_csr_client_data_rejects_foreign_and_reserved_pointer_fields() {
    for offset in POINTER_SLOTS {
        for reserved in [false, true] {
            let (mut information, views) = capture(LOGS[0]);
            let address = if reserved {
                views[0].base + 8192
            } else {
                0x1234_0000
            };
            information[offset..offset + 8].copy_from_slice(&address.to_le_bytes());
            assert!(
                CsrClientData::selected(information, views).is_err(),
                "slot {offset} reserved {reserved}"
            );
        }
    }
}

#[test]
fn native_csr_client_data_does_not_rewrite_untyped_address_sized_values() {
    let (mut information, views) = capture(LOGS[0]);
    information[576..584].copy_from_slice(&0x1234_5678_9ABC_DEF0u64.to_le_bytes());
    let data = CsrClientData::selected(information, views).unwrap();
    assert_eq!(data.information, information);
}

#[test]
fn native_csr_client_data_reuse_checks_current_pages_and_snapshot_identity() {
    for protection in [
        prot::NOACCESS,
        prot::WRITECOPY,
        prot::READONLY | prot::GUARD,
    ] {
        let data = fixture(0);
        let mut vm = vm();
        assert_eq!(data.map(&mut vm).unwrap().len(), 2);
        assert!(data.map(&mut vm).unwrap().is_empty());
        let base = data.views[0].base;
        vm.protect(base + 4096, 4096, protection).unwrap();
        assert_eq!(vm.allocation(base).unwrap().protect, prot::READONLY);
        assert!(!vm.owns_readonly_data_view(base, &data.views[0].section));
        let committed = vm.committed_bytes();
        assert!(data.map(&mut vm).is_err());
        assert_eq!(vm.committed_bytes(), committed);
        assert_eq!(vm.query(base + 4096).unwrap().protect, protection);
        let other = fixture(0);
        assert!(!vm.owns_readonly_data_view(data.views[1].base, &other.views[1].section));
    }
}
