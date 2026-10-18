use rax::user::windows::{WindowsConfig, WindowsProcess, layout, memory::Mem, process::RunStatus};
use std::sync::atomic::AtomicBool;

fn main() {
    for path in std::env::args().skip(1) {
        let mut cfg = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
        cfg.native_libraries = true;
        cfg.arena_bytes = 268435456;
        cfg.slice_insns = 1;
        let mut process = WindowsProcess::spawn_image(cfg, std::fs::read(&path).unwrap()).unwrap();
        let cancelled = AtomicBool::new(false);
        let skipped = 2_100_000;
        let result = process.run_slice(skipped, &cancelled);
        println!("program={path} prefix-turns={skipped} result={result:?}");
        if matches!(result, RunStatus::Complete(_)) {
            continue;
        }
        for turn in skipped..3_000_000 {
            let p = process.state();
            let o = layout::offsets(p.arch);
            for t in p.threads.values() {
                if let Ok(bytes) = p.space.bytes(t.cpu.pc(), 4) {
                    let raw = u32::from_le_bytes(bytes.try_into().unwrap());
                    if raw & 0xffe0001f == 0xd4000001 && (raw >> 5) & 0xffff == 0x58 {
                        let attrs = t.cpu.gpr(2);
                        let (root_offset, name_offset, flag_offset, buffer_offset) = if o.ptr == 8 {
                            (8, 16, 24, 8)
                        } else {
                            (4, 8, 12, 4)
                        };
                        println!(
                            "directory turn={turn} access={:#x} root={:x?} flags={:x?}",
                            t.cpu.gpr(1),
                            p.space.ptr(attrs + root_offset, o.ptr),
                            p.space.u32(attrs + flag_offset)
                        );
                        if let Ok(name) = p.space.ptr(attrs + name_offset, o.ptr) {
                            if let (Ok(length), Ok(buffer)) =
                                (p.space.u16(name), p.space.ptr(name + buffer_offset, o.ptr))
                            {
                                let bytes = p.space.bytes(buffer, usize::from(length)).unwrap();
                                let units: Vec<_> = bytes
                                    .chunks_exact(2)
                                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                                    .collect();
                                println!(
                                    "directory name={:?} length={length} raw={units:x?}",
                                    String::from_utf16_lossy(&units)
                                );
                            }
                        }
                    }
                }
            }
            let result = process.run_slice(1, &cancelled);
            if matches!(result, RunStatus::Complete(_)) {
                println!("terminal turn={turn} {result:?}");
                break;
            }
        }
    }
}
