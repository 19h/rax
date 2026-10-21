use rax::user::windows::{WindowsConfig, WindowsProcess, memory::Mem, process::RunStatus};
use std::sync::atomic::AtomicBool;
fn main() {
    let program = std::env::args().nth(1).unwrap();
    let mut cfg = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
    cfg.native_libraries = true;
    cfg.arena_bytes = 268435456;
    cfg.slice_insns = 4096;
    let mut process = WindowsProcess::spawn_image(cfg, std::fs::read(program).unwrap()).unwrap();
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
            (0..31).map(|n| t.cpu.gpr(n)).collect::<Vec<_>>()
        );
        let port = t.cpu.gpr(0);
        println!(
            "alpc port={port:X} object-kind={:?}",
            p.objects.get(port).map(std::mem::discriminant)
        );
        let send = t.cpu.gpr(2);
        let receive = t.cpu.gpr(4);
        let length = t.cpu.gpr(5);
        let sent = p.space.u16(send + 2).map_or(256, usize::from).min(4096);
        println!(
            "send-message={send:X} captured-bytes={sent} raw={:X?}",
            p.space.bytes(send, sent)
        );
        println!(
            "receive-message={receive:X} raw={:X?}",
            p.space.bytes(receive, 256)
        );
        println!(
            "buffer-length-pointer={length:X} value={:X?}",
            p.space.ptr(length, p.arch.ptr_size())
        );
        for (name, address) in [
            ("send-attributes", t.cpu.gpr(3)),
            ("receive-attributes", t.cpu.gpr(6)),
            ("timeout", t.cpu.gpr(7)),
        ] {
            println!("{name}={address:X} raw={:X?}", p.space.bytes(address, 64));
        }
    }
}
