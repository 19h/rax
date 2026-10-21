use rax::user::windows::{
    WindowsConfig, WindowsProcess, context::ExceptionRecord, memory::Mem, process::RunStatus,
};
use std::sync::atomic::AtomicBool;

fn main() {
    let program = std::env::args().nth(1).unwrap();
    let mut config = WindowsConfig::embedded("C:\\app\\probe.exe", vec![], vec![], 4096).unwrap();
    config.native_libraries = true;
    config.arena_bytes = 268435456;
    config.slice_insns = 64;
    let mut process = WindowsProcess::spawn_image(config, std::fs::read(program).unwrap()).unwrap();
    let cancel = AtomicBool::new(false);
    let mut recent = std::collections::VecDeque::new();
    let mut terminal = false;
    for turn in 0..2_000_000 {
        let p = process.state();
        let states: Vec<_> = p
            .threads
            .values()
            .map(|t| {
                (
                    t.tid,
                    t.cpu.pc(),
                    t.cpu.sp(),
                    t.cpu.gpr(0),
                    t.cpu.gpr(1),
                    t.cpu.gpr(30),
                )
            })
            .collect();
        recent.push_back((turn, states));
        if recent.len() > 32 {
            recent.pop_front();
        }
        let result = process.run_slice(1, &cancel);
        if matches!(result, RunStatus::Complete(_)) {
            println!("terminal turn={turn} result={result:?}");
            terminal = true;
            break;
        }
    }
    assert!(terminal, "diagnostic observation budget exhausted");
    for row in recent {
        println!("recent={row:X?}");
    }
    let p = process.state();
    for allocation in p.vm.allocations() {
        if format!("{:?}", allocation.kind) == "Image"
            || (allocation.base..allocation.base + allocation.size).contains(&0x37349F8)
        {
            println!(
                "allocation base={:X} size={:X} kind={:?} protection={:X} name={:?}",
                allocation.base,
                allocation.size,
                allocation.kind,
                allocation.protect,
                allocation.name
            );
        }
    }
    for (name, address) in [("failing-code", 0x37349F8), ("near-code", 0x37349D0)] {
        println!("bytes {name}={:X?}", p.space.bytes(address, 64));
        println!("allocation {name}={:?}", p.vm.allocation(address));
        println!(
            "module {name}={:?}",
            p.modules
                .by_address(address)
                .map(|(_, m)| (&m.name, m.base, m.size))
        );
        println!(
            "unwind {name}={:?}",
            rax::user::windows::seh::unwind::lookup(p, address)
        );
    }
    for t in p.threads.values() {
        println!(
            "thread={} pc={:X} sp={:X} lr={:X} frames={}",
            t.tid,
            t.cpu.pc(),
            t.cpu.sp(),
            t.cpu.gpr(30),
            t.frames.len()
        );
        println!(
            "original-pc bytes at={:X} data={:X?}",
            t.cpu.pc().saturating_sub(16),
            p.space.bytes(t.cpu.pc().saturating_sub(16), 64)
        );
        println!("original-pc allocation={:?}", p.vm.allocation(t.cpu.pc()));
        let regs: Vec<_> = (0..31).map(|n| t.cpu.gpr(n)).collect();
        println!("registers={regs:X?}");
        for f in &t.frames {
            println!(
                "frame api={} entry={:X} sp={:X} return={:X} cursor={:X} exception-caller={:?}",
                f.api.name, f.entry_pc, f.entry_sp, f.ret_addr, f.cursor, f.exception_caller
            );
            for e in &f.exception {
                println!("scope code={:X?} cursor={:X}", e.code, e.cursor);
            }
        }
        let start = t.cpu.sp().saturating_sub(8192).max(t.stack_limit);
        let end = t.cpu.sp().saturating_add(8192).min(t.stack_base);
        for address in (start..end).step_by(8) {
            if matches!(
                p.space.u32(address),
                Ok(0xC0000005 | 0xC000001D | 0x80000003 | 0xC0000006)
            ) {
                if let Ok(record) = ExceptionRecord::read(&p.space, p.arch, address) {
                    println!("possible-record address={address:X} record={record:?}");
                }
            }
        }
    }
}
