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
        let pc = t.cpu.pc().saturating_sub(4);
        println!("terminal-region={:?}", p.vm.query(pc));
        println!(
            "terminal-code={:X?}",
            p.space.bytes(pc.saturating_sub(32), 96)
        );
        for module in &p.modules.list {
            println!(
                "registered-module name={:?} base={:#x} size={:#x} host={:?}",
                module.name, module.base, module.size, module.host_path
            );
        }
        for module in p.modules.list.iter().filter(|module| module.contains(pc)) {
            println!(
                "terminal-module name={:?} base={:#x} size={:#x} rva={:#x} host={:?}",
                module.name,
                module.base,
                module.size,
                pc - module.base,
                module.host_path
            );
            let start = pc.saturating_sub(32).max(module.base);
            println!(
                "terminal-module-code start={start:#x} bytes={:X?}",
                p.space.bytes(start, 64)
            );
            let within =
                |rva: u64, size: u64| rva.checked_add(size).is_some_and(|end| end <= module.size);
            if within(u64::from(module.exports.rva), 40) {
                let directory = p
                    .space
                    .bytes(module.base + u64::from(module.exports.rva), 40)
                    .unwrap();
                let word =
                    |at: usize| u32::from_le_bytes(directory[at..at + 4].try_into().unwrap());
                let functions = u64::from(word(20));
                let names = u64::from(word(24));
                let function_table = u64::from(word(28));
                let name_table = u64::from(word(32));
                let ordinals = u64::from(word(36));
                if functions <= 1_048_576
                    && names <= 65_536
                    && within(function_table, functions * 4)
                    && within(name_table, names * 4)
                    && within(ordinals, names * 2)
                {
                    for index in 0..names {
                        let ordinal =
                            u64::from(p.space.u16(module.base + ordinals + index * 2).unwrap());
                        if ordinal >= functions {
                            continue;
                        }
                        let rva = u64::from(
                            p.space
                                .u32(module.base + function_table + ordinal * 4)
                                .unwrap(),
                        );
                        if !within(rva, 4) || rva > pc - module.base || pc - module.base - rva > 32
                        {
                            continue;
                        }
                        let name =
                            u64::from(p.space.u32(module.base + name_table + index * 4).unwrap());
                        if within(name, 128) {
                            let bytes = p.space.bytes(module.base + name, 128).unwrap();
                            let length = bytes
                                .iter()
                                .position(|&byte| byte == 0)
                                .unwrap_or(bytes.len());
                            println!(
                                "terminal-export name={:?} rva={rva:#x} displacement={:#x}",
                                String::from_utf8_lossy(&bytes[..length]),
                                pc - module.base - rva
                            );
                        }
                    }
                }
            }
        }
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
