use rax::user::windows::{WindowsConfig, WindowsProcess, memory::Mem, process::RunStatus};
use std::sync::atomic::AtomicBool;
fn main() {
    let program = std::env::args().nth(1).unwrap();
    let mut cfg = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
    cfg.native_libraries = true;
    cfg.arena_bytes = 268435456;
    cfg.slice_insns = 4096;
    let mut process = WindowsProcess::spawn_image(cfg, std::fs::read(&program).unwrap()).unwrap();
    let cancel = AtomicBool::new(false);
    let mut done = false;
    for turn in 0..100_000 {
        let result = process.run_slice(1, &cancel);
        if matches!(result, RunStatus::Complete(_)) {
            println!("terminal turn={turn} result={result:?}");
            done = true;
            break;
        }
    }
    assert!(done, "bounded diagnostic exhausted");
    let p = process.state();
    for t in p.threads.values() {
        println!(
            "thread={} pc={:X} sp={:X} frames={}",
            t.tid,
            t.cpu.pc(),
            t.cpu.sp(),
            t.frames.len()
        );
        println!(
            "registers={:X?}",
            (0..8).map(|n| t.cpu.gpr(n)).collect::<Vec<_>>()
        );
        let attributes = t.cpu.gpr(2);
        println!(
            "object-attributes={attributes:X} raw={:X?}",
            p.space.bytes(attributes, 48)
        );
        if let (Ok(root), Ok(name), Ok(flags)) = (
            p.space.u64(attributes + 8),
            p.space.u64(attributes + 16),
            p.space.u32(attributes + 24),
        ) {
            println!(
                "registry desired={:X} root={root:X} root-kind={:?} flags={flags:X}",
                t.cpu.gpr(1),
                p.objects.get(root).map(std::mem::discriminant)
            );
            println!("name-header={name:X} raw={:X?}", p.space.bytes(name, 16));
            if let (Ok(length), Ok(maximum), Ok(buffer)) = (
                p.space.u16(name),
                p.space.u16(name + 2),
                p.space.u64(name + 8),
            ) {
                assert!(
                    length <= 2048 && length % 2 == 0,
                    "bounded UTF16 key length"
                );
                let bytes = p.space.bytes(buffer, usize::from(length)).unwrap();
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|x| u16::from_le_bytes([x[0], x[1]]))
                    .collect();
                println!(
                    "key length-bytes={length} maximum-bytes={maximum} buffer={buffer:X} units={units:X?} utf16={:?}",
                    String::from_utf16(&units)
                );
            }
        }
    }
}
