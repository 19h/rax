//! `bpf_conformance` plugin for the RAX RFC 9669 engine planes.
//!
//! Protocol (eBPF Foundation `bpf_conformance`, `libbpf_plugin.cc`): the
//! program arrives as base-16 bytes on stdin or after `--program`; optional
//! base-16 input memory is the first positional argument. The plugin prints
//! the final `r0` in hexadecimal and exits with status 0, or prints a
//! diagnostic to stderr and exits with status 1.
//!
//! `--engine {direct,smir-o0,smir-o1,smir-o2}` selects the execution plane.
//! The default is `direct`. `r1` addresses the input memory, `r2` holds its
//! length in bytes, and `r10` addresses the top of a 4096-byte frame.

use std::io::BufRead;
use std::process::ExitCode;

use rax::smir::ir::types::Endian;
use rax_ebpf_conformance::{Engine, conformance_helpers, initial_registers, run_rfc9669};

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    text.split_whitespace()
        .map(|token| {
            let digits = token.strip_prefix("0x").unwrap_or(token);
            u8::from_str_radix(digits, 16).map_err(|_| format!("invalid hex byte `{token}`"))
        })
        .collect()
}

struct Options {
    program: Option<String>,
    memory: Option<String>,
    engine: Engine,
    debug: bool,
}

fn parse_options(mut args: Vec<String>) -> Result<Options, String> {
    let mut options = Options {
        program: None,
        memory: None,
        engine: Engine::Direct,
        debug: false,
    };
    if args.first().is_some_and(|arg| arg == "--program") {
        args.remove(0);
        if args.is_empty() {
            return Err("--program requires a value".to_string());
        }
        options.program = Some(args.remove(0));
    }
    if args.first().is_some_and(|arg| !arg.starts_with("--")) {
        options.memory = Some(args.remove(0));
    }
    let mut remaining = args.into_iter();
    while let Some(arg) = remaining.next() {
        match arg.as_str() {
            "--engine" => {
                let name = remaining
                    .next()
                    .ok_or_else(|| "--engine requires a value".to_string())?;
                options.engine =
                    Engine::parse(&name).ok_or_else(|| format!("unknown engine `{name}`"))?;
            }
            "--debug" => options.debug = true,
            "--elf" => return Err("ELF input is not supported".to_string()),
            "" => {}
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse_options(std::env::args().skip(1).collect()) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let program_text = match options.program {
        Some(text) => text,
        None => {
            let mut line = String::new();
            if let Err(error) = std::io::stdin().lock().read_line(&mut line) {
                eprintln!("failed to read program: {error}");
                return ExitCode::FAILURE;
            }
            line
        }
    };
    let decoded = decode_hex(&program_text).and_then(|program| {
        let memory = decode_hex(options.memory.as_deref().unwrap_or(""))?;
        Ok((program, memory))
    });
    let (program, memory) = match decoded {
        Ok(decoded) => decoded,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let result = run_rfc9669(
        options.engine,
        Endian::Little,
        &program,
        &memory,
        initial_registers(memory.len()),
        &conformance_helpers,
    );
    if options.debug {
        eprintln!(
            "engine={} instructions={} regs={:x?}",
            options.engine.name(),
            result.instructions,
            result.regs
        );
    }
    match result.exit {
        Ok(r0) => {
            println!("{r0:x}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
