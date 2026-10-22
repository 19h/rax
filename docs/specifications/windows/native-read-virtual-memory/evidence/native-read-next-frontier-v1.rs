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
        let handle = t.cpu.gpr(0);
        let base = t.cpu.gpr(1);
        let output = t.cpu.gpr(2);
        let length = t.cpu.gpr(3);
        let returned = t.cpu.gpr(4);
        println!(
            "raw-arguments x0={handle:X} x1={base:X} x2={output:X} x3={length:X} x4={returned:X}"
        );
        println!(
            "argument2-memory={:X?}",
            p.space
                .bytes(output, usize::try_from(length.min(256)).unwrap())
        );
        println!(
            "argument1-memory={:X?}",
            p.space
                .bytes(base, usize::try_from(length.min(256)).unwrap())
        );
        println!("argument4-memory={:X?}", p.space.bytes(returned, 8));
    }
}
