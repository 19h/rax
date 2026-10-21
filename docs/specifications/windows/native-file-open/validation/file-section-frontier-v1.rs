use rax::user::windows::{
    WindowsConfig, WindowsProcess, memory::Mem, objects::Object, process::RunStatus,
};
use std::sync::atomic::AtomicBool;
fn main() {
    let program = std::env::args().nth(1).unwrap();
    let mut config = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
    config.native_libraries = true;
    config.arena_bytes = 268435456;
    config.slice_insns = 4096;
    let mut process = WindowsProcess::spawn_image(config, std::fs::read(program).unwrap()).unwrap();
    let cancel = AtomicBool::new(false);
    let mut complete = false;
    for turn in 0..10_000 {
        let result = process.run_slice(1, &cancel);
        if matches!(result, RunStatus::Complete(_)) {
            println!("terminal slice={turn} result={result:?}");
            complete = true;
            break;
        }
    }
    assert!(complete);
    let state = process.state();
    for thread in state.threads.values() {
        let args: Vec<_> = (0..8).map(|index| thread.cpu.gpr(index)).collect();
        println!(
            "thread={} pc={:#x} args={args:x?}",
            thread.tid,
            thread.cpu.pc()
        );
        println!("section-output={:x?}", state.space.bytes(args[0], 8));
        if args[3] != 0 {
            println!("maximum-size={:x?}", state.space.bytes(args[3], 8));
        }
        if args[2] != 0 {
            println!("object-attributes={:x?}", state.space.bytes(args[2], 48));
        }
        if let Some(Object::File(file)) = state.objects.get(args[6]) {
            println!(
                "backing-file handle={:#x} access={:#x} share={:#x} directory={} path={:?} host-path={:?} owned={}",
                args[6],
                file.access,
                file.share,
                file.directory,
                file.path,
                file.host_path,
                file.host.is_some()
            );
        }
    }
}
