use rax::user::windows::{WindowsConfig, WindowsProcess, memory::Mem, process::RunStatus};
use std::sync::atomic::AtomicBool;

fn main() {
    let program = std::env::args().nth(1).unwrap();
    let mut config = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
    config.native_libraries = true;
    config.arena_bytes = 268435456;
    config.slice_insns = 4096;
    let mut process =
        WindowsProcess::spawn_image(config, std::fs::read(&program).unwrap()).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut terminal = false;
    for turn in 0..10_000 {
        let result = process.run_slice(4096, &cancelled);
        if let RunStatus::Complete(_) = result {
            println!("terminal slice={turn} result={result:?}");
            terminal = true;
            break;
        }
    }
    assert!(terminal);
    let state = process.state();
    assert_eq!(state.arch, rax::user::windows::arch::WinArch::Arm64);
    for thread in state.threads.values() {
        println!(
            "thread tid={} pc={:#x} args={:x?}",
            thread.tid,
            thread.cpu.pc(),
            (0..8)
                .map(|index| thread.cpu.gpr(index))
                .collect::<Vec<_>>()
        );
        let attributes = thread.cpu.gpr(0);
        let length = state.space.u32(attributes);
        let root = state.space.u64(attributes + 8);
        let name = state.space.u64(attributes + 16);
        let flags = state.space.u32(attributes + 24);
        println!(
            "attributes pointer={attributes:#x} length={length:?} root={root:x?} name={name:x?} flags={flags:x?} bytes={:x?}",
            state.space.bytes(attributes, 48)
        );
        if let Ok(name) = name {
            if let (Ok(length), Ok(maximum), Ok(buffer)) = (
                state.space.u16(name),
                state.space.u16(name + 2),
                state.space.u64(name + 8),
            ) {
                println!("unicode length={length} maximum={maximum} buffer={buffer:#x}");
                if length % 2 == 0 && length <= maximum && length <= 8192 {
                    let raw = state.space.bytes(buffer, usize::from(length)).unwrap();
                    let units: Vec<u16> = raw
                        .chunks_exact(2)
                        .map(|part| u16::from_le_bytes([part[0], part[1]]))
                        .collect();
                    println!(
                        "object-name units={units:x?} text={:?}",
                        String::from_utf16_lossy(&units)
                    );
                }
            }
        }
        println!(
            "file-basic-output pointer={:#x} bytes={:x?}",
            thread.cpu.gpr(1),
            state.space.bytes(thread.cpu.gpr(1), 40)
        );
    }
}
