use rax::user::windows::{WindowsConfig, WindowsProcess};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateEventW(sa: *const c_void, manual: i32, initial: i32, name: *const u16) -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}
fn main() {
    let image = std::fs::read(std::env::args().nth(1).expect("fixture path")).unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let barrier = Arc::new(Barrier::new(2));
    let worker_running = running.clone();
    let worker_barrier = barrier.clone();
    let worker = std::thread::spawn(move || {
        let mut handles = VecDeque::new();
        let mut created = 0u64;
        let mut closed = 0u64;
        let mut error = None;
        worker_barrier.wait();
        while worker_running.load(Ordering::Relaxed) {
            let name: Vec<u16> = format!(
                "Local\\AssistNativeCaptureChurn_{}_{}",
                std::process::id(),
                created
            )
            .encode_utf16()
            .chain([0])
            .collect();
            // SAFETY: this fixture owns its unique event name and native handles.
            let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
            if handle.is_null() {
                error = Some(unsafe { GetLastError() });
                break;
            }
            if unsafe { GetLastError() } == 183 {
                unsafe { CloseHandle(handle) };
                error = Some(183);
                break;
            }
            handles.push_back(handle);
            created += 1;
            if handles.len() > 64 {
                if unsafe { CloseHandle(handles.pop_front().unwrap()) } == 0 {
                    error = Some(unsafe { GetLastError() });
                    break;
                }
                closed += 1;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        for handle in handles {
            if unsafe { CloseHandle(handle) } != 0 {
                closed += 1;
            } else {
                error = Some(unsafe { GetLastError() });
            }
        }
        (created, closed, error)
    });
    barrier.wait();
    let mut ok = 0;
    let mut failed = 0;
    for index in 0..16 {
        let mut config =
            WindowsConfig::embedded("C:\\app\\capture-churn.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        config.arena_bytes = 64 << 20;
        match WindowsProcess::spawn_image(config, image.clone()) {
            Ok(_) => {
                ok += 1;
                println!("capture index={index} result=ok");
            }
            Err(e) => {
                failed += 1;
                println!("capture index={index} result=failed error={e:?}");
            }
        }
    }
    running.store(false, Ordering::Relaxed);
    let (created, closed, error) = worker.join().expect("churn worker");
    println!(
        "complete captures=16 ok={ok} failed={failed} created={created} closed={closed} error={error:?} scope=capture-only-no-guest-execution"
    );
    if failed != 0 || error.is_some() || created != closed || created == 0 {
        std::process::exit(1);
    }
}
