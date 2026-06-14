//! Differential targets for x86_64 SIMD mnemonics still listed as unimplemented.
//!
//! The non-ignored test keeps the manifest-backed corpus assembled and complete.
//! The ignored qemu test is the semantic audit to enable while implementing each
//! instruction family.

#![cfg(feature = "x86_64-suite")]
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "x86_64/common/mod.rs"]
mod common;

use common::{run_until_hlt, setup_vm, Bytes, GuestAddress, Registers};

const WIRE_MAGIC: u32 = 0x5845_5645; // 'E','V','E','X' oracle wire format.
const ZMM_REGS: usize = 32;
const K_REGS: usize = 8;
const SCRATCH_BYTES: usize = 256;
const SCRATCH_ADDR: u64 = 0x4000;
const INITIAL_RFLAGS: u64 = 0x8d7;
const STATUS_RFLAGS_MASK: u64 = 0x8d5;

const LLVM_MATTR: &str =
    "+avx,+avx2,+avx512f,+avx512bw,+avx512dq,+sha512,+sm3,+sm4,+avxneconvert,+avxvnni,+avxifma";
const UNIMPLEMENTED_AVX: &str = include_str!("x86_64/avx_unimplemented_mnemonics.txt");
const UNIMPLEMENTED_AVX2: &str = include_str!("x86_64/avx2_unimplemented_mnemonics.txt");
const UNIMPLEMENTED_AVX10: &str = include_str!("x86_64/avx10_unimplemented_mnemonics.txt");
const UNIMPLEMENTED_AVX512: &str = include_str!("x86_64/avx512_unimplemented_mnemonics.txt");
const UNIMPLEMENTED_APX: &str = include_str!("x86_64/apx_unimplemented_mnemonics.txt");
const INTEL_INTRINSICS_XML: &str =
    include_str!("../docs/specifications/x86_64/intel-intrinsics-guide-3-6-9.xml");
const AVX10_CPUIDS: &[&str] = &[
    "AVX_IFMA",
    "AVX_NE_CONVERT",
    "AVX_VNNI",
    "AVX_VNNI_INT8",
    "AVX_VNNI_INT16",
];

#[repr(C)]
#[derive(Clone, Copy)]
struct InCase {
    id: u32,
    reserved: u32,
    zmm: [[u64; 8]; ZMM_REGS],
    k: [u64; K_REGS],
    rax: u64,
    r8: u64,
    rflags: u64,
    scratch: [u8; SCRATCH_BYTES],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct OutCase {
    id: u32,
    valid: u32,
    zmm: [[u64; 8]; ZMM_REGS],
    k: [u64; K_REGS],
    rax: u64,
    r8: u64,
    rflags: u64,
    scratch: [u8; SCRATCH_BYTES],
}

struct CaseSpec {
    extension: &'static str,
    mnemonic: &'static str,
    form: &'static str,
    xed: &'static str,
    label: &'static str,
    asm: &'static str,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct InstructionFormKey {
    extension: String,
    mnemonic: String,
    form: String,
    xed: String,
}

struct DiffCase {
    id: u32,
    extension: &'static str,
    mnemonic: &'static str,
    label: &'static str,
    asm: &'static str,
    op: Vec<u8>,
    input: InCase,
}

fn manifest_entries(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn case(
    extension: &'static str,
    mnemonic: &'static str,
    form: &'static str,
    xed: &'static str,
    label: &'static str,
    asm: &'static str,
) -> CaseSpec {
    CaseSpec {
        extension,
        mnemonic,
        form,
        xed,
        label,
        asm,
    }
}

fn spec_instruction_form_key(spec: &CaseSpec) -> InstructionFormKey {
    InstructionFormKey {
        extension: spec.extension.to_string(),
        mnemonic: spec.mnemonic.to_string(),
        form: spec.form.to_string(),
        xed: spec.xed.to_string(),
    }
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn tag_values(block: &str, tag: &str) -> BTreeSet<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = BTreeSet::new();
    let mut rest = block;
    while let Some(start) = rest.find(&open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(&close) else {
            break;
        };
        values.insert(rest[..end].trim().to_string());
        rest = &rest[end + close.len()..];
    }
    values
}

fn instruction_form_rows(block: &str) -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    let mut rest = block;
    while let Some(start) = rest.find("<instruction") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        if let (Some(name), Some(form), Some(xed)) = (
            attr_value(tag, "name"),
            attr_value(tag, "form"),
            attr_value(tag, "xed"),
        ) {
            rows.push((name.to_ascii_lowercase(), form, xed));
        }
        rest = &rest[end..];
    }
    rows
}

fn xml_block_matches_extension(extension: &str, tech: &str, cpuid: &BTreeSet<String>) -> bool {
    match extension {
        "avx" => tech == "AVX_ALL" && cpuid.contains("AVX"),
        "avx2" => tech == "AVX_ALL" && cpuid.contains("AVX2"),
        "avx10" => tech == "AVX_ALL" && AVX10_CPUIDS.iter().any(|feature| cpuid.contains(*feature)),
        "avx512" => {
            tech != "SVML"
                && (tech == "AVX-512" || cpuid.iter().any(|feature| feature.starts_with("AVX512")))
        }
        "apx" => tech == "APX" || cpuid.iter().any(|feature| feature.contains("APX")),
        _ => false,
    }
}

fn xml_instruction_form_rows_for_manifest(
    extension: &str,
    manifest: &str,
) -> BTreeSet<InstructionFormKey> {
    let manifest = manifest_entries(manifest);
    let mut rows = BTreeSet::new();
    for chunk in INTEL_INTRINSICS_XML.split("<intrinsic ").skip(1) {
        let Some(end) = chunk.find("</intrinsic>") else {
            continue;
        };
        let block = &chunk[..end];
        let tag_end = block.find('>').unwrap_or(block.len());
        let tag = &block[..tag_end];
        let tech = attr_value(tag, "tech").unwrap_or_default();
        let cpuid = tag_values(block, "CPUID");
        if !xml_block_matches_extension(extension, &tech, &cpuid) {
            continue;
        }
        for (mnemonic, form, xed) in instruction_form_rows(block) {
            if !manifest.contains(&mnemonic) {
                continue;
            }
            if matches!(extension, "avx" | "avx2" | "avx10") && !mnemonic.starts_with('v') {
                continue;
            }
            if extension == "avx512" && !mnemonic.starts_with('k') {
                continue;
            }
            rows.insert(InstructionFormKey {
                extension: extension.to_string(),
                mnemonic,
                form,
                xed,
            });
        }
    }
    rows
}

fn expected_xml_instruction_form_rows() -> BTreeSet<InstructionFormKey> {
    let mut rows = BTreeSet::new();
    for (extension, manifest) in [
        ("avx", UNIMPLEMENTED_AVX),
        ("avx2", UNIMPLEMENTED_AVX2),
        ("avx10", UNIMPLEMENTED_AVX10),
        ("avx512", UNIMPLEMENTED_AVX512),
        ("apx", UNIMPLEMENTED_APX),
    ] {
        rows.extend(xml_instruction_form_rows_for_manifest(extension, manifest));
    }
    rows
}

fn generated_specs() -> Vec<CaseSpec> {
    vec![
        case(
            "avx",
            "vbroadcastf128",
            "ymm, m128",
            "VBROADCASTF128_YMMqq_MEMdq",
            "avx::vbroadcastf128_ymm_mem128",
            "vbroadcastf128 ymm1, xmmword ptr [rax]",
        ),
        case(
            "avx",
            "vlddqu",
            "ymm, m256",
            "VLDDQU_YMMqq_MEMqq",
            "avx::vlddqu_ymm_mem256",
            "vlddqu ymm1, ymmword ptr [rax]",
        ),
        case(
            "avx",
            "vsha512msg1",
            "ymm, xmm",
            "VSHA512MSG1_YMMu64_XMMu64",
            "avx::vsha512msg1_ymm_xmm",
            "vsha512msg1 ymm1, xmm2",
        ),
        case(
            "avx",
            "vsha512msg2",
            "ymm, ymm",
            "VSHA512MSG2_YMMu64_YMMu64",
            "avx::vsha512msg2_ymm_ymm",
            "vsha512msg2 ymm1, ymm2",
        ),
        case(
            "avx",
            "vsha512rnds2",
            "ymm, ymm, xmm",
            "VSHA512RNDS2_YMMu64_YMMu64_XMMu64",
            "avx::vsha512rnds2_ymm_ymm_xmm",
            "vsha512rnds2 ymm1, ymm2, xmm3",
        ),
        case(
            "avx",
            "vsm3msg1",
            "xmm, xmm, xmm",
            "VSM3MSG1_XMMu32_XMMu32_XMMu32",
            "avx::vsm3msg1_xmm_xmm_xmm",
            "vsm3msg1 xmm1, xmm2, xmm3",
        ),
        case(
            "avx",
            "vsm3msg2",
            "xmm, xmm, xmm",
            "VSM3MSG2_XMMu32_XMMu32_XMMu32",
            "avx::vsm3msg2_xmm_xmm_xmm",
            "vsm3msg2 xmm1, xmm2, xmm3",
        ),
        case(
            "avx",
            "vsm3rnds2",
            "xmm, xmm, xmm, imm8",
            "VSM3RNDS2_XMMu32_XMMu32_XMMu32_IMM8",
            "avx::vsm3rnds2_xmm_xmm_xmm_imm8",
            "vsm3rnds2 xmm1, xmm2, xmm3, 0",
        ),
        case(
            "avx",
            "vsm4key4",
            "xmm, xmm, xmm",
            "VSM4KEY4_XMMu32_XMMu32_XMMu32",
            "avx::vsm4key4_xmm_xmm_xmm",
            "vsm4key4 xmm1, xmm2, xmm3",
        ),
        case(
            "avx",
            "vsm4key4",
            "ymm, ymm, ymm",
            "VSM4KEY4_YMMu32_YMMu32_YMMu32",
            "avx::vsm4key4_ymm_ymm_ymm",
            "vsm4key4 ymm1, ymm2, ymm3",
        ),
        case(
            "avx",
            "vsm4rnds4",
            "xmm, xmm, xmm",
            "VSM4RNDS4_XMMu32_XMMu32_XMMu32",
            "avx::vsm4rnds4_xmm_xmm_xmm",
            "vsm4rnds4 xmm1, xmm2, xmm3",
        ),
        case(
            "avx",
            "vsm4rnds4",
            "ymm, ymm, ymm",
            "VSM4RNDS4_YMMu32_YMMu32_YMMu32",
            "avx::vsm4rnds4_ymm_ymm_ymm",
            "vsm4rnds4 ymm1, ymm2, ymm3",
        ),
        case(
            "avx10",
            "vbcstnebf162ps",
            "xmm, m16",
            "VBCSTNEBF162PS_XMMf32_MEMbf16",
            "avx10::vbcstnebf162ps_xmm_mem16",
            "vbcstnebf162ps xmm1, word ptr [rax]",
        ),
        case(
            "avx10",
            "vbcstnebf162ps",
            "ymm, m16",
            "VBCSTNEBF162PS_YMMf32_MEMbf16",
            "avx10::vbcstnebf162ps_ymm_mem16",
            "vbcstnebf162ps ymm1, word ptr [rax]",
        ),
        case(
            "avx10",
            "vbcstnesh2ps",
            "xmm, m16",
            "VBCSTNESH2PS_XMMf32_MEMf16",
            "avx10::vbcstnesh2ps_xmm_mem16",
            "vbcstnesh2ps xmm1, word ptr [rax]",
        ),
        case(
            "avx10",
            "vbcstnesh2ps",
            "ymm, m16",
            "VBCSTNESH2PS_YMMf32_MEMf16",
            "avx10::vbcstnesh2ps_ymm_mem16",
            "vbcstnesh2ps ymm1, word ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneebf162ps",
            "xmm, m128",
            "VCVTNEEBF162PS_XMMf32_MEMbf16",
            "avx10::vcvtneebf162ps_xmm_mem128",
            "vcvtneebf162ps xmm1, xmmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneebf162ps",
            "ymm, m256",
            "VCVTNEEBF162PS_YMMf32_MEMbf16",
            "avx10::vcvtneebf162ps_ymm_mem256",
            "vcvtneebf162ps ymm1, ymmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneeph2ps",
            "xmm, m128",
            "VCVTNEEPH2PS_XMMf32_MEMf16",
            "avx10::vcvtneeph2ps_xmm_mem128",
            "vcvtneeph2ps xmm1, xmmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneeph2ps",
            "ymm, m256",
            "VCVTNEEPH2PS_YMMf32_MEMf16",
            "avx10::vcvtneeph2ps_ymm_mem256",
            "vcvtneeph2ps ymm1, ymmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneobf162ps",
            "xmm, m128",
            "VCVTNEOBF162PS_XMMf32_MEMbf16",
            "avx10::vcvtneobf162ps_xmm_mem128",
            "vcvtneobf162ps xmm1, xmmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneobf162ps",
            "ymm, m256",
            "VCVTNEOBF162PS_YMMf32_MEMbf16",
            "avx10::vcvtneobf162ps_ymm_mem256",
            "vcvtneobf162ps ymm1, ymmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneoph2ps",
            "xmm, m128",
            "VCVTNEOPH2PS_XMMf32_MEMf16",
            "avx10::vcvtneoph2ps_xmm_mem128",
            "vcvtneoph2ps xmm1, xmmword ptr [rax]",
        ),
        case(
            "avx10",
            "vcvtneoph2ps",
            "ymm, m256",
            "VCVTNEOPH2PS_YMMf32_MEMf16",
            "avx10::vcvtneoph2ps_ymm_mem256",
            "vcvtneoph2ps ymm1, ymmword ptr [rax]",
        ),
        case(
            "avx512",
            "kortestb",
            "k, k",
            "KORTESTB_MASKmskw_MASKmskw_AVX512",
            "avx512::kortestb_k_k",
            "kortestb k1, k2",
        ),
        case(
            "avx512",
            "kortestd",
            "k, k",
            "KORTESTD_MASKmskw_MASKmskw_AVX512",
            "avx512::kortestd_k_k",
            "kortestd k1, k2",
        ),
        case(
            "avx512",
            "kortestq",
            "k, k",
            "KORTESTQ_MASKmskw_MASKmskw_AVX512",
            "avx512::kortestq_k_k",
            "kortestq k1, k2",
        ),
        case(
            "avx512",
            "kortestw",
            "k, k",
            "KORTESTW_MASKmskw_MASKmskw_AVX512",
            "avx512::kortestw_k_k",
            "kortestw k1, k2",
        ),
        case(
            "avx512",
            "kshiftlb",
            "k, k, imm8",
            "KSHIFTLB_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftlb_k_k_imm8",
            "kshiftlb k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftld",
            "k, k, imm8",
            "KSHIFTLD_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftld_k_k_imm8",
            "kshiftld k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftlq",
            "k, k, imm8",
            "KSHIFTLQ_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftlq_k_k_imm8",
            "kshiftlq k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftlw",
            "k, k, imm8",
            "KSHIFTLW_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftlw_k_k_imm8",
            "kshiftlw k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftrb",
            "k, k, imm8",
            "KSHIFTRB_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftrb_k_k_imm8",
            "kshiftrb k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftrd",
            "k, k, imm8",
            "KSHIFTRD_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftrd_k_k_imm8",
            "kshiftrd k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftrq",
            "k, k, imm8",
            "KSHIFTRQ_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftrq_k_k_imm8",
            "kshiftrq k2, k1, 3",
        ),
        case(
            "avx512",
            "kshiftrw",
            "k, k, imm8",
            "KSHIFTRW_MASKmskw_MASKmskw_IMM8_AVX512",
            "avx512::kshiftrw_k_k_imm8",
            "kshiftrw k2, k1, 3",
        ),
        case(
            "avx512",
            "ktestb",
            "k, k",
            "KTESTB_MASKmskw_MASKmskw_AVX512",
            "avx512::ktestb_k_k",
            "ktestb k1, k2",
        ),
        case(
            "avx512",
            "ktestd",
            "k, k",
            "KTESTD_MASKmskw_MASKmskw_AVX512",
            "avx512::ktestd_k_k",
            "ktestd k1, k2",
        ),
        case(
            "avx512",
            "ktestq",
            "k, k",
            "KTESTQ_MASKmskw_MASKmskw_AVX512",
            "avx512::ktestq_k_k",
            "ktestq k1, k2",
        ),
        case(
            "avx512",
            "ktestw",
            "k, k",
            "KTESTW_MASKmskw_MASKmskw_AVX512",
            "avx512::ktestw_k_k",
            "ktestw k1, k2",
        ),
        case(
            "avx512",
            "kunpckbw",
            "k, k, k",
            "KUNPCKBW_MASKmskw_MASKmskw_MASKmskw_AVX512",
            "avx512::kunpckbw_k_k_k",
            "kunpckbw k2, k0, k1",
        ),
        case(
            "avx512",
            "kunpckdq",
            "k, k, k",
            "KUNPCKDQ_MASKmskw_MASKmskw_MASKmskw_AVX512",
            "avx512::kunpckdq_k_k_k",
            "kunpckdq k2, k0, k1",
        ),
        case(
            "avx512",
            "kunpckwd",
            "k, k, k",
            "KUNPCKWD_MASKmskw_MASKmskw_MASKmskw_AVX512",
            "avx512::kunpckwd_k_k_k",
            "kunpckwd k2, k0, k1",
        ),
    ]
}

fn assert_manifest_coverage(specs: &[CaseSpec]) {
    let mut actual = BTreeMap::<&str, BTreeSet<String>>::new();
    for spec in specs {
        actual
            .entry(spec.extension)
            .or_default()
            .insert(spec.mnemonic.to_string());
    }

    for (extension, manifest) in [
        ("avx", UNIMPLEMENTED_AVX),
        ("avx2", UNIMPLEMENTED_AVX2),
        ("avx10", UNIMPLEMENTED_AVX10),
        ("avx512", UNIMPLEMENTED_AVX512),
        ("apx", UNIMPLEMENTED_APX),
    ] {
        let expected = if extension == "avx512" {
            manifest_entries(manifest)
                .into_iter()
                .filter(|mnemonic| mnemonic.starts_with('k'))
                .collect()
        } else {
            manifest_entries(manifest)
        };
        let actual = actual.remove(extension).unwrap_or_default();
        assert_eq!(
            actual, expected,
            "{extension} unimplemented differential corpus must match its manifest"
        );
    }

    assert!(
        actual.is_empty(),
        "unimplemented differential corpus has unknown extension buckets: {:?}",
        actual.keys().collect::<Vec<_>>()
    );
}

fn format_instruction_form_keys(keys: BTreeSet<InstructionFormKey>) -> String {
    keys.into_iter()
        .map(|key| {
            format!(
                "{} {} form={} xed={}",
                key.extension, key.mnemonic, key.form, key.xed
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_xml_instruction_form_coverage(specs: &[CaseSpec]) {
    let actual = specs
        .iter()
        .map(spec_instruction_form_key)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual.len(),
        specs.len(),
        "unimplemented differential corpus must not duplicate XML instruction forms"
    );

    let expected = expected_xml_instruction_form_rows();
    let missing = expected
        .difference(&actual)
        .cloned()
        .collect::<BTreeSet<_>>();
    let unexpected = actual
        .difference(&expected)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "unimplemented differential corpus must cover every manifest-backed XML instruction form\nmissing:\n{}\nunexpected:\n{}",
        format_instruction_form_keys(missing),
        format_instruction_form_keys(unexpected)
    );
}

fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(prog))
        .find(|candidate| candidate.is_file())
}

fn llvm_mc_path() -> Option<PathBuf> {
    std::env::var_os("LLVM_MC")
        .map(PathBuf::from)
        .or_else(|| which("llvm-mc"))
}

fn qemu_path() -> Option<PathBuf> {
    std::env::var_os("QEMU_X86_64")
        .map(PathBuf::from)
        .or_else(|| which("qemu-x86_64"))
}

fn cc_path() -> Option<PathBuf> {
    std::env::var_os("CC")
        .map(PathBuf::from)
        .or_else(|| which("clang"))
        .or_else(|| which("cc"))
}

fn parse_encoding(text: &str) -> Option<Vec<u8>> {
    let start = text.find("encoding: [")? + "encoding: [".len();
    let rest = &text[start..];
    let end = rest.find(']')?;
    let mut bytes = Vec::new();
    for token in rest[..end].split(',') {
        let token = token.trim().trim_start_matches("0x");
        bytes.push(u8::from_str_radix(token, 16).ok()?);
    }
    Some(bytes)
}

fn assemble_case(llvm_mc: &Path, asm: &str) -> Option<Vec<u8>> {
    let mut child = Command::new(llvm_mc)
        .args([
            "-triple=x86_64",
            "-mcpu=skylake-avx512",
            "-mattr",
            LLVM_MATTR,
            "-x86-asm-syntax=intel",
            "-show-encoding",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{asm}\n").as_bytes())
        .ok()?;

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_encoding(&String::from_utf8_lossy(&output.stdout))
}

fn as_bytes<T: Copy>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn read_struct<T: Copy>(buf: &[u8], offset: usize) -> T {
    unsafe { std::ptr::read_unaligned(buf[offset..].as_ptr() as *const T) }
}

fn zmm_from_bytes(bytes: [u8; 64]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    out
}

fn set_zmm_bytes(input: &mut InCase, reg: usize, bytes: [u8; 64]) {
    input.zmm[reg] = zmm_from_bytes(bytes);
}

fn int_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = (reg * 37 + i * 29 + 0x83) as u8;
    }
    bytes
}

fn input_for(id: u32) -> InCase {
    let mut input = InCase {
        id,
        reserved: 0,
        zmm: [[0; 8]; ZMM_REGS],
        k: [
            u64::MAX,
            0x5555_5555_5555_5555,
            0xAAAA_AAAA_AAAA_AAAA,
            0x0F0F_0F0F_0F0F_0F0F,
            0x00FF_00FF_00FF_00FF,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        ],
        rax: SCRATCH_ADDR,
        r8: 0x8877_6655_4433_2211,
        rflags: INITIAL_RFLAGS,
        scratch: [0; SCRATCH_BYTES],
    };

    for reg in 0..ZMM_REGS {
        set_zmm_bytes(&mut input, reg, int_zmm(reg));
    }

    for (i, byte) in input.scratch.iter_mut().enumerate() {
        *byte = (0x31u8).wrapping_add((i as u8).wrapping_mul(17));
    }

    input
}

fn set_regs_zmm(regs: &mut Registers, index: usize, value: [u64; 8]) {
    if index < 16 {
        regs.xmm[index] = [value[0], value[1]];
        regs.ymm_high[index] = [value[2], value[3]];
        regs.zmm_high[index] = [value[4], value[5], value[6], value[7]];
    } else {
        regs.zmm_ext[index - 16] = value;
    }
}

fn get_regs_zmm(regs: &Registers, index: usize) -> [u64; 8] {
    if index < 16 {
        [
            regs.xmm[index][0],
            regs.xmm[index][1],
            regs.ymm_high[index][0],
            regs.ymm_high[index][1],
            regs.zmm_high[index][0],
            regs.zmm_high[index][1],
            regs.zmm_high[index][2],
            regs.zmm_high[index][3],
        ]
    } else {
        regs.zmm_ext[index - 16]
    }
}

fn registers_from_input(input: &InCase) -> Registers {
    let mut regs = Registers {
        rax: input.rax,
        r8: input.r8,
        rflags: input.rflags,
        ..Registers::default()
    };
    for reg in 0..ZMM_REGS {
        set_regs_zmm(&mut regs, reg, input.zmm[reg]);
    }
    regs.k = input.k;
    regs
}

fn assembled_cases(llvm_mc: &Path) -> Vec<DiffCase> {
    let specs = generated_specs();
    assert_manifest_coverage(&specs);
    assert_xml_instruction_form_coverage(&specs);

    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for spec in specs {
        let Some(op) = assemble_case(llvm_mc, spec.asm) else {
            failures.push(format!("{}: {}", spec.label, spec.asm));
            continue;
        };
        assert!(
            matches!(op.first(), Some(0xc4 | 0xc5)),
            "{} assembled outside VEX encoding: {:02x?}",
            spec.label,
            op
        );
        let id = cases.len() as u32;
        cases.push(DiffCase {
            id,
            extension: spec.extension,
            mnemonic: spec.mnemonic,
            label: spec.label,
            asm: spec.asm,
            op,
            input: input_for(id),
        });
    }

    assert!(
        failures.is_empty(),
        "unimplemented x86_64 differential corpus failed to assemble:\n{}",
        failures.join("\n")
    );
    cases
}

fn c_byte_directive(bytes: &[u8]) -> String {
    let mut text = String::from(".byte ");
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            text.push_str(", ");
        }
        text.push_str(&format!("0x{byte:02x}"));
    }
    text
}

fn write_cases_inc(build_dir: &Path, cases: &[DiffCase]) -> Option<PathBuf> {
    let cases_inc = build_dir.join("cases.inc");
    let mut text = String::new();
    for case in cases {
        text.push_str(&format!(
            "        case {}:\n            RUN_OP(\"{}\"); /* {} */\n            break;\n",
            case.id,
            c_byte_directive(&case.op),
            case.label
        ));
    }
    std::fs::write(&cases_inc, text).ok()?;
    Some(cases_inc)
}

fn oracle_path(cases: &[DiffCase]) -> Option<PathBuf> {
    let cc = cc_path()?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("tools/x86_64-evex-diff/oracle.c");
    let build_dir = root.join("target/x86_64-unimplemented-diff");
    let oracle = build_dir.join("oracle");
    std::fs::create_dir_all(&build_dir).ok()?;
    let cases_inc = write_cases_inc(&build_dir, cases)?;

    let include_arg = format!("-DEVEX_DIFF_CASES_INC=\"{}\"", cases_inc.display());
    let status = Command::new(cc)
        .args([
            "-O2",
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-mavx512f",
            "-mavx512bw",
            "-mavx512dq",
            "-mavx512vl",
            "-o",
        ])
        .arg(&oracle)
        .arg(&include_arg)
        .arg(&src)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    oracle.exists().then_some(oracle)
}

fn run_oracle(qemu: &Path, oracle: &Path, cases: &[DiffCase]) -> Option<Vec<OutCase>> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&WIRE_MAGIC.to_le_bytes());
    payload.extend_from_slice(&(cases.len() as u32).to_le_bytes());
    for case in cases {
        payload.extend_from_slice(as_bytes(&case.input));
    }

    let mut child = Command::new(qemu)
        .args(["-cpu", "max"])
        .arg(oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&payload);
    });

    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).ok()?;
    let _ = writer.join();
    if !child.wait().ok()?.success() || out.len() < 8 {
        return None;
    }

    let magic = u32::from_le_bytes(out[0..4].try_into().unwrap());
    let count = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
    if magic != WIRE_MAGIC || count != cases.len() {
        return None;
    }

    let expected_len = 8 + count * std::mem::size_of::<OutCase>();
    if out.len() != expected_len {
        return None;
    }

    let mut outputs = Vec::with_capacity(count);
    let mut offset = 8;
    for _ in 0..count {
        outputs.push(read_struct::<OutCase>(&out, offset));
        offset += std::mem::size_of::<OutCase>();
    }
    Some(outputs)
}

fn try_run_rax(case: &DiffCase) -> Result<OutCase, String> {
    let mut code = case.op.clone();
    code.push(0xf4);

    let (mut vcpu, mem) = setup_vm(&code, Some(registers_from_input(&case.input)));
    mem.write_slice(&case.input.scratch, GuestAddress(SCRATCH_ADDR))
        .unwrap();
    let regs = run_until_hlt(&mut vcpu).map_err(|error| error.to_string())?;

    let mut scratch = [0u8; SCRATCH_BYTES];
    mem.read_slice(&mut scratch, GuestAddress(SCRATCH_ADDR))
        .unwrap();

    let mut out = OutCase {
        id: case.id,
        valid: 1,
        zmm: [[0; 8]; ZMM_REGS],
        k: regs.k,
        rax: regs.rax,
        r8: regs.r8,
        rflags: regs.rflags,
        scratch,
    };
    for reg in 0..ZMM_REGS {
        out.zmm[reg] = get_regs_zmm(&regs, reg);
    }
    Ok(out)
}

fn assert_same_snapshot(case: &DiffCase, rax: &OutCase, oracle: &OutCase) {
    assert_eq!(rax.id, oracle.id, "{}: case id", case.label);
    assert_eq!(rax.zmm, oracle.zmm, "{}: ZMM snapshot", case.label);
    assert_eq!(rax.k, oracle.k, "{}: opmask snapshot", case.label);
    assert_eq!(rax.rax, oracle.rax, "{}: RAX snapshot", case.label);
    assert_eq!(rax.r8, oracle.r8, "{}: R8 snapshot", case.label);
    assert_eq!(
        rax.rflags & STATUS_RFLAGS_MASK,
        oracle.rflags & STATUS_RFLAGS_MASK,
        "{}: RFLAGS status snapshot",
        case.label
    );
    assert_eq!(
        rax.scratch, oracle.scratch,
        "{}: scratch memory snapshot",
        case.label
    );
}

#[test]
fn unimplemented_vex_simd_diff_corpus_covers_manifests_and_assembles() {
    let specs = generated_specs();
    assert_manifest_coverage(&specs);
    assert_xml_instruction_form_coverage(&specs);

    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping unimplemented x86_64 assembly coverage");
        return;
    };
    let cases = assembled_cases(&llvm_mc);
    assert_eq!(cases.len(), specs.len());
}

#[test]
#[ignore = "semantic audit for VEX SIMD instructions intentionally still listed as unimplemented"]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn qemu_unimplemented_vex_simd_corpus_matches_rax_when_enabled() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping unimplemented x86_64 differential audit");
        return;
    };
    let cases = assembled_cases(&llvm_mc);
    assert!(
        !cases.is_empty(),
        "unimplemented VEX SIMD differential corpus is empty"
    );

    let Some(qemu) = qemu_path() else {
        eprintln!(
            "[skip] qemu-x86_64 unavailable; skipping unimplemented x86_64 differential audit"
        );
        return;
    };
    let Some(oracle) = oracle_path(&cases) else {
        eprintln!("[skip] x86_64 unimplemented oracle build failed or compiler unavailable");
        return;
    };
    let Some(outputs) = run_oracle(&qemu, &oracle, &cases) else {
        eprintln!("[skip] qemu-x86_64 could not run the x86_64 unimplemented oracle");
        return;
    };

    let mut compared = 0usize;
    let mut qemu_unsupported = 0usize;
    let mut rax_failures = Vec::new();
    for (case, oracle) in cases.iter().zip(outputs.iter()) {
        assert_eq!(oracle.id, case.id, "{}: oracle case id", case.label);
        if oracle.valid == 0 {
            qemu_unsupported += 1;
            continue;
        }
        match try_run_rax(case) {
            Ok(rax) => {
                assert_same_snapshot(case, &rax, oracle);
                compared += 1;
            }
            Err(error) => rax_failures.push(format!("{}: {error}", case.label)),
        }
    }

    assert!(
        compared > 0 || !rax_failures.is_empty(),
        "qemu rejected all {qemu_unsupported} unimplemented VEX SIMD cases"
    );
    assert!(
        rax_failures.is_empty(),
        "unimplemented VEX SIMD differential audit found rax execution failures\ncompared: {compared}\nqemu rejected: {qemu_unsupported}\nfailures:\n{}",
        rax_failures
            .into_iter()
            .take(80)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
