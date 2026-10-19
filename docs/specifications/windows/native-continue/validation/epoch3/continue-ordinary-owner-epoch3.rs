use rax::user::windows::{WindowsConfig, WindowsProcess, layout, memory::Mem, process::RunStatus};
use std::sync::atomic::AtomicBool;

use rax::user::image::pe::{
    RvaFault, RvaSource,
    exports::{ExportDirectory, ExportTarget},
};
use rax::user::windows::{context::RegContext, process::Proc};
struct Image<'a> {
    p: &'a Proc,
    base: u64,
    size: u64,
}
impl RvaSource for Image<'_> {
    fn read_rva(&self, rva: u64, buf: &mut [u8]) -> Result<(), RvaFault> {
        rva.checked_add(buf.len() as u64)
            .filter(|end| *end <= self.size)
            .ok_or(RvaFault { rva })?;
        let address = self.base.checked_add(rva).ok_or(RvaFault { rva })?;
        self.p.space.rd(address, buf).map_err(|_| RvaFault { rva })
    }
}
fn continue_service(p: &Proc) -> u32 {
    let m = &p.modules.list[p.modules.by_name("ntdll.dll").unwrap()];
    let image = Image {
        p,
        base: m.base,
        size: m.size,
    };
    let exports = ExportDirectory::read(&image, m.exports).unwrap().unwrap();
    let (_, ExportTarget::Rva(rva)) = exports
        .by_name(&image, b"NtContinue", None)
        .unwrap()
        .unwrap()
    else {
        panic!("NtContinue must be a direct installed export")
    };
    let entry = m.base + u64::from(rva);
    let found: Vec<_> = (0..8)
        .step_by(4)
        .filter_map(|offset| {
            let raw = p.space.u32(entry + offset).unwrap();
            (raw & 0xffe0001f == 0xd4000001).then_some((entry + offset, (raw >> 5) & 0xffff))
        })
        .collect();
    assert_eq!(found.len(), 1);
    println!(
        "continue-leaf entry={entry:#x} svc-pc={:#x} service={:#x}",
        found[0].0, found[0].1
    );
    found[0].1
}

fn main() {
    for path in std::env::args().skip(1) {
        let mut config =
            WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        config.arena_bytes = 268435456;
        config.slice_insns = 1;
        let console = match &config.console {
            rax::user::console::Console::Captured(value) => value.clone(),
            _ => unreachable!(),
        };
        let mut process =
            WindowsProcess::spawn_image(config, std::fs::read(&path).unwrap()).unwrap();
        let cancelled = AtomicBool::new(false);
        let prefix = 2_100_000;
        let first = process.run_slice(prefix, &cancelled);
        println!("program={path} prefix-turns={prefix} result={first:?}");
        let mut terminal = matches!(first, RunStatus::Complete(_));
        let mut reached = 0;
        let mut continued = 0;
        let service = continue_service(process.state());
        if !terminal {
            for turn in prefix..10_000_000 {
                let before: Vec<_> = process
                    .state()
                    .threads
                    .values()
                    .filter_map(|thread| {
                        let raw = process.state().space.u32(thread.cpu.pc()).ok()?;
                        if raw & 0xffe0001f != 0xd4000001 || (raw >> 5) & 0xffff != 0x1d3 {
                            return None;
                        }
                        println!(
                            "alert-call turn={turn} tid={} pc={:#x} user-alerted={} queued={}",
                            thread.tid,
                            thread.cpu.pc(),
                            thread.user_alerted,
                            thread.apcs.len()
                        );
                        Some(thread.tid)
                    })
                    .collect();
                reached += before.len();
                let contexts:Vec<_>=process.state().threads.values().filter_map(|thread|{
                    let raw=process.state().space.u32(thread.cpu.pc()).ok()?;
                    if raw&0xffe0001f!=0xd4000001 || (raw>>5)&0xffff!=service{return None;}
                    let context=RegContext::read(&process.state().space,process.state().arch,thread.cpu.gpr(0)).unwrap();
                    println!("continue-call turn={turn} tid={} test={} context={:#x} flags={:#x} target-pc={:#x} target-sp={:#x} target-gpr0={:#x} target-gpr3={:#x} validation={:?} alerted={} queued={}",thread.tid,thread.cpu.gpr(1)&255,thread.cpu.gpr(0),context.flags(),context.pc(),context.sp(),context.gpr(0),context.gpr(3),context.validate(&thread.cpu),thread.user_alerted,thread.apcs.len());
                    Some(thread.tid)
                }).collect();
                continued += contexts.len();
                let result = process.run_slice(1, &cancelled);
                for tid in contexts {
                    if let Some(thread) = process.state().threads.get(&tid) {
                        println!(
                            "continue-result turn={turn} tid={tid} pc={:#x} sp={:#x} gpr0={:#x} gpr3={:#x} alerted={} queued={} frames={} wait-status={:?}",
                            thread.cpu.pc(),
                            thread.cpu.sp(),
                            thread.cpu.gpr(0),
                            thread.cpu.gpr(3),
                            thread.user_alerted,
                            thread.apcs.len(),
                            thread.frames.len(),
                            thread.wait_status
                        );
                    }
                }
                for tid in before {
                    if let Some(thread) = process.state().threads.get(&tid) {
                        println!(
                            "alert-result turn={turn} tid={tid} status={:#x} pc={:#x} user-alerted={} queued={} frames={}",
                            thread.cpu.gpr(0),
                            thread.cpu.pc(),
                            thread.user_alerted,
                            thread.apcs.len(),
                            thread.frames.len()
                        );
                    }
                }
                if matches!(result, RunStatus::Complete(_)) {
                    println!("terminal turn={turn} {result:?}");
                    terminal = true;
                    break;
                }
                if turn % 1_000_000 == 0 {
                    println!("progress turn={turn} result={result:?}");
                }
            }
        }
        let state = process.state();
        for thread in state.threads.values() {
            if thread.cpu.gpr(1) == 1 {
                if let Ok(context) = rax::user::windows::context::RegContext::read(
                    &state.space,
                    state.arch,
                    thread.cpu.gpr(0),
                ) {
                    println!(
                        "continued-context flags={:#x} pc={:#x} sp={:#x} validation={:?}",
                        context.flags(),
                        context.pc(),
                        context.sp(),
                        context.validate(&thread.cpu)
                    );
                }
            }
            let last = state
                .space
                .u32(thread.teb + layout::offsets(state.arch).teb_last_error);
            println!(
                "final-thread tid={} pc={:#x} sp={:#x} args={:x?} bytes={:x?} last-error={last:x?}",
                thread.tid,
                thread.cpu.pc(),
                thread.cpu.sp(),
                (0..8).map(|i| thread.cpu.gpr(i)).collect::<Vec<_>>(),
                state.space.bytes(thread.cpu.pc(), 16)
            );
        }
        for (name, stream) in [
            ("stdout", rax::user::console::OutputStream::Stdout),
            ("stderr", rax::user::console::OutputStream::Stderr),
        ] {
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0u8; 4096];
                let count = console.drain(stream, &mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            println!(
                "guest-console program={path} stream={name} bytes={} hex={hex}",
                bytes.len()
            );
        }
        println!(
            "observation-finished program={path} turn-limit=10000000 reached-alert={reached} reached-continue={continued} terminal={terminal}"
        );
    }
}
