//! Exhaustive SBPF campaigns. Run with `--nocapture` to see coverage counts.
//!
//! Live campaigns compare every streamed program and state on the pinned
//! Anza interpreter and all rax planes. The corpus test also digests the
//! upstream outcomes of the shared corpus streams and checks them against
//! `tests/generated/ebpf/anza_sbpf.txt`, which the engine `ebpf_diff` target
//! replays without the upstream package (`RAX_SBPF_CORPUS_OUT` rewrites it).

use std::collections::BTreeMap;

use rax::isa::ebpf::{EbpfDialect, SbpfVersion, decode_insn};
use rax::smir::ir::types::Endian;
use solana_sbpf::program::SBPFVersion;

use crate::exhaustive::{
    CHARGE_KEY, Case, Digest, EXIT, Fixture, Rejection, Rng, SYSCALL_KEY, State, Trap,
    boundary_cases, compare, corpus_campaigns, encode, is_endian, random_cases, random_program,
    single_instruction_cases, single_instruction_program, verified_opcodes, versions,
};

#[derive(Default)]
struct Tally {
    programs: usize,
    rejected: usize,
    executions: usize,
    outcomes: BTreeMap<String, usize>,
}

impl Tally {
    fn record(&mut self, result: &Result<(u64, [u64; 11]), (Trap, u64)>) {
        let key = match result {
            Ok(_) => "exit".to_string(),
            Err((Trap::Memory { write, .. }, _)) => {
                format!("memory-{}", if *write { "store" } else { "load" })
            }
            Err((trap, _)) => format!("{trap:?}"),
        };
        *self.outcomes.entry(key).or_default() += 1;
        self.executions += 1;
    }
}

type Failures = BTreeMap<String, (usize, String)>;

fn note(failures: &mut Failures, (signature, detail): (String, String)) {
    let entry = failures.entry(signature).or_insert((0, detail));
    entry.0 += 1;
}

fn run_parallel<F>(job: F) -> Vec<(SbpfVersion, Tally, Failures)>
where
    F: Fn(SbpfVersion, SBPFVersion) -> (Tally, Failures) + Sync,
{
    std::thread::scope(|scope| {
        let workers: Vec<_> = versions()
            .into_iter()
            .map(|(version, upstream)| {
                let job = &job;
                scope.spawn(move || {
                    let (tally, failures) = job(version, upstream);
                    (version, tally, failures)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("campaign worker"))
            .collect()
    })
}

fn report(name: &str, results: &[(SbpfVersion, Tally, Failures)]) {
    let mut total = 0;
    for (version, tally, failures) in results {
        eprintln!(
            "{name} {version:?}: {} verified programs, {} rejected by the upstream verifier, {} executions x 5 implementations, outcomes {:?}",
            tally.programs, tally.rejected, tally.executions, tally.outcomes
        );
        assert!(tally.executions > 0, "{name} {version:?} executed nothing");
        for (signature, (count, detail)) in failures {
            eprintln!("MISMATCH x{count} [{signature}]\n{detail}");
            total += count;
        }
    }
    assert_eq!(total, 0, "{total} mismatches");
}

/// Run a case stream on every implementation. Streams are admitted with
/// `verify_sbpf`; upstream must admit every streamed program as well.
fn run_cases(
    cases: &[Case],
    upstream: SBPFVersion,
    tally: &mut Tally,
    failures: &mut Failures,
    digest: &mut Digest,
) {
    for case in cases {
        let fixture = match Fixture::new(case.version, upstream, &case.program, &case.functions) {
            Ok(fixture) => fixture,
            Err(Rejection::Upstream) => {
                note(
                    failures,
                    (
                        format!("{:?} admission", case.version),
                        format!(
                            "verify_sbpf admitted a program upstream rejects: {:02x?}",
                            case.program
                        ),
                    ),
                );
                continue;
            }
            Err(Rejection::Lift(error)) => {
                note(failures, (format!("{:?} lift", case.version), error));
                continue;
            }
        };
        tally.programs += 1;
        for state in &case.states {
            match compare(&fixture, state) {
                Ok(outcome) => {
                    digest.update(&outcome.bytes());
                    tally.record(&outcome.result);
                }
                Err(failure) => note(failures, failure),
            }
        }
    }
}

#[test]
fn the_shared_syscall_key_is_the_upstream_symbol_hash() {
    assert_eq!(SYSCALL_KEY, solana_sbpf::ebpf::hash_symbol_name(b"rax_mix"));
    assert_eq!(
        CHARGE_KEY,
        solana_sbpf::ebpf::hash_symbol_name(b"rax_charge")
    );
}

#[test]
fn verified_opcode_sets_match_the_rax_decoder() {
    for (version, upstream) in versions() {
        let opcodes = verified_opcodes(version);
        // The admitted opcode sets agree when filtered by the upstream verifier.
        let upstream_opcodes: Vec<u8> = (0..=255u8)
            .filter(|opc| !matches!(opc, 0x85 | 0x8d | 0x95))
            .filter(|&opc| {
                let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ u64::from(opc));
                (0..200).any(|_| {
                    let program = single_instruction_program(&mut rng, version, opc);
                    !matches!(
                        Fixture::new(version, upstream, &program, &[]),
                        Err(Rejection::Upstream)
                    )
                })
            })
            .collect();
        assert_eq!(opcodes, upstream_opcodes, "{version:?}");
        let dialect = EbpfDialect::Solana(version);
        for opc in 0..=255u8 {
            if matches!(opc, 0x85 | 0x8d | 0x95) {
                continue;
            }
            // A canonical form: r1, r2, zero offset, an immediate valid for
            // shifts, divisors, and byte-order widths.
            let imm = if is_endian(opc) { 16 } else { 1 };
            let mut bytes = encode(opc, 1, 2, 0, imm).to_vec();
            bytes.extend_from_slice(&[0; 8]);
            let decodes = decode_insn(&bytes, dialect, Endian::Little).is_ok();
            assert_eq!(
                decodes,
                opcodes.contains(&opc),
                "{version:?} opcode {opc:#04x}: rax decode={decodes}"
            );
        }
        eprintln!(
            "{version:?}: {} opcodes verified upstream; the rax decoder accepts exactly this set",
            opcodes.len()
        );
    }
}

#[test]
fn every_verified_opcode_matches_anza_execution() {
    let results = run_parallel(|version, upstream| {
        let mut tally = Tally::default();
        let mut failures = Failures::new();
        let cases = single_instruction_cases(version, 256, 6);
        run_cases(
            &cases,
            upstream,
            &mut tally,
            &mut failures,
            &mut Digest::default(),
        );
        (tally, failures)
    });
    report("single-instruction", &results);
}

#[test]
fn random_programs_match_anza_execution() {
    let results = run_parallel(|version, upstream| {
        let mut tally = Tally::default();
        let mut failures = Failures::new();
        let cases = random_cases(version, 6000, 4);
        run_cases(
            &cases,
            upstream,
            &mut tally,
            &mut failures,
            &mut Digest::default(),
        );
        (tally, failures)
    });
    report("random-program", &results);
}

#[test]
fn boundary_operands_match_anza_for_every_verified_opcode() {
    let results = run_parallel(|version, upstream| {
        let mut tally = Tally::default();
        let mut failures = Failures::new();
        let cases = boundary_cases(version, 1);
        run_cases(
            &cases,
            upstream,
            &mut tally,
            &mut failures,
            &mut Digest::default(),
        );
        (tally, failures)
    });
    report("boundary", &results);
}

/// Corpus lines: `<version> <campaign> <programs> <executions> <digest>`.
fn corpus_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/generated/ebpf/anza_sbpf.txt")
}

#[test]
fn corpus_streams_match_anza_and_the_checked_in_digests() {
    let lines = std::sync::Mutex::new(Vec::new());
    let results = run_parallel(|version, upstream| {
        let mut tally = Tally::default();
        let mut failures = Failures::new();
        for (campaign, cases) in corpus_campaigns(version) {
            let mut digest = Digest::default();
            let programs = tally.programs;
            let executions = tally.executions;
            run_cases(&cases, upstream, &mut tally, &mut failures, &mut digest);
            lines.lock().expect("lines").push(format!(
                "{version:?} {campaign} {} {} {}",
                tally.programs - programs,
                tally.executions - executions,
                digest.hex()
            ));
        }
        (tally, failures)
    });
    report("corpus", &results);
    let mut lines = lines.into_inner().expect("lines");
    lines.sort();
    let body = lines.join("\n") + "\n";
    if std::env::var_os("RAX_SBPF_CORPUS_OUT").is_some() {
        let header = "# Pinned Anza solana-sbpf 0.25.0 (commit 3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b)\n\
# interpreter outcomes for the corpus streams of tests/support/ebpf_sbpf_oracle.rs.\n\
# Generator: RAX_SBPF_CORPUS_OUT=1 python3 tools/ebpf-reference/run.py\n\
# Columns: version campaign programs executions digest\n";
        std::fs::write(corpus_path(), format!("{header}{body}")).expect("write corpus");
        eprintln!("wrote {}", corpus_path().display());
        return;
    }
    let checked_in = std::fs::read_to_string(corpus_path()).expect("checked-in corpus");
    let expected: Vec<&str> = checked_in
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    let actual: Vec<&str> = body.lines().collect();
    assert_eq!(actual, expected, "upstream corpus digests changed");
}

fn upstream_verdict(
    version: SBPFVersion,
    program: &[u8],
) -> Result<(), rax::isa::ebpf::SbpfVerifierError> {
    use rax::isa::ebpf::SbpfVerifierError as E;
    use solana_sbpf::verifier::{RequisiteVerifier, Verifier, VerifierError as U};
    let registry = solana_sbpf::program::FunctionRegistry::<usize>::default();
    RequisiteVerifier::verify(program, version, &registry).map_err(|error| match error {
        U::ProgramLengthNotMultiple => E::ProgramLengthNotMultiple,
        U::NoProgram => E::NoProgram,
        U::DivisionByZero(i) => E::DivisionByZero(i),
        U::UnsupportedLEBEArgument(i) => E::UnsupportedLeBeArgument(i),
        U::LDDWCannotBeLast => E::LddwCannotBeLast,
        U::IncompleteLDDW(i) => E::IncompleteLddw(i),
        U::JumpOutOfCode(t, i) => E::JumpOutOfCode(t, i),
        U::JumpToMiddleOfLDDW(t, i) => E::JumpToMiddleOfLddw(t, i),
        U::InvalidSourceRegister(i) => E::InvalidSourceRegister(i),
        U::CannotWriteR10(i) => E::CannotWriteR10(i),
        U::InvalidDestinationRegister(i) => E::InvalidDestinationRegister(i),
        U::UnknownOpCode(opc, i) => E::UnknownOpCode(opc, i),
        U::ShiftWithOverflow(amount, bits, i) => E::ShiftWithOverflow(amount, bits, i),
        U::InvalidRegister(i) => E::InvalidRegister(i),
        U::UnalignedImmediate(i) => E::UnalignedImmediate(i),
        other => panic!("unmapped upstream verifier error {other:?}"),
    })
}

#[test]
fn sbpf_admission_matches_the_reference_verifier_exhaustively() {
    use rax::isa::ebpf::verify_sbpf;
    let registers = [0u8, 1, 9, 10, 11, 15];
    let offsets = [-32768i16, -3, -2, -1, 0, 1, 2, 32767];
    let immediates = [
        i32::MIN,
        -65,
        -64,
        -1,
        0,
        1,
        9,
        10,
        16,
        31,
        32,
        33,
        63,
        64,
        65,
        128,
        i32::MAX,
    ];
    std::thread::scope(|scope| {
        for (version, upstream) in versions() {
            scope.spawn(move || {
                let mut checked = 0usize;
                let mut accepted = 0usize;
                let mut verdicts: BTreeMap<String, usize> = BTreeMap::new();
                let mut check = |program: &[u8]| {
                    let expected = upstream_verdict(upstream, program);
                    let actual = verify_sbpf(program, version);
                    assert_eq!(actual, expected, "{version:?} program={program:02x?}");
                    checked += 1;
                    accepted += usize::from(expected.is_ok());
                    let key = match expected {
                        Ok(()) => "accepted".to_string(),
                        Err(error) => format!("{error:?}").split('(').next().unwrap().to_string(),
                    };
                    *verdicts.entry(key).or_default() += 1;
                };
                // Every opcode in every operand-field partition, as the sole
                // instruction, before EXIT, before an opcode-zero slot, and
                // after EXIT.
                for opc in 0..=255u8 {
                    for &dst in &registers {
                        for &src in &registers {
                            for &off in &offsets {
                                for &imm in &immediates {
                                    let insn = encode(opc, dst, src, off, imm);
                                    check(&insn);
                                    check(&[insn, EXIT].concat());
                                    check(&[insn, [0; 8], EXIT].concat());
                                    check(&[EXIT, insn, EXIT].concat());
                                }
                            }
                        }
                    }
                }
                // Lengths that are not slot multiples, and the empty program.
                for len in 0..24 {
                    check(&vec![0x95; len]);
                }
                // Random multi-instruction programs over all 256 opcodes.
                let opcodes: Vec<u8> = (0..=255).collect();
                let mut rng = Rng(0x243f_6a88_85a3_08d3 ^ version as u64);
                for _ in 0..200_000 {
                    let (program, _) = random_program(&mut rng, version, &opcodes);
                    check(&program);
                }
                eprintln!(
                    "admission {version:?}: {checked} programs, {accepted} accepted; verdicts {verdicts:?}"
                );
            });
        }
    });
}

/// `RAX_SBPF_REPLAY=<file with one printed mismatch>`: print every plane.
#[test]
fn replay_a_recorded_case() {
    let Some(path) = std::env::var_os("RAX_SBPF_REPLAY") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("replay file");
    let (version, program, functions, state) = parse_case(&text).expect("parse case");
    let upstream = versions()
        .into_iter()
        .find(|(candidate, _)| *candidate == version)
        .expect("version")
        .1;
    let fixture = Fixture::new(version, upstream, &program, &functions)
        .unwrap_or_else(|error| panic!("{error:?}"));
    if std::env::var_os("RAX_SBPF_DUMP").is_some() {
        for level in [
            rax::smir::optimize::OptLevel::O0,
            rax::smir::optimize::OptLevel::O2,
        ] {
            fixture.dump(level);
        }
    }
    eprintln!("anza    {:x?}", fixture.anza(&state).result);
    for (plane, outcome) in fixture.rax.planes(&state) {
        eprintln!(
            "{plane:7} {:x?} insns={}",
            outcome.result, outcome.instructions
        );
    }
}

/// A reconstructed case: version, program bytes, function registry, and state.
type ParsedCase = (SbpfVersion, Vec<u8>, Vec<(u32, usize)>, State);

/// Parse a mismatch diagnostic (as printed by `compare`) back into a case.
fn parse_case(text: &str) -> Option<ParsedCase> {
    let field = |name: &str| -> Option<&str> {
        let start = text.find(name)? + name.len();
        let rest = &text[start..];
        Some(rest[..rest.find(['\n', ']']).unwrap_or(rest.len())].trim())
    };
    let bytes = |hex: &str| -> Vec<u8> {
        let digits: String = hex.chars().filter(char::is_ascii_hexdigit).collect();
        (0..digits.len() / 2)
            .map(|i| u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).expect("hex"))
            .collect()
    };
    let version = match text.trim_start().get(..2)? {
        "V0" => SbpfVersion::V0,
        "V1" => SbpfVersion::V1,
        "V2" => SbpfVersion::V2,
        "V3" => SbpfVersion::V3,
        _ => SbpfVersion::V4,
    };
    let program = bytes(field("program=")?.split(" functions=").next()?);
    let functions_text = &text[text.find("functions=[")? + 11..];
    let functions_text = &functions_text[..functions_text.find(']')?];
    let numbers: Vec<u64> = functions_text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|t| !t.is_empty())
        .map(|t| t.parse().expect("number"))
        .collect();
    let functions = numbers
        .chunks(2)
        .map(|pair| (pair[0] as u32, pair[1] as usize))
        .collect();
    let budget = field("budget=")?.split_whitespace().next()?.parse().ok()?;
    let regs_text = &text[text.find("regs=[")? + 6..];
    let regs_text = &regs_text[..regs_text.find(']')?];
    let mut regs = [0; 10];
    for (slot, token) in regs.iter_mut().zip(
        regs_text
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty()),
    ) {
        *slot = u64::from_str_radix(token.trim_start_matches("0x"), 16).expect("register");
    }
    Some((
        version,
        program,
        functions,
        State {
            regs,
            heap: bytes(field("heap=")?),
            input: bytes(field("input=")?),
            budget,
        },
    ))
}
