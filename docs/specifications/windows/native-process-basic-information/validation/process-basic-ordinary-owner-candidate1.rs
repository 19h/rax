use rax::user::windows::{WindowsConfig, WindowsProcess, process::RunStatus};
use std::sync::atomic::AtomicBool;

fn main() {
    for program in std::env::args().skip(1) {
        let mut config =
            WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        config.arena_bytes = 268435456;
        config.slice_insns = 4096;
        let mut process =
            WindowsProcess::spawn_image(config, std::fs::read(&program).unwrap()).unwrap();
        let cancelled = AtomicBool::new(false);
        let mut terminal = false;
        for call in 0..10_000 {
            let result = process.run_slice(1, &cancelled);
            if matches!(result, RunStatus::Complete(_)) {
                println!("program={program} terminal-call={call} result={result:?}");
                terminal = true;
                break;
            }
        }
        assert!(
            terminal,
            "program exceeded10000scheduler-slice observation budget"
        );
    }
}
