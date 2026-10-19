//! Replay of the Linux-kernel verifier admission corpora.
//!
//! `tests/generated/ebpf/linux_kernel_admission_{le,be}.txt` record a
//! campaign that loaded every RFC 9669 encoding partition into the kernel
//! verifier and compared its verdict with the rax decoder. Every
//! disagreement is listed with its reason: encodings RFC 9669 defines that
//! the verifier refuses by policy, and Linux extensions that postdate RFC
//! 9669. The decoder's verdict vector is pinned by digest, so a decoder
//! change cannot silently alter admission; rerun the campaign with
//! `tools/ebpf-conformance/run.py admission-le|admission-be`.

use rax::smir::ir::types::Endian;

use super::rfc_oracle::admission_digest;

fn replay(endian: Endian, corpus: &str) {
    let field = |name: &str| {
        corpus
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::trim)
            .unwrap_or_else(|| panic!("corpus lacks `{name}`"))
    };
    let (encodings, digest) = admission_digest(endian);
    assert_eq!(field("encodings ").parse::<usize>().unwrap(), encodings);
    assert_eq!(
        field("decoder "),
        digest.hex(),
        "{endian:?} decoder admission differs from the kernel-compared verdicts"
    );
    for line in corpus.lines().filter(|line| line.starts_with("disagree ")) {
        assert!(!line.ends_with("UNEXPLAINED"), "{line}");
    }
    let disagreements = corpus
        .lines()
        .filter(|line| line.starts_with("disagree "))
        .count();
    eprintln!("{endian:?}: {encodings} encodings, {disagreements} explained disagreements");
}

#[test]
fn decoder_admission_matches_the_little_endian_kernel_verifier_campaign() {
    replay(
        Endian::Little,
        include_str!("../../../generated/ebpf/linux_kernel_admission_le.txt"),
    );
}

#[test]
fn decoder_admission_matches_the_big_endian_kernel_verifier_campaign() {
    replay(
        Endian::Big,
        include_str!("../../../generated/ebpf/linux_kernel_admission_be.txt"),
    );
}
