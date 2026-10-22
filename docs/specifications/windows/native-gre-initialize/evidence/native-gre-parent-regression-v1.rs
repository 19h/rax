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
    let mut terminal = String::new();
    for turn in 0..100_000 {
        let result = process.run_slice(1, &cancel);
        if matches!(result, RunStatus::Complete(_)) {
            println!("terminal turn={turn} result={result:?}");
            terminal = format!("{result:?}");
            if program.to_ascii_lowercase().ends_with("whoami.exe") {
                assert!(
                    turn >= 2872,
                    "ordinary probe stopped before the measured GRE boundary"
                );
            }
            done = true;
            break;
        }
    }
    assert!(done, "bounded diagnostic exhausted");
    let p = process.state();
    let mut unsupported_gre = false;
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
        // Bounded, read-only diagnostics for a terminal NtOpenKey boundary.
        // Retain UTF-16 bytes as well as decoded text; no host key is opened.
        let width = p.arch.ptr_size();
        let object_size = if width == 8 { 48 } else { 24 };
        if let Ok(attributes) = p.space.bytes(output, object_size) {
            let pointer = |offset: usize| -> u64 {
                if width == 8 {
                    u64::from_le_bytes(attributes[offset..offset + 8].try_into().unwrap())
                } else {
                    u64::from(u32::from_le_bytes(
                        attributes[offset..offset + 4].try_into().unwrap(),
                    ))
                }
            };
            let root = pointer(if width == 8 { 8 } else { 4 });
            let name = pointer(if width == 8 { 16 } else { 8 });
            if let Ok(unicode) = p.space.bytes(name, if width == 8 { 16 } else { 8 }) {
                let length = usize::from(u16::from_le_bytes(unicode[..2].try_into().unwrap()));
                let capacity = usize::from(u16::from_le_bytes(unicode[2..4].try_into().unwrap()));
                let buffer = if width == 8 {
                    u64::from_le_bytes(unicode[8..16].try_into().unwrap())
                } else {
                    u64::from(u32::from_le_bytes(unicode[4..8].try_into().unwrap()))
                };
                if length <= 4096 && length <= capacity && length % 2 == 0 {
                    if let Ok(bytes) = p.space.bytes(buffer, length) {
                        let words = bytes
                            .chunks_exact(2)
                            .map(|word| u16::from_le_bytes(word.try_into().unwrap()))
                            .collect::<Vec<_>>();
                        let target = "\\Registry\\Machine\\Software\\Microsoft\\Windows NT\\CurrentVersion\\GRE_Initialize".encode_utf16().collect::<Vec<_>>();
                        unsupported_gre |= root == 0
                            && words == target
                            && terminal.contains("registry key outside selected runtime snapshot");
                        println!(
                            "terminal-object-name root={root:#x} utf16={words:X?} decoded={:?}",
                            String::from_utf16_lossy(&words)
                        );
                    }
                }
            }
        }
    }
    assert!(
        !unsupported_gre,
        "independently present GRE_Initialize root remains outside the actual owning runtime snapshot"
    );
}
