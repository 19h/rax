//! Linux-kernel differential campaign driver.
//!
//! `generate <dir>` writes `<dir>/input.bin` for `kernel/harness.c`.
//! `compare <dir>` regenerates the identical programs, reads the harness's
//! `<dir>/output.bin`, executes every program on every engine plane, and
//! fails on any difference from the kernel.
//! `export <dir> <corpus> --kernel <description>` compares as above and then
//! writes one digest line per kernel program to `<corpus>`, the file the
//! engine `ebpf_diff` target replays.
//!
//! Options: `--endian le|be` (default `le`), `--scale N` random operand
//! pairs per form in addition to the boundary cross product (default 400).

use std::process::ExitCode;
use std::time::Instant;

use rax::smir::ir::types::Endian;
use rax_ebpf_conformance::{
    Form, KernelResult, Program, describe_case, forms, harness_input, kernel, parse_harness_output,
    program_digest,
};

struct Options {
    command: String,
    dir: std::path::PathBuf,
    corpus: Option<std::path::PathBuf>,
    kernel: Option<String>,
    endian: Endian,
    scale: usize,
}

fn parse() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or("missing command (generate|compare|export)")?;
    let dir = args.next().ok_or("missing directory")?.into();
    let corpus = if command == "export" {
        Some(args.next().ok_or("missing corpus path")?.into())
    } else {
        None
    };
    let mut options = Options {
        command,
        dir,
        corpus,
        kernel: None,
        endian: Endian::Little,
        scale: 400,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--endian" => {
                options.endian = match args.next().as_deref() {
                    Some("le") => Endian::Little,
                    Some("be") => Endian::Big,
                    other => return Err(format!("invalid --endian {other:?}")),
                }
            }
            "--scale" => {
                options.scale = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("invalid --scale")?;
            }
            "--kernel" => options.kernel = Some(args.next().ok_or("missing --kernel value")?),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(options)
}

fn build(options: &Options) -> (Vec<Form>, Vec<Program>) {
    let forms = forms(options.scale, options.endian);
    let programs = forms
        .iter()
        .flat_map(|form| form.programs(options.endian))
        .collect();
    (forms, programs)
}

fn read_results(options: &Options, programs: &[Program]) -> Result<Vec<KernelResult>, String> {
    let output = std::fs::read(options.dir.join("output.bin"))
        .map_err(|error| format!("cannot read harness output: {error}"))?;
    let results = parse_harness_output(&output)?;
    if results.len() != programs.len() {
        return Err(format!(
            "harness reported {} programs, expected {}",
            results.len(),
            programs.len()
        ));
    }
    Ok(results)
}

fn compare(options: &Options, forms: &[Form], programs: &[Program]) -> Result<(), String> {
    let results = read_results(options, programs)?;
    let started = Instant::now();
    let summary = kernel::compare(programs, &results, options.endian);
    for mismatch in summary.mismatches.iter().take(60) {
        let context = mismatch
            .case
            .map(|index| describe_case(forms, &mismatch.form, index, options.endian))
            .unwrap_or_default();
        println!(
            "MISMATCH {} {} case={:?}: {}\n    {}",
            mismatch.engine, mismatch.form, mismatch.case, mismatch.detail, context
        );
    }
    println!(
        "{:?}: {} forms, {} programs ({} rejected by the kernel), {} kernel-verified cases, {} engine program runs, {} mismatches, {:.1} s",
        options.endian,
        summary.per_form.len(),
        summary.programs,
        summary.rejected_programs,
        summary.cases,
        summary.engine_runs,
        summary.mismatches.len(),
        started.elapsed().as_secs_f64()
    );
    if summary.mismatches.is_empty() && summary.rejected_programs == 0 {
        Ok(())
    } else {
        Err("kernel differential failed".to_string())
    }
}

fn export(options: &Options, programs: &[Program]) -> Result<(), String> {
    let results = read_results(options, programs)?;
    let kernel = options
        .kernel
        .as_deref()
        .ok_or("export requires --kernel <description>")?;
    let endian = match options.endian {
        Endian::Little => "le",
        Endian::Big => "be",
    };
    let mut text = format!(
        "# Linux kernel BPF_PROG_TEST_RUN results for the RFC 9669 campaign of\n\
         # tests/support/ebpf_rfc_oracle.rs (scale {}, {endian}).\n\
         # Kernel: {kernel}\n\
         # Generator: python3 tools/ebpf-conformance/run.py kernel-{endian} --export\n\
         # Columns: program form cases retval digest(retval, context)\n",
        options.scale
    );
    for (index, (program, result)) in programs.iter().zip(&results).enumerate() {
        if result.load_status != 0 || result.run_status != 0 {
            return Err(format!(
                "kernel rejected program {index} ({})",
                program.form
            ));
        }
        text.push_str(&format!(
            "{index} {} {} {:08x} {}\n",
            program.form,
            program.cases.len(),
            result.retval,
            program_digest(result.retval, &result.ctx).hex()
        ));
    }
    let path = options.corpus.as_ref().expect("export corpus path");
    std::fs::write(path, text).map_err(|error| format!("cannot write corpus: {error}"))?;
    println!("wrote {} digests to {}", programs.len(), path.display());
    Ok(())
}

fn main() -> ExitCode {
    let options = match parse() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let (forms, programs) = build(&options);
    let cases: usize = forms.iter().map(Form::case_count).sum();
    let result = match options.command.as_str() {
        "generate" => {
            let input = harness_input(&programs, options.endian);
            std::fs::write(options.dir.join("input.bin"), &input)
                .map_err(|error| format!("cannot write input: {error}"))
                .map(|()| {
                    println!(
                        "generated {} forms, {} cases, {} programs, {} bytes ({:?})",
                        forms.len(),
                        cases,
                        programs.len(),
                        input.len(),
                        options.endian
                    );
                })
        }
        "compare" => compare(&options, &forms, &programs),
        "export" => compare(&options, &forms, &programs).and_then(|()| export(&options, &programs)),
        other => Err(format!("unknown command `{other}`")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
