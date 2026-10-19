//! Replay of the eBPF Foundation `bpf_conformance` suite.
//!
//! `tests/generated/ebpf/bpf_conformance.txt` holds the suite's programs as
//! assembled by its own assembler, the memory it supplies, and its expected
//! `r0`, plus the raw negative programs whose unused fields are non-zero. The
//! plugin protocol sets `r1` to the memory, `r2` to its length, and `r10` to
//! the top of the stack; the conformance platform defines helper 5.

use rax::isa::ebpf::{EbpfDialect, decode_insn};
use rax::smir::ir::types::Endian;

use super::rfc_oracle::{Engine, conformance_helpers, initial_registers, run_rfc9669};

const CORPUS: &str = include_str!("../../../generated/ebpf/bpf_conformance.txt");

fn bytes(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("hex byte"))
        .collect()
}

#[test]
fn every_plane_passes_the_bpf_conformance_suite() {
    let (mut passed, mut rejected, mut skipped) = (0, 0, Vec::new());
    for line in CORPUS.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields[0] {
            "run" => {
                let (name, program, memory) = (fields[1], bytes(fields[2]), bytes(fields[3]));
                let expected = u64::from_str_radix(fields[4], 16).expect("expected r0");
                for engine in Engine::ALL {
                    let result = run_rfc9669(
                        engine,
                        Endian::Little,
                        &program,
                        &memory,
                        initial_registers(memory.len()),
                        &conformance_helpers,
                    );
                    assert_eq!(result.exit, Ok(expected), "{name} {}", engine.name());
                }
                passed += 1;
            }
            "reject" => {
                let (name, program) = (fields[1], bytes(fields[2]));
                for engine in Engine::ALL {
                    let result = run_rfc9669(
                        engine,
                        Endian::Little,
                        &program,
                        &[],
                        initial_registers(0),
                        &conformance_helpers,
                    );
                    assert!(
                        result.exit.is_err(),
                        "{name} {}: {:?}",
                        engine.name(),
                        result.exit
                    );
                }
                rejected += 1;
            }
            "skip" => skipped.push((fields[1], fields[2])),
            other => panic!("unknown corpus record {other}"),
        }
    }
    // The runner skips only the uBPF `callx` group, which RFC 9669 does not
    // define: the RFC dialect rejects opcode 0x8d.
    assert_eq!(skipped, [("callx.data", "unsupported-group")]);
    let callx = [0x8d, 0x02, 0, 0, 0, 0, 0, 0];
    assert!(decode_insn(&callx, EbpfDialect::Rfc9669, Endian::Little).is_err());
    assert_eq!((passed, rejected), (312, 45));
    eprintln!(
        "bpf_conformance: {passed} tests passed and {rejected} negatives rejected on {} planes",
        Engine::ALL.len()
    );
}
