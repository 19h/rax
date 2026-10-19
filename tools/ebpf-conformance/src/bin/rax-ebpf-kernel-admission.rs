//! Linux-kernel verifier admission of RFC 9669 encoding partitions compared
//! with the rax RFC 9669 decoder.
//!
//! `generate <dir> [--endian le|be]` writes a load-only harness input with
//! every candidate of `admission_candidates` (shared with the engine replay)
//! after a prologue that initializes `r0..r9` and eight stack slots.
//! `compare <dir>` classifies each kernel verdict by the verifier's final log
//! line and compares it with `decode_insn`; `export <dir> <corpus> --kernel
//! <description>` also records the result for `ebpf_diff`.
//!
//! A rejection is syntactic when the verifier reports an encoding error
//! (reserved fields, unknown opcode, invalid register or immediate); any
//! other rejection is a program-safety verdict that a decoder does not make.
//! Disagreements are classified: RFC-defined encodings the verifier refuses
//! by policy, platform objects the load did not bind, and Linux extensions
//! that postdate RFC 9669.

use std::collections::BTreeMap;
use std::process::ExitCode;

use rax::smir::ir::types::Endian;
use rax_ebpf_conformance::{
    Insn, admission_candidates, admission_digest, admission_program, admission_verdict,
    parse_harness_output,
};

fn input(endian: Endian) -> Vec<u8> {
    let candidates = admission_candidates();
    let mut out = b"RAXBPFLD".to_vec();
    out.extend_from_slice(&(candidates.len() as u32).to_le_bytes());
    for candidate in &candidates {
        let insns = admission_program(candidate);
        out.extend_from_slice(&(insns.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for insn in insns {
            out.extend_from_slice(&insn.encode(endian));
        }
    }
    out
}

/// The verifier's error line with numbers normalized to `#`.
fn verdict_line(log: &str) -> String {
    let line = log
        .lines()
        .rev()
        .find(|line| !line.starts_with("processed ") && !line.trim().is_empty())
        .unwrap_or("");
    let mut normalized = String::new();
    let mut previous_digit = false;
    for c in line.chars() {
        if c.is_ascii_digit() {
            if !previous_digit {
                normalized.push('#');
            }
            previous_digit = true;
        } else {
            normalized.push(c);
            previous_digit = false;
        }
    }
    normalized
}

/// Verifier diagnostics that reject an encoding rather than a program.
fn syntactic(message: &str) -> bool {
    let encoding_errors = [
        "unknown opcode",
        "uses reserved fields",
        "invalid BPF_LD_IMM",
        "invalid bpf_ld_imm",
        "unrecognized bpf_ld_imm",
        "invalid atomic",
        "uses invalid atomic opcode",
        "is invalid",
        "invalid shift",
        "div by zero",
        "invalid mode",
        "invalid src_reg",
        "invalid dst_reg",
    ];
    encoding_errors
        .iter()
        .any(|pattern| message.contains(pattern))
}

/// Why a disagreement is expected, or `None` for an unexplained one.
fn explanation(
    candidate: &[Insn],
    rax: Option<&str>,
    kernel: &str,
    message: &str,
) -> Option<&'static str> {
    let insn = candidate[0];
    match (rax, kernel) {
        // The verifier refuses encodings whose results RFC 9669 defines.
        (None, "syntax") if message.starts_with("div by zero") => Some(
            "RFC 9669 4.1 defines division by zero; the verifier rejects an immediate zero divisor by policy",
        ),
        (None, "syntax") if message.starts_with("invalid shift") => Some(
            "RFC 9669 4.1 masks shift counts; the verifier rejects out-of-range immediates by policy",
        ),
        // map_by_idx needs the program's map set, a platform binding.
        (None, "syntax") if message.starts_with("fd_idx without fd_array") => Some(
            "RFC 9669 5.4.1 map_by_idx indexes the program's map set; none was bound at load (rax: UnsupportedPseudoLoad when executed unbound)",
        ),
        // may_goto (BPF_JMP | BPF_JCOND, Linux 6.9) postdates RFC 9669.
        (Some(_), "accepted") if insn.opc == 0xe5 => {
            Some("Linux may_goto (BPF_JCOND) is not part of RFC 9669")
        }
        // Atomic load-acquire / store-release (Linux 6.15) postdate RFC 9669.
        (Some(_), "accepted") if insn.opc & 0xe7 == 0xc3 && matches!(insn.imm, 0x100 | 0x110) => {
            Some("Linux load-acquire/store-release atomics are not part of RFC 9669")
        }
        _ => None,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(command), Some(dir)) = (args.first(), args.get(1)) else {
        eprintln!(
            "usage: rax-ebpf-kernel-admission generate|compare|export <dir> [<corpus>] [--endian le|be] [--kernel <text>]"
        );
        return ExitCode::FAILURE;
    };
    let option = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let endian = match option("--endian").as_deref() {
        Some("be") => Endian::Big,
        _ => Endian::Little,
    };
    let dir = std::path::Path::new(dir);
    if command == "generate" {
        let bytes = input(endian);
        if let Err(error) = std::fs::write(dir.join("input.bin"), &bytes) {
            eprintln!("cannot write input: {error}");
            return ExitCode::FAILURE;
        }
        println!(
            "generated {} admission programs ({} bytes)",
            admission_candidates().len(),
            bytes.len()
        );
        return ExitCode::SUCCESS;
    }

    let output = match std::fs::read(dir.join("output.bin")) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("cannot read harness output: {error}");
            return ExitCode::FAILURE;
        }
    };
    let results = match parse_harness_output(&output) {
        Ok(results) => results,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let candidates = admission_candidates();
    if results.len() != candidates.len() {
        eprintln!(
            "harness reported {} programs, expected {}",
            results.len(),
            candidates.len()
        );
        return ExitCode::FAILURE;
    }

    let mut table: BTreeMap<(String, String, String), (usize, String)> = BTreeMap::new();
    let mut disagreements = Vec::new();
    let mut unexplained = 0usize;
    for (candidate, result) in candidates.iter().zip(&results) {
        let rax = admission_verdict(candidate, endian);
        let (kernel, message) = if result.load_status == 0 {
            ("accepted", String::new())
        } else {
            let message = verdict_line(&result.log);
            let class = if syntactic(&message) {
                "syntax"
            } else {
                "semantic"
            };
            (class, message)
        };
        let encoding: String = candidate
            .iter()
            .flat_map(|insn| insn.encode(endian))
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let rax_text = rax.clone().unwrap_or_else(|| "decodes".to_string());
        let entry = table
            .entry((rax_text.clone(), kernel.to_string(), message.clone()))
            .or_insert((0, encoding.clone()));
        entry.0 += 1;
        let disagrees =
            (rax.is_some() && kernel == "accepted") || (rax.is_none() && kernel == "syntax");
        if disagrees {
            let reason = explanation(candidate, rax.as_deref(), kernel, &message);
            if reason.is_none() {
                unexplained += 1;
            }
            disagreements.push(format!(
                "disagree {encoding} rax={rax_text} kernel={kernel} [{message}] {}",
                reason.unwrap_or("UNEXPLAINED")
            ));
        }
    }

    let mut report = String::new();
    for ((rax, kernel, message), (count, example)) in &table {
        report.push_str(&format!(
            "row {count} rax={rax} kernel={kernel} [{message}] e.g. {example}\n"
        ));
    }
    print!("{report}");
    for line in &disagreements {
        println!("{line}");
    }
    println!(
        "{:?}: {} encodings, {} disagreements, {unexplained} unexplained",
        endian,
        candidates.len(),
        disagreements.len()
    );

    if command == "export" {
        let (Some(corpus), Some(kernel)) = (args.get(2), option("--kernel")) else {
            eprintln!("export requires <corpus> and --kernel <description>");
            return ExitCode::FAILURE;
        };
        let (encodings, digest) = admission_digest(endian);
        let endian_name = if endian == Endian::Big { "be" } else { "le" };
        let text = format!(
            "# Linux kernel verifier admission of the RFC 9669 encoding partitions of\n\
             # tests/support/ebpf_rfc_oracle.rs (admission_candidates, {endian_name}),\n\
             # compared with the rax RFC 9669 decoder.\n\
             # Kernel: {kernel}\n\
             # Generator: python3 tools/ebpf-conformance/run.py admission-{endian_name} --export\n\
             # `decoder` pins the digest of the rax verdicts the campaign compared;\n\
             # `row` is one (rax verdict, kernel class, verifier message) class;\n\
             # `disagree` lists each encoding where the two differ, with its reason.\n\
             encodings {encodings}\n\
             decoder {}\n\
             {report}{}\n",
            digest.hex(),
            disagreements.join("\n")
        );
        if let Err(error) = std::fs::write(corpus, text) {
            eprintln!("cannot write corpus: {error}");
            return ExitCode::FAILURE;
        }
        println!("wrote {corpus}");
    }
    if unexplained == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
