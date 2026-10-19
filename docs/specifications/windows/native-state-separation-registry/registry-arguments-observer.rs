use rax::user::windows::{WindowsConfig, WindowsProcess, layout, memory::Mem, process::RunStatus};
use std::sync::atomic::AtomicBool;

const SEMAPHORE_SERVICES: &[(u16, &str)] = &[
    (0xc7, "NtCreateSemaphore"),
    (0x136, "NtOpenSemaphore"),
    (0xa, "NtReleaseSemaphore"),
    (0x16a, "NtQuerySemaphore"),
];

fn main() {
    for path in std::env::args().skip(1) {
        let mut cfg = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
        cfg.native_libraries = true;
        cfg.arena_bytes = 268435456;
        cfg.slice_insns = 1;
        let mut process = WindowsProcess::spawn_image(cfg, std::fs::read(&path).unwrap()).unwrap();
        let cancelled = AtomicBool::new(false);
        let skipped = 2_100_000;
        let result = process.run_slice(skipped, &cancelled);
        println!("program={path} prefix-turns={skipped} result={result:?}");
        if matches!(result, RunStatus::Complete(_)) {
            continue;
        }
        for turn in skipped..3_000_000 {
            let mut mutant_outputs = Vec::new();
            let mut wait_outputs = Vec::new();
            let mut semaphore_outputs = Vec::new();
            let p = process.state();
            let o = layout::offsets(p.arch);
            for t in p.threads.values() {
                if let Ok(bytes) = p.space.bytes(t.cpu.pc(), 4) {
                    let raw = u32::from_le_bytes(bytes.try_into().unwrap());
                    if raw & 0xffe0001f == 0xd4000001 && (raw >> 5) & 0xffff == 0x12 {
                        let attrs = t.cpu.gpr(2);
                        let root = p.space.ptr(attrs + o.ptr, o.ptr);
                        let descriptor = p.space.ptr(attrs + 2 * o.ptr, o.ptr);
                        println!(
                            "registry-call turn={turn} tid={} desired={:#x} output={:#x} attributes={attrs:#x} root={root:x?} flags={:x?}",
                            t.tid,
                            t.cpu.gpr(1),
                            t.cpu.gpr(0),
                            p.space.u32(attrs + 3 * o.ptr)
                        );
                        if let Ok(descriptor) = descriptor {
                            if let (Ok(length), Ok(buffer)) = (
                                p.space.u16(descriptor),
                                p.space.ptr(descriptor + o.ptr, o.ptr),
                            ) {
                                if let Ok(bytes) = p.space.bytes(buffer, usize::from(length)) {
                                    let units: Vec<_> = bytes
                                        .chunks_exact(2)
                                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                                        .collect();
                                    println!(
                                        "registry-name turn={turn} length-bytes={length} name={:?} raw={units:x?}",
                                        String::from_utf16_lossy(&units)
                                    );
                                }
                            }
                        }
                        if let Ok(root) = root {
                            println!("registry-root turn={turn} object={:?}", p.objects.get(root));
                        }
                    }
                    if raw & 0xffe0001f == 0xd4000001 {
                        let svc = ((raw >> 5) & 0xffff) as u16;
                        if let Some((_, name)) = SEMAPHORE_SERVICES.iter().find(|row| row.0 == svc)
                        {
                            println!(
                                "semaphore-call turn={turn} tid={} name={name} svc={svc:#x} pc={:#x} args={:x?}",
                                t.tid,
                                t.cpu.pc(),
                                [
                                    t.cpu.gpr(0),
                                    t.cpu.gpr(1),
                                    t.cpu.gpr(2),
                                    t.cpu.gpr(3),
                                    t.cpu.gpr(4)
                                ]
                            );
                            let creating =
                                *name == "NtCreateSemaphore" || *name == "NtOpenSemaphore";
                            semaphore_outputs.push((t.tid, *name, t.cpu.gpr(0), creating));
                        }
                    }
                    if raw & 0xffe0001f == 0xd4000001 && (raw >> 5) & 0xffff == 4 {
                        wait_outputs.push((t.tid, t.cpu.gpr(0)));
                        println!(
                            "wait-call turn={turn} tid={} handle={:#x} alertable={:#x} timeout={:#x} timeout-value={:x?} object={:?}",
                            t.tid,
                            t.cpu.gpr(0),
                            t.cpu.gpr(1),
                            t.cpu.gpr(2),
                            p.space.u64(t.cpu.gpr(2)),
                            p.objects.get(t.cpu.gpr(0))
                        );
                    }
                    if raw & 0xffe0001f == 0xd4000001 && (raw >> 5) & 0xffff == 0xba {
                        println!(
                            "mutant-call turn={turn} tid={} output={:#x} access={:#x} attributes={:#x} owner={:#x}",
                            t.tid,
                            t.cpu.gpr(0),
                            t.cpu.gpr(1),
                            t.cpu.gpr(2),
                            t.cpu.gpr(3)
                        );
                        mutant_outputs.push((t.tid, t.cpu.gpr(0)));
                    }
                    if raw & 0xffe0001f == 0xd4000001 && (raw >> 5) & 0xffff == 0x58 {
                        let attrs = t.cpu.gpr(2);
                        let (root_offset, name_offset, flag_offset, buffer_offset) = if o.ptr == 8 {
                            (8, 16, 24, 8)
                        } else {
                            (4, 8, 12, 4)
                        };
                        println!(
                            "directory turn={turn} access={:#x} root={:x?} flags={:x?}",
                            t.cpu.gpr(1),
                            p.space.ptr(attrs + root_offset, o.ptr),
                            p.space.u32(attrs + flag_offset)
                        );
                        if let Ok(name) = p.space.ptr(attrs + name_offset, o.ptr) {
                            if let (Ok(length), Ok(buffer)) =
                                (p.space.u16(name), p.space.ptr(name + buffer_offset, o.ptr))
                            {
                                let bytes = p.space.bytes(buffer, usize::from(length)).unwrap();
                                let units: Vec<_> = bytes
                                    .chunks_exact(2)
                                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                                    .collect();
                                println!(
                                    "directory name={:?} length={length} raw={units:x?}",
                                    String::from_utf16_lossy(&units)
                                );
                            }
                        }
                    }
                }
            }
            let result = process.run_slice(1, &cancelled);
            for (tid, name, argument, output_pointer) in semaphore_outputs {
                let p = process.state();
                let returned = p.threads.get(&tid).map(|t| t.cpu.gpr(0));
                let value = if output_pointer {
                    p.space.ptr(argument, layout::offsets(p.arch).ptr).ok()
                } else {
                    Some(argument)
                };
                println!(
                    "semaphore-result turn={turn} tid={tid} name={name} result={result:?} return-register={returned:x?} output-value={value:x?} object={:?}",
                    value.and_then(|h| p.objects.get(h))
                );
            }
            for (tid, handle) in wait_outputs {
                let p = process.state();
                println!(
                    "wait-result turn={turn} tid={tid} status={:x?} state={:?} object={:?}",
                    p.threads.get(&tid).map(|t| t.cpu.gpr(0)),
                    p.threads.get(&tid).map(|t| &t.state),
                    p.objects.get(handle)
                );
            }
            for (tid, output) in mutant_outputs {
                let p = process.state();
                let status = p.threads.get(&tid).map(|t| t.cpu.gpr(0));
                let handle = p.space.ptr(output, layout::offsets(p.arch).ptr);
                let kind = handle
                    .as_ref()
                    .ok()
                    .and_then(|h| p.objects.get(*h))
                    .map(|object| object.type_name());
                println!(
                    "mutant-result turn={turn} tid={tid} status={status:x?} handle={handle:x?} kind={kind:?}"
                );
            }

            if matches!(result, RunStatus::Complete(_)) {
                println!("terminal turn={turn} {result:?}");
                break;
            }
        }
        println!("observation-finished program={path} turn-limit=3000000");
    }
}
