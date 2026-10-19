//! Replay of the pinned Anza SBPF interpreter corpus.
//!
//! `tests/generated/ebpf/anza_sbpf.txt` holds digests of the upstream
//! interpreter's outcomes (result, all GPRs or trap class and slot, executed
//! instructions, and final stack, heap, and input bytes) for deterministic
//! V0-V4 case streams. This target regenerates the identical streams, runs
//! every rax plane, requires the planes to agree, and requires the digest of
//! their outcomes to equal the upstream digest. `tools/ebpf-reference`
//! regenerates the corpus and reports case-level differences.

use rax::isa::ebpf::{EbpfDialect, decode_insn};
use rax::smir::ir::types::Endian;

use super::sbpf_oracle::{
    Digest, RaxFixture, VERSIONS, corpus_campaigns, encode, is_endian, verified_opcodes,
};

const CORPUS: &str = include_str!("../../../generated/ebpf/anza_sbpf.txt");

#[test]
fn every_plane_reproduces_the_pinned_anza_corpus() {
    let expected: Vec<&str> = CORPUS
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(expected.len(), 20, "five versions x four campaigns");
    let lines = std::thread::scope(|scope| {
        let workers: Vec<_> = VERSIONS
            .into_iter()
            .map(|version| {
                scope.spawn(move || {
                    let mut lines = Vec::new();
                    for (campaign, cases) in corpus_campaigns(version) {
                        let mut digest = Digest::default();
                        let mut executions = 0usize;
                        for case in &cases {
                            let fixture =
                                RaxFixture::new(case.version, &case.program, &case.functions)
                                    .unwrap_or_else(|error| panic!("{error}"));
                            for state in &case.states {
                                let planes = fixture.planes(state);
                                for (plane, outcome) in &planes[1..] {
                                    assert_eq!(
                                        outcome, &planes[0].1,
                                        "{version:?} {campaign} {plane} differs from direct: {:02x?}",
                                        case.program
                                    );
                                }
                                digest.update(&planes[0].1.bytes());
                                executions += 1;
                            }
                        }
                        lines.push(format!(
                            "{version:?} {campaign} {} {executions} {}",
                            cases.len(),
                            digest.hex()
                        ));
                    }
                    lines
                })
            })
            .collect();
        let mut lines: Vec<String> = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("replay worker"))
            .collect();
        lines.sort();
        lines
    });
    for (actual, expected) in lines.iter().zip(&expected) {
        assert_eq!(
            actual, expected,
            "rax differs from the pinned Anza interpreter; run tools/ebpf-reference/run.py"
        );
    }
    eprintln!("replayed {} Anza corpus digests", lines.len());
}

/// Every version's decoder accepts exactly the opcodes `verify_sbpf` (equal
/// to the upstream `RequisiteVerifier`) admits, in a canonical encoding.
#[test]
fn the_decoder_accepts_exactly_the_admitted_sbpf_opcodes() {
    for version in VERSIONS {
        let opcodes = verified_opcodes(version);
        for opc in 0..=255u8 {
            if matches!(opc, 0x85 | 0x8d | 0x95) {
                continue;
            }
            let imm = if is_endian(opc) { 16 } else { 1 };
            let mut bytes = encode(opc, 1, 2, 0, imm).to_vec();
            bytes.extend_from_slice(&[0; 8]);
            let decodes = decode_insn(&bytes, EbpfDialect::Solana(version), Endian::Little).is_ok();
            assert_eq!(decodes, opcodes.contains(&opc), "{version:?} {opc:#04x}");
        }
        let expected = match version {
            rax::isa::ebpf::SbpfVersion::V0 | rax::isa::ebpf::SbpfVersion::V1 => 88,
            rax::isa::ebpf::SbpfVersion::V2 => 97,
            _ => 110,
        };
        assert_eq!(opcodes.len(), expected, "{version:?}");
    }
}
