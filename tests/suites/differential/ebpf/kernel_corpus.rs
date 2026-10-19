//! Replay of the Linux-kernel RFC 9669 corpora.
//!
//! `tests/generated/ebpf/linux_kernel_{le,be}.txt` record, per campaign
//! program, the digest of the kernel's `BPF_PROG_TEST_RUN` return value and
//! final context buffer: the arm64 JIT of Linux 7.0.14 (little-endian) and the
//! generic interpreter of Linux 6.12.107 on s390x (big-endian). This target
//! regenerates the identical programs (`tests/support/ebpf_rfc_oracle.rs`),
//! runs them on every rax plane, and requires every digest to match.
//! `tools/ebpf-conformance` reruns the kernels and reports case-level
//! differences.

use rax::smir::ir::types::Endian;

use super::rfc_oracle::{
    CORPUS_SCALE, Engine, forms, initial_registers, no_helpers, program_digest, run_rfc9669,
};

fn replay(endian: Endian, corpus: &str) {
    let programs: Vec<_> = forms(CORPUS_SCALE, endian)
        .iter()
        .flat_map(|form| form.programs(endian))
        .collect();
    let expected: Vec<&str> = corpus
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(
        expected.len(),
        programs.len(),
        "{endian:?} corpus program count"
    );
    let cases: usize = programs.iter().map(|program| program.cases.len()).sum();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(program) = programs.get(index) else {
                        break;
                    };
                    let fields: Vec<&str> = expected[index].split_whitespace().collect();
                    assert_eq!(
                        (fields[0], fields[1], fields[2]),
                        (
                            index.to_string().as_str(),
                            program.form.as_str(),
                            program.cases.len().to_string().as_str()
                        ),
                        "{endian:?} corpus line {index} describes a different program"
                    );
                    let bytes: Vec<u8> = program
                        .insns
                        .iter()
                        .flat_map(|insn| insn.encode(endian))
                        .collect();
                    for engine in Engine::ALL {
                        let result = run_rfc9669(
                            engine,
                            endian,
                            &bytes,
                            &program.ctx,
                            initial_registers(program.ctx.len()),
                            &no_helpers,
                        );
                        let r0 = result.exit.unwrap_or_else(|error| {
                            panic!(
                                "{endian:?} {} program {index} trapped: {error}",
                                engine.name()
                            )
                        });
                        let digest = program_digest(r0 as u32, &result.input).hex();
                        assert_eq!(
                            (format!("{:08x}", r0 as u32), digest),
                            (fields[3].to_string(), fields[4].to_string()),
                            "{endian:?} {} differs from the kernel on program {index} ({}); \
                             run tools/ebpf-conformance/run.py for case-level differences",
                            engine.name(),
                            program.form
                        );
                    }
                }
            });
        }
    });
    eprintln!(
        "{endian:?}: {} kernel programs, {cases} cases, {} planes",
        programs.len(),
        Engine::ALL.len()
    );
}

#[test]
fn every_plane_reproduces_the_little_endian_kernel_corpus() {
    replay(
        Endian::Little,
        include_str!("../../../generated/ebpf/linux_kernel_le.txt"),
    );
}

#[test]
fn every_plane_reproduces_the_big_endian_kernel_corpus() {
    replay(
        Endian::Big,
        include_str!("../../../generated/ebpf/linux_kernel_be.txt"),
    );
}
