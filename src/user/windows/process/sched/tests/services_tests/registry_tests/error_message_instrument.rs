use super::*;
use crate::user::windows::registry::{ERROR_MESSAGE_INSTRUMENT_KEY, TreeRoot};

const LOGS: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-error-message-instrument/native/native-error-message-instrument-20261021-v1-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-error-message-instrument/native/native-error-message-instrument-20261021-v1-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-error-message-instrument/native/native-error-message-instrument-20261021-v1-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-error-message-instrument/native/native-error-message-instrument-20261021-v1-four-views-x86-laa.log"
    )),
];

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_ascii_whitespace()
        .find_map(|word| word.strip_prefix(key))
        .unwrap()
}

#[test]
fn native_error_message_instrument_registry_open_replays_all_execution_views() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::ErrorMessageInstrument, None)
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        let error = thread.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
        let handles = p.objects.handle_count();
        let mut count = 0;
        for log in LOGS {
            let mut view_count = 0;
            for row in log.lines().filter(|line| line.starts_with("OPEN ")) {
                let spelling: u32 = field(row, "spelling=").parse().unwrap();
                let mut path = ERROR_MESSAGE_INSTRUMENT_KEY.to_owned();
                if spelling != 0 {
                    path.push('\\');
                }
                if spelling == 2 {
                    path.make_ascii_uppercase();
                }
                if spelling == 3 {
                    path.make_ascii_lowercase();
                }
                let flags = u32::from_str_radix(field(row, "flags="), 16).unwrap();
                let access = u32::from_str_radix(field(row, "grant="), 16).unwrap();
                let status = u32::from_str_radix(field(row, "status="), 16).unwrap();
                let output = u64::from_str_radix(field(row, "handle="), 16).unwrap();
                assert_eq!(field(row, "exception="), "00000000");
                unicode(p, attrs + 64, attrs + 128, &path);
                attributes(p, attrs, attrs + 64, 0, flags);
                p.space.wptr(base, arch.ptr_size(), 0xA5A5A5A5).unwrap();
                p.space.w32(error, 0x12345678).unwrap();
                assert_eq!(
                    call(
                        p,
                        &mut thread,
                        "NtOpenKey",
                        &[base, u64::from(access), attrs]
                    ),
                    status,
                    "{arch}: {row}"
                );
                assert_eq!(p.space.ptr(base, arch.ptr_size()).unwrap(), output);
                assert_eq!(
                    p.space.u32(error).unwrap(),
                    u32::from_str_radix(field(row, "last-error="), 16).unwrap()
                );
                assert_eq!(p.objects.handle_count(), handles);
                view_count += 1;
            }
            assert_eq!(view_count, 72);
            count += view_count;
        }
        assert_eq!(count, 288);
    }
}
