//! Comparison of the Linux-kernel campaign results with every engine plane.

use std::collections::BTreeMap;

use rax::smir::ir::types::Endian;

use crate::{
    Engine, KernelResult, Program, describe_field, initial_registers, no_helpers, run_rfc9669,
};

/// Mismatch between the kernel and one engine plane.
#[derive(Debug, Clone)]
pub struct Mismatch {
    pub engine: &'static str,
    pub form: String,
    pub case: Option<usize>,
    pub detail: String,
}

/// Campaign totals.
#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub programs: usize,
    pub cases: usize,
    pub engine_runs: usize,
    pub rejected_programs: usize,
    pub per_form: BTreeMap<String, usize>,
    pub mismatches: Vec<Mismatch>,
}

fn compare_program(program: &Program, kernel: &KernelResult, endian: Endian) -> Summary {
    let mut summary = Summary {
        programs: 1,
        ..Summary::default()
    };
    *summary.per_form.entry(program.form.clone()).or_default() += program.cases.len();
    if kernel.load_status != 0 || kernel.run_status != 0 {
        summary.rejected_programs = 1;
        summary.mismatches.push(Mismatch {
            engine: "kernel",
            form: program.form.clone(),
            case: None,
            detail: format!(
                "kernel load={} run={} log: {}",
                kernel.load_status, kernel.run_status, kernel.log
            ),
        });
        return summary;
    }
    summary.cases = program.cases.len();
    let bytes: Vec<u8> = program
        .insns
        .iter()
        .flat_map(|i| i.encode(endian))
        .collect();
    for engine in Engine::ALL {
        summary.engine_runs += 1;
        let result = run_rfc9669(
            engine,
            endian,
            &bytes,
            &program.ctx,
            initial_registers(program.ctx.len()),
            &no_helpers,
        );
        let r0 = match result.exit {
            Ok(r0) => r0,
            Err(error) => {
                summary.mismatches.push(Mismatch {
                    engine: engine.name(),
                    form: program.form.clone(),
                    case: None,
                    detail: format!("engine trapped: {error}"),
                });
                continue;
            }
        };
        if r0 as u32 != kernel.retval {
            summary.mismatches.push(Mismatch {
                engine: engine.name(),
                form: program.form.clone(),
                case: None,
                detail: format!("retval {:#x} vs kernel {:#x}", r0 as u32, kernel.retval),
            });
        }
        for layout in &program.cases {
            let range = layout.output_offset..layout.output_offset + layout.output_len;
            let ours = &result.input[range.clone()];
            let theirs = &kernel.ctx[range];
            if ours != theirs {
                let first = ours
                    .iter()
                    .zip(theirs)
                    .position(|(a, b)| a != b)
                    .expect("differing byte");
                let slot = first & !7;
                summary.mismatches.push(Mismatch {
                    engine: engine.name(),
                    form: layout.form.clone(),
                    case: Some(layout.index),
                    detail: format!(
                        "{} differs: ours {:02x?} kernel {:02x?}",
                        describe_field(first),
                        &ours[slot..slot + 8],
                        &theirs[slot..slot + 8]
                    ),
                });
            }
        }
    }
    summary
}

/// Run every program on every engine plane and compare it with the kernel.
/// Programs are independent, so the comparison is spread across host threads.
pub fn compare(programs: &[Program], results: &[KernelResult], endian: Endian) -> Summary {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let partials: Vec<Summary> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut local = Summary::default();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(program) = programs.get(index) else {
                            break;
                        };
                        let part = compare_program(program, &results[index], endian);
                        local.programs += part.programs;
                        local.cases += part.cases;
                        local.engine_runs += part.engine_runs;
                        local.rejected_programs += part.rejected_programs;
                        for (form, count) in part.per_form {
                            *local.per_form.entry(form).or_default() += count;
                        }
                        local.mismatches.extend(part.mismatches);
                    }
                    local
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("comparison worker"))
            .collect()
    });
    let mut summary = Summary::default();
    for part in partials {
        summary.programs += part.programs;
        summary.cases += part.cases;
        summary.engine_runs += part.engine_runs;
        summary.rejected_programs += part.rejected_programs;
        for (form, count) in part.per_form {
            *summary.per_form.entry(form).or_default() += count;
        }
        summary.mismatches.extend(part.mismatches);
    }
    summary
        .mismatches
        .sort_by(|a, b| (&a.form, a.case, a.engine).cmp(&(&b.form, b.case, b.engine)));
    summary
}
