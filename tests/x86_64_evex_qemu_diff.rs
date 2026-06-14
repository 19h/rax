//! Generated EVEX differential tests: rax vs. qemu-x86_64.
//!
//! The corpus is scoped to EVEX SIMD handlers that rax dispatches today. It is
//! generated from assembly strings, assembled with LLVM for the rax side, and
//! compiled into a qemu oracle from the same case table.

#![cfg(feature = "x86_64-suite")]
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "x86_64/common/mod.rs"]
mod common;

#[path = "x86_64/avx512_inventory_data.rs"]
mod avx512_inventory_data;

#[path = "x86_64/avx512_spec.rs"]
mod avx512_spec;

use avx512_inventory_data::RAX_EVEX_SIMD_DIFF_MNEMONICS;
use avx512_spec::{
    EvexAsmMode, EvexCaseVariant, EvexOperandForm, EvexRmRegisterClass, avx512_spec_evex_rows,
    evex_case_variants_for_row, evex_rm_register_class, raw_evex_spec_bytes_for_variant,
    spec_case_variant_id,
};
use common::{Bytes, GuestAddress, Registers, run_until_hlt, setup_vm};

const WIRE_MAGIC: u32 = 0x5845_5645; // 'E','V','E','X'
const ZMM_REGS: usize = 32;
const K_REGS: usize = 8;
const SCRATCH_BYTES: usize = 256;
const SCRATCH_ADDR: u64 = 0x4000;

const LLVM_MATTR: &str = "+avx512f,+avx512bw,+avx512dq,+avx512vl,+avx512cd,+avx512fp16,+avx512vnni,+avx512ifma,+avx512vpopcntdq,+avx512vbmi,+avx512vbmi2,+avx512bitalg,+avx512bf16,+avxvnni,+vp2intersect";
const INITIAL_RFLAGS: u64 = 0x8d7;
const STATUS_RFLAGS_MASK: u64 = 0x8d5;
const UNIMPLEMENTED_AVX512: &str = include_str!("x86_64/avx512_unimplemented_mnemonics.txt");

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

#[derive(Clone, Copy)]
enum InputProfile {
    Int,
    F32,
    F64,
    F16,
    F32Pow4,
    F64Pow4,
    F16Pow4,
    Vsib32,
    Vsib64,
}

struct CaseSpec {
    label: String,
    asm: String,
    op: Option<Vec<u8>>,
    profile: InputProfile,
}

struct DiffCase {
    id: u32,
    label: String,
    asm: String,
    op: Vec<u8>,
    input: InCase,
}

const RM_EXT_REGS: [usize; 4] = [0, 8, 16, 24];
const DST_EXT_REGS: [usize; 2] = [1, 17];
const SRC1_EXT_REGS: [usize; 2] = [2, 18];

fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(prog))
        .find(|candidate| candidate.is_file())
}

fn qemu_path() -> Option<PathBuf> {
    std::env::var_os("QEMU_X86_64")
        .map(PathBuf::from)
        .or_else(|| which("qemu-x86_64"))
}

fn llvm_mc_path() -> Option<PathBuf> {
    std::env::var_os("LLVM_MC")
        .map(PathBuf::from)
        .or_else(|| which("llvm-mc"))
}

fn cc_path() -> Option<PathBuf> {
    std::env::var_os("CC")
        .map(PathBuf::from)
        .or_else(|| which("clang"))
        .or_else(|| which("cc"))
}

fn as_bytes<T: Copy>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn read_struct<T: Copy>(buf: &[u8], offset: usize) -> T {
    unsafe { std::ptr::read_unaligned(buf[offset..].as_ptr() as *const T) }
}

fn zmm_to_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (i, word) in value.iter().enumerate() {
        bytes[i * 8..i * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
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

fn f32_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = 1.0 + reg as f32 * 0.125 + lane as f32 * 0.0625;
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f64_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = 1.0 + reg as f64 * 0.25 + lane as f64 * 0.125;
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn int_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = (reg * 37 + i * 29 + 0x83) as u8;
    }
    bytes
}

fn f16_zmm(reg: usize) -> [u8; 64] {
    const VALUES: [u16; 8] = [
        0x3c00, 0x4000, 0x4200, 0x4400, 0x4500, 0x4600, 0x4700, 0x4800,
    ];
    let mut bytes = [0u8; 64];
    for lane in 0..32 {
        let value = VALUES[(reg + lane) % VALUES.len()];
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f32_pow4_zmm(reg: usize) -> [u8; 64] {
    const VALUES: [f32; 4] = [1.0, 4.0, 16.0, 64.0];
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = VALUES[(reg + lane) % VALUES.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f64_pow4_zmm(reg: usize) -> [u8; 64] {
    const VALUES: [f64; 4] = [1.0, 4.0, 16.0, 64.0];
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = VALUES[(reg + lane) % VALUES.len()];
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f16_pow4_zmm(reg: usize) -> [u8; 64] {
    const VALUES: [u16; 4] = [0x3c00, 0x4400, 0x4c00, 0x5400];
    let mut bytes = [0u8; 64];
    for lane in 0..32 {
        let value = VALUES[(reg + lane) % VALUES.len()];
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn vsib32_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = ((lane + (reg & 0x3)) * 8) as i32;
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn vsib64_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = ((lane + (reg & 0x3)) * 8) as i64;
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn profile_zmm(profile: InputProfile, reg: usize) -> [u8; 64] {
    match profile {
        InputProfile::Int => int_zmm(reg),
        InputProfile::F32 => f32_zmm(reg),
        InputProfile::F64 => f64_zmm(reg),
        InputProfile::F16 => f16_zmm(reg),
        InputProfile::F32Pow4 => f32_pow4_zmm(reg),
        InputProfile::F64Pow4 => f64_pow4_zmm(reg),
        InputProfile::F16Pow4 => f16_pow4_zmm(reg),
        InputProfile::Vsib32 => vsib32_zmm(reg),
        InputProfile::Vsib64 => vsib64_zmm(reg),
    }
}

fn input_for(id: u32, profile: InputProfile) -> InCase {
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

    if matches!(profile, InputProfile::Vsib32 | InputProfile::Vsib64) {
        input.k[1] = u64::MAX;
    }

    for reg in 0..ZMM_REGS {
        set_zmm_bytes(&mut input, reg, profile_zmm(profile, reg));
    }

    let scratch_profile = profile_zmm(profile, 31);
    for chunk in input.scratch.chunks_mut(64) {
        chunk.copy_from_slice(&scratch_profile[..chunk.len()]);
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

fn mov_imm64(code: &mut Vec<u8>, reg: u8, value: u64) {
    code.push(0x48);
    code.push(0xb8 + reg);
    code.extend_from_slice(&value.to_le_bytes());
}

fn run_rax(case: &DiffCase) -> OutCase {
    try_run_rax(case)
        .unwrap_or_else(|error| panic!("{}: rax execution failed: {error}", case.label))
}

fn try_run_rax(case: &DiffCase) -> Result<OutCase, String> {
    let mut code = Vec::new();
    mov_imm64(&mut code, 0, SCRATCH_ADDR);
    code.extend_from_slice(&case.op);
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

fn spec(
    specs: &mut Vec<CaseSpec>,
    label: impl Into<String>,
    asm: impl Into<String>,
    profile: InputProfile,
) {
    specs.push(CaseSpec {
        label: label.into(),
        asm: asm.into(),
        op: None,
        profile,
    });
}

fn spec_raw(
    specs: &mut Vec<CaseSpec>,
    label: impl Into<String>,
    asm: impl Into<String>,
    op: Vec<u8>,
    profile: InputProfile,
) {
    specs.push(CaseSpec {
        label: label.into(),
        asm: asm.into(),
        op: Some(op),
        profile,
    });
}

fn asm_mnemonic(asm: &str) -> String {
    asm.split_whitespace()
        .next()
        .expect("generated EVEX asm must have a mnemonic")
        .to_ascii_lowercase()
}

fn expected_diff_mnemonics() -> BTreeSet<String> {
    RAX_EVEX_SIMD_DIFF_MNEMONICS
        .iter()
        .map(|mnemonic| (*mnemonic).to_string())
        .collect()
}

fn assert_mnemonic_coverage(mnemonics: BTreeSet<String>, context: &str) {
    let expected = expected_diff_mnemonics();
    let missing = expected
        .difference(&mnemonics)
        .cloned()
        .collect::<BTreeSet<_>>();
    let unexpected = mnemonics
        .difference(&expected)
        .cloned()
        .collect::<BTreeSet<_>>();

    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "{context} mnemonic coverage mismatch\nmissing:\n{}\nunexpected:\n{}",
        missing.into_iter().collect::<Vec<_>>().join("\n"),
        unexpected.into_iter().collect::<Vec<_>>().join("\n")
    );
}

fn assert_requested_mnemonic_coverage(specs: &[CaseSpec]) {
    assert_mnemonic_coverage(
        specs.iter().map(|spec| asm_mnemonic(&spec.asm)).collect(),
        "generated EVEX differential specs",
    );
}

fn assert_assembled_mnemonic_coverage(cases: &[DiffCase]) {
    assert_mnemonic_coverage(
        cases.iter().map(|case| asm_mnemonic(&case.asm)).collect(),
        "assembled EVEX differential cases",
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EvexSelector {
    map: u8,
    opcode: u8,
    pp: u8,
    w: bool,
    subop: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OperandForm {
    Register,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MaskMode {
    None,
    Merge,
    Zero,
}

#[derive(Clone, Debug)]
struct EvexCoverage {
    selector: EvexSelector,
    form: OperandForm,
    ll: u8,
    mask: MaskMode,
    rm_reg: Option<u8>,
}

fn sel(map: u8, opcode: u8, pp: u8, w: bool) -> EvexSelector {
    EvexSelector {
        map,
        opcode,
        pp,
        w,
        subop: None,
    }
}

fn group_sel(map: u8, opcode: u8, pp: u8, w: bool, subop: u8) -> EvexSelector {
    EvexSelector {
        map,
        opcode,
        pp,
        w,
        subop: Some(subop),
    }
}

fn is_evex_fma_opcode(opcode: u8) -> bool {
    matches!(opcode, 0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF)
}

fn is_evex_scalar_fma_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        0x99 | 0x9B | 0x9D | 0x9F | 0xA9 | 0xAB | 0xAD | 0xAF | 0xB9 | 0xBB | 0xBD | 0xBF
    )
}

fn selector_uses_gpr_rm_bucket(selector: EvexSelector) -> bool {
    matches!(
        selector,
        EvexSelector {
            map: 1,
            opcode: 0x2a | 0x7b,
            pp: 2 | 3,
            subop: None,
            ..
        } | EvexSelector {
            map: 5,
            opcode: 0x2a | 0x7b,
            pp: 2,
            subop: None,
            ..
        } | EvexSelector {
            map: 1,
            opcode: 0xc4 | 0xc5,
            pp: 1,
            w: false,
            subop: None,
        } | EvexSelector {
            map: 3,
            opcode: 0x14 | 0x15 | 0x17 | 0x20,
            pp: 1,
            w: false,
            subop: None,
        } | EvexSelector {
            map: 3,
            opcode: 0x16 | 0x22,
            pp: 1,
            subop: None,
            ..
        }
    )
}

fn expected_dispatch_selectors() -> BTreeSet<EvexSelector> {
    let mut selectors = BTreeSet::new();

    for opcode in [0x10, 0x11, 0x28, 0x29] {
        selectors.insert(sel(1, opcode, 0, false));
        selectors.insert(sel(1, opcode, 1, true));
    }
    selectors.insert(sel(1, 0x2b, 0, false));
    selectors.insert(sel(1, 0x2b, 1, true));
    selectors.insert(sel(1, 0xe7, 1, false));
    selectors.insert(sel(1, 0x10, 2, false));
    selectors.insert(sel(1, 0x10, 3, true));
    selectors.insert(sel(1, 0x11, 2, false));
    selectors.insert(sel(1, 0x11, 3, true));
    selectors.insert(sel(1, 0x12, 0, false));
    selectors.insert(sel(1, 0x12, 1, true));
    selectors.insert(sel(1, 0x12, 2, false));
    selectors.insert(sel(1, 0x12, 3, true));
    selectors.insert(sel(1, 0x16, 0, false));
    selectors.insert(sel(1, 0x16, 1, true));
    selectors.insert(sel(1, 0x16, 2, false));
    selectors.insert(sel(1, 0x6e, 1, false));
    selectors.insert(sel(1, 0x6e, 1, true));
    selectors.insert(sel(1, 0x7e, 1, false));
    selectors.insert(sel(1, 0x7e, 1, true));
    selectors.insert(sel(1, 0x7e, 2, true));
    selectors.insert(sel(1, 0xd6, 1, true));
    for opcode in [0x2e, 0x2f] {
        selectors.insert(sel(1, opcode, 0, false));
        selectors.insert(sel(1, opcode, 1, true));
    }
    for opcode in [0x2a, 0x2c, 0x2d, 0x78, 0x79, 0x7b] {
        for (pp, w) in [(2, false), (2, true), (3, false), (3, true)] {
            selectors.insert(sel(1, opcode, pp, w));
        }
    }
    for (opcode, pp, w) in [
        (0x5a, 0, false),
        (0x5a, 1, true),
        (0x5b, 0, false),
        (0x5b, 0, true),
        (0x5b, 1, false),
        (0x5b, 2, false),
        (0x78, 0, false),
        (0x78, 0, true),
        (0x78, 1, false),
        (0x78, 1, true),
        (0x79, 0, false),
        (0x79, 0, true),
        (0x79, 1, false),
        (0x79, 1, true),
        (0x7a, 1, false),
        (0x7a, 1, true),
        (0x7a, 2, false),
        (0x7a, 2, true),
        (0x7a, 3, false),
        (0x7a, 3, true),
        (0x7b, 1, false),
        (0x7b, 1, true),
        (0xe6, 1, true),
        (0xe6, 2, false),
        (0xe6, 2, true),
        (0xe6, 3, true),
    ] {
        selectors.insert(sel(1, opcode, pp, w));
    }
    selectors.insert(sel(1, 0x5a, 2, false));
    selectors.insert(sel(1, 0x5a, 3, true));
    selectors.insert(sel(1, 0xc2, 0, false));
    selectors.insert(sel(1, 0xc2, 1, true));
    selectors.insert(sel(1, 0xc2, 2, false));
    selectors.insert(sel(1, 0xc2, 3, true));
    for opcode in [0x51, 0x58, 0x59, 0x5c, 0x5d, 0x5e, 0x5f] {
        selectors.insert(sel(1, opcode, 0, false));
        selectors.insert(sel(1, opcode, 1, true));
        selectors.insert(sel(1, opcode, 2, false));
        selectors.insert(sel(1, opcode, 3, true));
    }
    for opcode in [0x54, 0x55, 0x56, 0x57] {
        selectors.insert(sel(1, opcode, 0, false));
        selectors.insert(sel(1, opcode, 1, true));
    }
    for opcode in [0x14, 0x15] {
        selectors.insert(sel(1, opcode, 0, false));
        selectors.insert(sel(1, opcode, 1, true));
    }

    for opcode in [0x6f, 0x7f] {
        for (pp, w) in [
            (1, false),
            (1, true),
            (2, false),
            (2, true),
            (3, false),
            (3, true),
        ] {
            selectors.insert(sel(1, opcode, pp, w));
        }
    }

    for opcode in [0xdb, 0xdf, 0xeb, 0xef] {
        selectors.insert(sel(1, opcode, 1, false));
        selectors.insert(sel(1, opcode, 1, true));
    }
    for opcode in [
        0xd5, 0xd8, 0xd9, 0xda, 0xdc, 0xdd, 0xde, 0xe0, 0xe3, 0xe4, 0xe5, 0xe8, 0xe9, 0xea, 0xec,
        0xed, 0xee, 0xf5, 0xf8, 0xf9, 0xfa, 0xfc, 0xfd, 0xfe,
    ] {
        selectors.insert(sel(1, opcode, 1, false));
    }
    for opcode in [0xd4, 0xf4, 0xfb] {
        selectors.insert(sel(1, opcode, 1, true));
    }
    for opcode in [0x74, 0x75, 0x76, 0x64, 0x65, 0x66] {
        selectors.insert(sel(1, opcode, 1, false));
    }
    selectors.insert(sel(1, 0x63, 1, false));
    selectors.insert(sel(1, 0x67, 1, false));
    selectors.insert(sel(1, 0x6b, 1, false));
    selectors.insert(sel(1, 0x70, 1, false));
    selectors.insert(sel(1, 0x70, 2, false));
    selectors.insert(sel(1, 0x70, 3, false));
    selectors.insert(sel(1, 0xc4, 1, false));
    selectors.insert(sel(1, 0xc5, 1, false));
    selectors.insert(sel(1, 0xc6, 0, false));
    selectors.insert(sel(1, 0xc6, 1, true));
    for opcode in [0x60, 0x61, 0x62, 0x68, 0x69, 0x6a] {
        selectors.insert(sel(1, opcode, 1, false));
    }
    selectors.insert(sel(1, 0x6c, 1, true));
    selectors.insert(sel(1, 0x6d, 1, true));

    selectors.insert(group_sel(1, 0x71, 1, false, 2));
    selectors.insert(group_sel(1, 0x71, 1, false, 4));
    selectors.insert(group_sel(1, 0x71, 1, false, 6));
    selectors.insert(group_sel(1, 0x72, 1, false, 0));
    selectors.insert(group_sel(1, 0x72, 1, true, 0));
    selectors.insert(group_sel(1, 0x72, 1, false, 1));
    selectors.insert(group_sel(1, 0x72, 1, true, 1));
    selectors.insert(group_sel(1, 0x72, 1, false, 2));
    selectors.insert(group_sel(1, 0x72, 1, false, 4));
    selectors.insert(group_sel(1, 0x72, 1, true, 4));
    selectors.insert(group_sel(1, 0x72, 1, false, 6));
    selectors.insert(group_sel(1, 0x73, 1, true, 2));
    selectors.insert(group_sel(1, 0x73, 1, false, 3));
    selectors.insert(group_sel(1, 0x73, 1, true, 6));
    selectors.insert(group_sel(1, 0x73, 1, false, 7));
    selectors.insert(sel(1, 0xd1, 1, false));
    selectors.insert(sel(1, 0xd2, 1, false));
    selectors.insert(sel(1, 0xd3, 1, true));
    selectors.insert(sel(1, 0xe1, 1, false));
    selectors.insert(sel(1, 0xe2, 1, false));
    selectors.insert(sel(1, 0xe2, 1, true));
    selectors.insert(sel(1, 0xf1, 1, false));
    selectors.insert(sel(1, 0xf2, 1, false));
    selectors.insert(sel(1, 0xf3, 1, true));
    selectors.insert(sel(1, 0xf6, 1, false));

    selectors.insert(sel(2, 0x40, 1, false));
    selectors.insert(sel(2, 0x40, 1, true));
    selectors.insert(sel(2, 0x68, 3, false));
    selectors.insert(sel(2, 0x68, 3, true));
    for opcode in [0x2c, 0x2d, 0x42, 0x43, 0x4c, 0x4d, 0x4e, 0x4f] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0xc8, 0xca, 0xcb, 0xcc, 0xcd] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    selectors.insert(sel(2, 0x00, 1, false));
    selectors.insert(sel(2, 0x04, 1, false));
    selectors.insert(sel(2, 0x0b, 1, false));
    selectors.insert(sel(2, 0x10, 1, true));
    selectors.insert(sel(2, 0x11, 1, true));
    selectors.insert(sel(2, 0x12, 1, true));
    selectors.insert(sel(2, 0x13, 1, false));
    selectors.insert(sel(2, 0x70, 1, true));
    selectors.insert(sel(2, 0x71, 1, false));
    selectors.insert(sel(2, 0x71, 1, true));
    selectors.insert(sel(2, 0x72, 1, true));
    selectors.insert(sel(2, 0x73, 1, false));
    selectors.insert(sel(2, 0x73, 1, true));
    selectors.insert(sel(2, 0x14, 1, false));
    selectors.insert(sel(2, 0x14, 1, true));
    selectors.insert(sel(2, 0x15, 1, false));
    selectors.insert(sel(2, 0x15, 1, true));
    selectors.insert(sel(2, 0x1c, 1, false));
    selectors.insert(sel(2, 0x1d, 1, false));
    selectors.insert(sel(2, 0x1e, 1, false));
    selectors.insert(sel(2, 0x1f, 1, true));
    for opcode in [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35,
    ] {
        selectors.insert(sel(2, opcode, 1, false));
    }
    for opcode in [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x30, 0x31, 0x32,
        0x33, 0x34, 0x35,
    ] {
        selectors.insert(sel(2, opcode, 2, false));
    }
    for (opcode, pp, w) in [
        (0x26, 1, false),
        (0x26, 1, true),
        (0x26, 2, false),
        (0x26, 2, true),
        (0x27, 1, false),
        (0x27, 1, true),
        (0x27, 2, false),
        (0x27, 2, true),
    ] {
        selectors.insert(sel(2, opcode, pp, w));
    }
    selectors.insert(sel(2, 0x28, 1, true));
    selectors.insert(sel(2, 0x28, 2, false));
    selectors.insert(sel(2, 0x28, 2, true));
    selectors.insert(sel(2, 0x29, 2, false));
    selectors.insert(sel(2, 0x29, 2, true));
    selectors.insert(sel(2, 0x2a, 1, false));
    selectors.insert(sel(2, 0x2a, 2, true));
    selectors.insert(sel(2, 0x2b, 1, false));
    for opcode in [0xdc, 0xdd, 0xde, 0xdf] {
        selectors.insert(sel(2, opcode, 1, false));
    }
    for opcode in (0x96..=0x9F).chain(0xA6..=0xAF).chain(0xB6..=0xBF) {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0x52, 0x53, 0x9a, 0x9b, 0xaa, 0xab] {
        selectors.insert(sel(2, opcode, 3, false));
    }
    for opcode in (0x96..=0x9F).chain(0xA6..=0xAF).chain(0xB6..=0xBF) {
        selectors.insert(sel(6, opcode, 1, false));
    }
    for opcode in [0x64, 0x65, 0x66] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    selectors.insert(sel(2, 0x18, 1, false));
    selectors.insert(sel(2, 0x19, 1, false));
    selectors.insert(sel(2, 0x19, 1, true));
    selectors.insert(sel(2, 0x1a, 1, false));
    selectors.insert(sel(2, 0x1a, 1, true));
    selectors.insert(sel(2, 0x1b, 1, false));
    selectors.insert(sel(2, 0x1b, 1, true));
    selectors.insert(sel(2, 0x58, 1, false));
    selectors.insert(sel(2, 0x59, 1, false));
    selectors.insert(sel(2, 0x59, 1, true));
    selectors.insert(sel(2, 0x5a, 1, false));
    selectors.insert(sel(2, 0x5a, 1, true));
    selectors.insert(sel(2, 0x5b, 1, false));
    selectors.insert(sel(2, 0x5b, 1, true));
    selectors.insert(sel(2, 0x78, 1, false));
    selectors.insert(sel(2, 0x79, 1, false));
    selectors.insert(sel(2, 0x29, 1, true));
    selectors.insert(sel(2, 0x37, 1, true));
    for opcode in [0x38, 0x3a, 0x3c, 0x3e] {
        selectors.insert(sel(2, opcode, 1, false));
    }
    selectors.insert(sel(2, 0x38, 2, false));
    selectors.insert(sel(2, 0x38, 2, true));
    selectors.insert(sel(2, 0x39, 2, false));
    selectors.insert(sel(2, 0x39, 2, true));
    selectors.insert(sel(2, 0x3a, 2, false));
    for opcode in [0x39, 0x3b, 0x3d, 0x3f] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    selectors.insert(sel(2, 0x44, 1, false));
    selectors.insert(sel(2, 0x44, 1, true));
    for opcode in [0x45, 0x46, 0x47] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0x62, 0x63] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0x88, 0x89, 0x8a, 0x8b] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0x50, 0x51, 0x52, 0x53] {
        selectors.insert(sel(2, opcode, 1, false));
    }
    selectors.insert(sel(2, 0xb4, 1, true));
    selectors.insert(sel(2, 0xb5, 1, true));
    for opcode in [0x54, 0x55] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    selectors.insert(sel(2, 0xc4, 1, false));
    selectors.insert(sel(2, 0xc4, 1, true));
    selectors.insert(sel(2, 0xcf, 1, false));
    selectors.insert(sel(2, 0x16, 1, false));
    selectors.insert(sel(2, 0x16, 1, true));
    selectors.insert(sel(2, 0x36, 1, false));
    selectors.insert(sel(2, 0x36, 1, true));
    selectors.insert(sel(2, 0x0c, 1, false));
    selectors.insert(sel(2, 0x0d, 1, true));
    for opcode in [0x8d, 0x75, 0x7d, 0x8f] {
        selectors.insert(sel(2, opcode, 1, false));
    }
    selectors.insert(sel(2, 0x83, 1, true));
    selectors.insert(sel(2, 0x8d, 1, true));
    selectors.insert(sel(2, 0x75, 1, true));
    for opcode in [0x76, 0x77, 0x7e, 0x7f] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0x90, 0x91, 0x92, 0x93, 0xa0, 0xa1, 0xa2, 0xa3] {
        selectors.insert(sel(2, opcode, 1, false));
        selectors.insert(sel(2, opcode, 1, true));
    }
    for opcode in [0xc6, 0xc7] {
        for subop in [1, 2, 5, 6] {
            selectors.insert(group_sel(2, opcode, 1, false, subop));
            selectors.insert(group_sel(2, opcode, 1, true, subop));
        }
    }
    selectors.insert(sel(2, 0x7d, 1, true));
    selectors.insert(sel(2, 0x52, 2, false));
    selectors.insert(sel(2, 0x72, 2, false));
    selectors.insert(sel(2, 0x72, 3, false));
    selectors.insert(sel(5, 0x68, 1, false));
    selectors.insert(sel(5, 0x6a, 1, false));
    selectors.insert(sel(5, 0x6d, 1, true));
    selectors.insert(sel(5, 0x6c, 1, true));
    selectors.insert(sel(5, 0x10, 2, false));
    selectors.insert(sel(5, 0x11, 2, false));
    selectors.insert(sel(5, 0x6e, 1, false));
    selectors.insert(sel(5, 0x7e, 1, false));
    for opcode in [0x2a, 0x2c, 0x2d, 0x78, 0x79, 0x7b] {
        selectors.insert(sel(5, opcode, 2, false));
        selectors.insert(sel(5, opcode, 2, true));
    }
    selectors.insert(sel(5, 0x1d, 0, false));
    selectors.insert(sel(5, 0x1d, 1, false));
    for (opcode, pp, w) in [
        (0x5a, 0, false),
        (0x5a, 1, true),
        (0x5b, 0, false),
        (0x5b, 0, true),
        (0x5b, 1, false),
        (0x5b, 2, false),
        (0x78, 0, false),
        (0x78, 1, false),
        (0x79, 0, false),
        (0x79, 1, false),
        (0x7a, 1, false),
        (0x7a, 3, false),
        (0x7a, 3, true),
        (0x7b, 1, false),
        (0x7c, 0, false),
        (0x7c, 1, false),
        (0x7d, 0, false),
        (0x7d, 1, false),
        (0x7d, 2, false),
        (0x7d, 3, false),
    ] {
        selectors.insert(sel(5, opcode, pp, w));
    }
    selectors.insert(sel(5, 0x5a, 2, false));
    selectors.insert(sel(5, 0x5a, 3, true));
    selectors.insert(sel(6, 0x13, 0, false));
    selectors.insert(sel(6, 0x13, 1, false));
    for opcode in [0x2c, 0x2d, 0x42, 0x43, 0x4c, 0x4d, 0x4e, 0x4f] {
        selectors.insert(sel(6, opcode, 1, false));
    }
    selectors.insert(sel(2, 0x50, 3, false));
    selectors.insert(sel(2, 0x51, 3, false));
    selectors.insert(sel(2, 0x50, 2, false));
    selectors.insert(sel(2, 0x51, 2, false));
    selectors.insert(sel(2, 0x50, 0, false));
    selectors.insert(sel(2, 0x51, 0, false));
    for (pp, w) in [(2, false), (1, false), (0, false)] {
        selectors.insert(sel(2, 0xd2, pp, w));
        selectors.insert(sel(2, 0xd3, pp, w));
    }

    selectors.insert(sel(3, 0x03, 1, false));
    selectors.insert(sel(3, 0x03, 1, true));
    selectors.insert(sel(3, 0x00, 1, true));
    selectors.insert(sel(3, 0x01, 1, true));
    for (opcode, pp, w) in [
        (0x08, 0, false),
        (0x08, 1, false),
        (0x09, 1, true),
        (0x0a, 0, false),
        (0x0a, 1, false),
        (0x0b, 1, true),
        (0x26, 0, false),
        (0x26, 1, false),
        (0x26, 1, true),
        (0x27, 0, false),
        (0x27, 1, false),
        (0x27, 1, true),
        (0x50, 1, false),
        (0x50, 1, true),
        (0x51, 1, false),
        (0x51, 1, true),
        (0x54, 1, false),
        (0x54, 1, true),
        (0x55, 1, false),
        (0x55, 1, true),
        (0x56, 0, false),
        (0x56, 1, false),
        (0x56, 1, true),
        (0x57, 0, false),
        (0x57, 1, false),
        (0x57, 1, true),
    ] {
        selectors.insert(sel(3, opcode, pp, w));
    }
    selectors.insert(sel(3, 0x04, 1, false));
    selectors.insert(sel(3, 0x05, 1, true));
    selectors.insert(sel(3, 0x0f, 1, false));
    selectors.insert(sel(3, 0x1d, 1, false));
    selectors.insert(sel(3, 0x14, 1, false));
    selectors.insert(sel(3, 0x15, 1, false));
    selectors.insert(sel(3, 0x16, 1, false));
    selectors.insert(sel(3, 0x16, 1, true));
    selectors.insert(sel(3, 0x17, 1, false));
    selectors.insert(sel(3, 0x20, 1, false));
    selectors.insert(sel(3, 0x21, 1, false));
    selectors.insert(sel(3, 0x22, 1, false));
    selectors.insert(sel(3, 0x22, 1, true));
    selectors.insert(sel(3, 0x23, 1, false));
    selectors.insert(sel(3, 0x23, 1, true));
    selectors.insert(sel(3, 0x43, 1, false));
    selectors.insert(sel(3, 0x43, 1, true));
    for opcode in [0x18, 0x19, 0x1a, 0x1b, 0x38, 0x39, 0x3a, 0x3b] {
        selectors.insert(sel(3, opcode, 1, false));
        selectors.insert(sel(3, opcode, 1, true));
    }
    selectors.insert(sel(3, 0x70, 1, true));
    selectors.insert(sel(3, 0x71, 1, false));
    selectors.insert(sel(3, 0x71, 1, true));
    selectors.insert(sel(3, 0x72, 1, true));
    selectors.insert(sel(3, 0x73, 1, false));
    selectors.insert(sel(3, 0x73, 1, true));
    for opcode in [0x1e, 0x1f, 0x3e, 0x3f] {
        selectors.insert(sel(3, opcode, 1, false));
        selectors.insert(sel(3, opcode, 1, true));
    }
    selectors.insert(sel(3, 0xc2, 0, false));
    selectors.insert(sel(3, 0xc2, 2, false));
    selectors.insert(sel(3, 0x66, 0, false));
    selectors.insert(sel(3, 0x66, 1, false));
    selectors.insert(sel(3, 0x66, 1, true));
    selectors.insert(sel(3, 0x67, 0, false));
    selectors.insert(sel(3, 0x67, 1, false));
    selectors.insert(sel(3, 0x67, 1, true));
    selectors.insert(sel(3, 0x25, 1, false));
    selectors.insert(sel(3, 0x25, 1, true));
    selectors.insert(sel(3, 0x42, 1, false));
    selectors.insert(sel(3, 0x44, 1, false));
    selectors.insert(sel(3, 0x52, 1, false));
    selectors.insert(sel(3, 0x52, 1, true));
    selectors.insert(sel(3, 0x53, 1, false));
    selectors.insert(sel(3, 0x53, 1, true));
    selectors.insert(sel(3, 0xce, 1, true));
    selectors.insert(sel(3, 0xcf, 1, true));

    for opcode in [0x51, 0x58, 0x59, 0x5c, 0x5d, 0x5e, 0x5f] {
        selectors.insert(sel(5, opcode, 0, false));
        selectors.insert(sel(5, opcode, 2, false));
    }
    for opcode in [0x56, 0x57, 0xd6, 0xd7] {
        selectors.insert(sel(6, opcode, 2, false));
        selectors.insert(sel(6, opcode, 3, false));
    }

    selectors
}

fn decode_evex_coverage(case: &DiffCase) -> EvexCoverage {
    assert!(
        case.op.len() >= 6 && case.op[0] == 0x62,
        "{} assembled outside supported EVEX shape: {:02x?}",
        case.label,
        case.op
    );

    let p0 = case.op[1];
    let p1 = case.op[2];
    let p2 = case.op[3];
    let opcode = case.op[4];
    let modrm = case.op[5];
    let form = if (modrm & 0xc0) == 0xc0 {
        OperandForm::Register
    } else {
        OperandForm::Memory
    };
    let map = p0 & 0x7;
    let subop = if (map == 1 && matches!(opcode, 0x71 | 0x72 | 0x73))
        || (map == 2 && matches!(opcode, 0xc6 | 0xc7))
    {
        Some((modrm >> 3) & 0x7)
    } else {
        None
    };
    let selector = EvexSelector {
        map,
        opcode,
        pp: p1 & 0x3,
        w: (p1 & 0x80) != 0,
        subop,
    };
    let rm_reg = if form == OperandForm::Register {
        let rm = modrm & 0x7;
        let b_set = (p0 & 0x20) != 0;
        let x_set = (p0 & 0x40) != 0;
        let high_ext = if selector_uses_gpr_rm_bucket(selector) {
            0
        } else if x_set {
            0
        } else {
            16
        };
        Some(rm + if b_set { 0 } else { 8 } + high_ext)
    } else {
        None
    };
    let mask = match (p2 & 0x7, (p2 & 0x80) != 0) {
        (0, _) => MaskMode::None,
        (_, false) => MaskMode::Merge,
        (_, true) => MaskMode::Zero,
    };

    EvexCoverage {
        selector,
        form,
        ll: (p2 >> 5) & 0x3,
        mask,
        rm_reg,
    }
}

fn format_debug_set<T: std::fmt::Debug>(set: &BTreeSet<T>) -> String {
    set.iter()
        .map(|item| format!("{item:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn expected_vector_lengths(selector: EvexSelector) -> BTreeSet<u8> {
    if (selector.map == 1
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x10, 2, false)
                | (0x10, 3, true)
                | (0x11, 2, false)
                | (0x11, 3, true)
                | (0x12, 0, false)
                | (0x12, 1, true)
                | (0x16, 0, false)
                | (0x16, 1, true)
                | (0x2a, 2, false)
                | (0x2a, 2, true)
                | (0x2a, 3, false)
                | (0x2a, 3, true)
                | (0x2c, 2, false)
                | (0x2c, 2, true)
                | (0x2c, 3, false)
                | (0x2c, 3, true)
                | (0x2d, 2, false)
                | (0x2d, 2, true)
                | (0x2d, 3, false)
                | (0x2d, 3, true)
                | (0x2e, 0, false)
                | (0x2e, 1, true)
                | (0x2f, 0, false)
                | (0x2f, 1, true)
                | (0x5a, 2, false)
                | (0x5a, 3, true)
                | (0x6e, 1, false)
                | (0x6e, 1, true)
                | (0x7e, 1, false)
                | (0x7e, 1, true)
                | (0x7e, 2, true)
                | (0x78, 2, false)
                | (0x78, 2, true)
                | (0x78, 3, false)
                | (0x78, 3, true)
                | (0x79, 2, false)
                | (0x79, 2, true)
                | (0x79, 3, false)
                | (0x79, 3, true)
                | (0x7b, 2, false)
                | (0x7b, 2, true)
                | (0x7b, 3, false)
                | (0x7b, 3, true)
                | (0xd6, 1, true)
                | (0xc2, 2, false)
                | (0xc2, 3, true)
                | (0xc4, 1, false)
                | (0xc5, 1, false)
        ))
        || (selector.map == 3
            && matches!(
                (selector.opcode, selector.pp, selector.w),
                (0x14, 1, false)
                    | (0x15, 1, false)
                    | (0x16, 1, false)
                    | (0x16, 1, true)
                    | (0x17, 1, false)
                    | (0x20, 1, false)
                    | (0x21, 1, false)
                    | (0x22, 1, false)
                    | (0x22, 1, true)
                    | (0xc2, 2, false)
                    | (0x67, 0, false)
                    | (0x67, 1, false)
                    | (0x67, 1, true)
            ))
        || (selector.map == 5
            && matches!(
                (selector.opcode, selector.pp, selector.w),
                (0x10, 2, false)
                    | (0x11, 2, false)
                    | (0x1d, 0, false)
                    | (0x2a, 2, false)
                    | (0x2a, 2, true)
                    | (0x2c, 2, false)
                    | (0x2c, 2, true)
                    | (0x2d, 2, false)
                    | (0x2d, 2, true)
                    | (0x5a, 2, false)
                    | (0x5a, 3, true)
                    | (0x6e, 1, false)
                    | (0x7e, 1, false)
                    | (0x78, 2, false)
                    | (0x78, 2, true)
                    | (0x79, 2, false)
                    | (0x79, 2, true)
                    | (0x7b, 2, false)
                    | (0x7b, 2, true)
                    | (0x51, 2, false)
                    | (0x58, 2, false)
                    | (0x59, 2, false)
                    | (0x5c, 2, false)
                    | (0x5d, 2, false)
                    | (0x5e, 2, false)
                    | (0x5f, 2, false)
            ))
        || selector == sel(6, 0x13, 0, false)
    {
        return BTreeSet::from([0]);
    }
    if selector.map == 1
        && matches!(
            selector.opcode,
            0x51 | 0x58 | 0x59 | 0x5c | 0x5d | 0x5e | 0x5f
        )
        && matches!((selector.pp, selector.w), (2, false) | (3, true))
    {
        return BTreeSet::new();
    }
    if selector.map == 3 && selector.opcode == 0x53 {
        return BTreeSet::new();
    }
    if selector.map == 2 && selector.pp == 3 && matches!(selector.opcode, 0x52 | 0x53 | 0x9a | 0xaa)
    {
        return BTreeSet::from([2]);
    }
    if selector.map == 2 && selector.pp == 3 && matches!(selector.opcode, 0x9b | 0xab) {
        return BTreeSet::new();
    }
    if (selector.map == 2 || selector.map == 6)
        && selector.pp == 1
        && is_evex_fma_opcode(selector.opcode)
        && is_evex_scalar_fma_opcode(selector.opcode)
    {
        return BTreeSet::new();
    }
    if selector == sel(2, 0x19, 1, false)
        || selector == sel(2, 0x19, 1, true)
        || selector == sel(2, 0x1a, 1, false)
        || selector == sel(2, 0x1a, 1, true)
        || selector == sel(2, 0x5a, 1, false)
        || selector == sel(2, 0x5a, 1, true)
    {
        return BTreeSet::from([1, 2]);
    }
    if selector == sel(2, 0x1b, 1, false)
        || selector == sel(2, 0x1b, 1, true)
        || selector == sel(2, 0x5b, 1, false)
        || selector == sel(2, 0x5b, 1, true)
    {
        return BTreeSet::from([2]);
    }
    if (selector.map == 2 && matches!((selector.opcode, selector.pp), (0x16, 1) | (0x36, 1)))
        || selector == sel(3, 0x00, 1, true)
        || selector == sel(3, 0x01, 1, true)
    {
        return BTreeSet::from([1, 2]);
    }
    if selector.map == 3 && matches!(selector.opcode, 0x18 | 0x19 | 0x38 | 0x39) {
        return BTreeSet::from([1, 2]);
    }
    if selector.map == 3 && matches!(selector.opcode, 0x23 | 0x43) {
        return BTreeSet::from([1, 2]);
    }
    if selector.map == 3 && matches!(selector.opcode, 0x1a | 0x1b | 0x3a | 0x3b) {
        return BTreeSet::from([2]);
    }
    if selector.map == 2 && selector.pp == 1 && matches!(selector.opcode, 0xc8 | 0xca | 0xcc) {
        return BTreeSet::from([2]);
    }
    if selector.map == 2
        && selector.pp == 1
        && matches!(selector.opcode, 0xc6 | 0xc7)
        && selector.subop.is_some()
    {
        return BTreeSet::from([2]);
    }
    if (selector.map == 2
        && selector.pp == 1
        && matches!(selector.opcode, 0x2d | 0x43 | 0x4d | 0x4f | 0xcb | 0xcd))
        || (selector.map == 3
            && matches!(
                (selector.opcode, selector.pp, selector.w),
                (0x0a, 0, false)
                    | (0x0a, 1, false)
                    | (0x0b, 1, true)
                    | (0x27, 0, false)
                    | (0x27, 1, false)
                    | (0x27, 1, true)
                    | (0x51, 1, false)
                    | (0x51, 1, true)
                    | (0x55, 1, false)
                    | (0x55, 1, true)
                    | (0x57, 0, false)
                    | (0x57, 1, false)
                    | (0x57, 1, true)
            ))
        || (selector.map == 6
            && matches!(selector.pp, 1 | 2 | 3)
            && !selector.w
            && matches!(selector.opcode, 0x2d | 0x43 | 0x4d | 0x4f | 0x57 | 0xd7))
    {
        return BTreeSet::new();
    }
    BTreeSet::from([0, 1, 2])
}

fn expected_operand_forms(selector: EvexSelector) -> BTreeSet<OperandForm> {
    if selector == sel(1, 0x6e, 1, true) || selector == sel(1, 0x7e, 1, true) {
        return BTreeSet::from([OperandForm::Register]);
    }
    if selector == sel(1, 0xc5, 1, false) {
        return BTreeSet::from([OperandForm::Register]);
    }
    if (selector.map == 1
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x11, 2, false) | (0x11, 3, true) | (0x12, 1, true) | (0x16, 1, true)
        ))
        || selector == sel(5, 0x11, 2, false)
    {
        return BTreeSet::from([OperandForm::Memory]);
    }
    if (selector.map == 1
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x2b, 0, false) | (0x2b, 1, true) | (0xe7, 1, false)
        ))
        || selector == sel(2, 0x2a, 1, false)
    {
        return BTreeSet::from([OperandForm::Memory]);
    }
    if selector.map == 2 && selector.pp == 1 && matches!(selector.opcode, 0x1a | 0x1b | 0x5a | 0x5b)
    {
        return BTreeSet::from([OperandForm::Memory]);
    }
    if selector.map == 2
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x28, 2, false)
                | (0x28, 2, true)
                | (0x29, 2, false)
                | (0x29, 2, true)
                | (0x2a, 2, true)
                | (0x38, 2, false)
                | (0x38, 2, true)
                | (0x39, 2, false)
                | (0x39, 2, true)
                | (0x3a, 2, false)
        )
    {
        BTreeSet::from([OperandForm::Register])
    } else if selector.map == 2
        && selector.pp == 1
        && (matches!(selector.opcode, 0x90..=0x93 | 0xa0..=0xa3)
            || (matches!(selector.opcode, 0xc6 | 0xc7) && selector.subop.is_some()))
    {
        BTreeSet::from([OperandForm::Memory])
    } else if selector.map == 2
        && selector.pp == 3
        && matches!(selector.opcode, 0x52 | 0x53 | 0x9a | 0x9b | 0xaa | 0xab)
    {
        BTreeSet::from([OperandForm::Memory])
    } else {
        BTreeSet::from([OperandForm::Register, OperandForm::Memory])
    }
}

fn required_rm_register_buckets(selector: EvexSelector) -> BTreeSet<u8> {
    if (selector.map == 1
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x11, 2, false) | (0x11, 3, true) | (0x12, 1, true) | (0x16, 1, true)
        ))
        || selector == sel(5, 0x11, 2, false)
    {
        return BTreeSet::new();
    }
    if (selector.map == 1
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x6e, 1, false) | (0x6e, 1, true) | (0x7e, 1, false) | (0x7e, 1, true)
        ))
        || (selector.map == 1
            && matches!(
                (selector.opcode, selector.pp),
                (0x2a, 2 | 3) | (0x7b, 2 | 3)
            ))
        || (selector.map == 5
            && matches!(
                (selector.opcode, selector.pp, selector.w),
                (0x6e, 1, false) | (0x7e, 1, false)
            ))
        || (selector.map == 5 && matches!((selector.opcode, selector.pp), (0x2a, 2) | (0x7b, 2)))
    {
        return BTreeSet::from([0, 8]);
    }
    if matches!(
        selector,
        EvexSelector {
            map: 1,
            opcode: 0xc4 | 0xc5,
            pp: 1,
            w: false,
            subop: None,
        } | EvexSelector {
            map: 3,
            opcode: 0x14 | 0x15 | 0x17 | 0x20,
            pp: 1,
            w: false,
            subop: None,
        } | EvexSelector {
            map: 3,
            opcode: 0x16 | 0x22,
            pp: 1,
            subop: None,
            ..
        }
    ) {
        return BTreeSet::from([0, 8]);
    }
    if (selector.map == 1
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x2b, 0, false) | (0x2b, 1, true) | (0xe7, 1, false)
        ))
        || selector == sel(2, 0x2a, 1, false)
    {
        return BTreeSet::new();
    }
    if selector.map == 2 && selector.pp == 1 && matches!(selector.opcode, 0x1a | 0x1b | 0x5a | 0x5b)
    {
        return BTreeSet::new();
    }
    if selector.map == 2
        && selector.pp == 1
        && (matches!(selector.opcode, 0x90..=0x93 | 0xa0..=0xa3)
            || (matches!(selector.opcode, 0xc6 | 0xc7) && selector.subop.is_some()))
    {
        return BTreeSet::new();
    }
    if selector.map == 2
        && selector.pp == 3
        && matches!(selector.opcode, 0x52 | 0x53 | 0x9a | 0x9b | 0xaa | 0xab)
    {
        return BTreeSet::new();
    }
    if selector.map == 2
        && matches!(
            (selector.opcode, selector.pp, selector.w),
            (0x28, 2, false)
                | (0x28, 2, true)
                | (0x2a, 2, true)
                | (0x38, 2, false)
                | (0x38, 2, true)
                | (0x3a, 2, false)
        )
    {
        BTreeSet::from([0])
    } else {
        BTreeSet::from([0, 8, 16, 24])
    }
}

fn assert_evex_form_coverage(cases: &[DiffCase]) {
    let coverage = cases.iter().map(decode_evex_coverage).collect::<Vec<_>>();
    let actual_selectors = coverage
        .iter()
        .map(|coverage| coverage.selector)
        .collect::<BTreeSet<_>>();
    let expected_selectors = expected_dispatch_selectors();
    let missing = expected_selectors
        .difference(&actual_selectors)
        .cloned()
        .collect::<BTreeSet<_>>();
    let unexpected = actual_selectors
        .difference(&expected_selectors)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "EVEX dispatch selector coverage mismatch\nmissing:\n{}\nunexpected:\n{}",
        format_debug_set(&missing),
        format_debug_set(&unexpected)
    );

    let mut by_selector = BTreeMap::<EvexSelector, Vec<&EvexCoverage>>::new();
    for item in &coverage {
        by_selector.entry(item.selector).or_default().push(item);
    }

    for selector in expected_selectors {
        let items = by_selector
            .get(&selector)
            .unwrap_or_else(|| panic!("missing EVEX selector coverage for {selector:?}"));
        let forms = items.iter().map(|item| item.form).collect::<BTreeSet<_>>();
        assert_eq!(
            forms,
            expected_operand_forms(selector),
            "{selector:?} must cover its valid operand forms"
        );

        let expected_lls = expected_vector_lengths(selector);
        if !expected_lls.is_empty() {
            let lls = items.iter().map(|item| item.ll).collect::<BTreeSet<_>>();
            assert!(
                expected_lls.is_subset(&lls),
                "{selector:?} must cover 128/256/512-bit vector lengths"
            );
        }

        let rm_regs = items
            .iter()
            .filter_map(|item| item.rm_reg)
            .collect::<BTreeSet<_>>();
        let required_rm_regs = required_rm_register_buckets(selector);
        assert!(
            required_rm_regs.is_subset(&rm_regs),
            "{selector:?} must cover its required r/m register buckets"
        );
    }

    assert!(
        coverage.iter().any(|item| item.mask == MaskMode::Merge),
        "EVEX corpus must include merge-masked forms"
    );
    assert!(
        coverage.iter().any(|item| item.mask == MaskMode::Zero),
        "EVEX corpus must include zero-masked forms"
    );
}

fn add_ternary_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    suffixes: &[&str],
) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
        }
    }
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} (%rax), %zmm{src1}, %zmm{dst}"),
                profile,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src2}, {src1}, {dst}"),
            profile,
        );
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %zmm18, %zmm17 {suffix}"),
            profile,
        );
    }
}

fn add_scalar_fp_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    suffixes: &[&str],
) {
    for dst in [0u8, 9, 17, 25] {
        for src1 in [1u8, 10, 18, 26] {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_xmm{dst}_src1_xmm{src1}_rm_xmm{rm}"),
                    format!("{mnemonic} %xmm{rm}, %xmm{src1}, %xmm{dst} {{%k1}}"),
                    profile,
                );
            }
        }
    }
    for dst in [0u8, 17] {
        for src1 in [1u8, 18] {
            spec(
                specs,
                format!("{mnemonic}_dst_xmm{dst}_src1_xmm{src1}_mem"),
                format!("{mnemonic} (%rax), %xmm{src1}, %xmm{dst} {{%k1}}"),
                profile,
            );
        }
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_xmm17_xmm18_xmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %xmm16, %xmm18, %xmm17 {suffix}"),
            profile,
        );
    }
}

fn add_fma_packed_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    bcst: &str,
    suffixes: &[&str],
) {
    add_ternary_family(specs, mnemonic, profile, suffixes);
    spec(
        specs,
        format!("{mnemonic}_zmm_mem_broadcast"),
        format!("{mnemonic} (%rax){{{bcst}}}, %zmm18, %zmm17 {{%k1}}"),
        profile,
    );
}

fn add_unary_rm_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    suffixes: &[&str],
) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
    }
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            profile,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%ymm1", "%ymm16"),
        ("zmm", "%zmm1", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src}, {dst}"),
            profile,
        );
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %zmm17 {suffix}"),
            profile,
        );
    }
}

fn add_unary_zmm_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    suffixes: &[&str],
) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
    }
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            profile,
        );
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %zmm17 {suffix}"),
            profile,
        );
    }
}

fn add_unary_imm_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    imm: u8,
    suffixes: &[&str],
) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} ${imm}, %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
    }
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} ${imm}, (%rax), %zmm{dst}"),
            profile,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%ymm1", "%ymm16"),
        ("zmm", "%zmm1", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} ${imm}, {src}, {dst}"),
            profile,
        );
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} ${imm}, %zmm16, %zmm17 {suffix}"),
            profile,
        );
    }
}

fn add_scalar_fp_imm_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    imm: u8,
    suffixes: &[&str],
) {
    for dst in [0u8, 9, 17, 25] {
        for src1 in [1u8, 10, 18, 26] {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_xmm{dst}_src1_xmm{src1}_rm_xmm{rm}"),
                    format!("{mnemonic} ${imm}, %xmm{rm}, %xmm{src1}, %xmm{dst} {{%k1}}"),
                    profile,
                );
            }
        }
    }
    for dst in [0u8, 17] {
        for src1 in [1u8, 18] {
            spec(
                specs,
                format!("{mnemonic}_dst_xmm{dst}_src1_xmm{src1}_mem"),
                format!("{mnemonic} ${imm}, (%rax), %xmm{src1}, %xmm{dst} {{%k1}}"),
                profile,
            );
        }
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_xmm17_xmm18_xmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} ${imm}, %xmm16, %xmm18, %xmm17 {suffix}"),
            profile,
        );
    }
}

fn add_ternary_imm_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    imm: u8,
    suffixes: &[&str],
) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} ${imm}, %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
        }
    }
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} ${imm}, (%rax), %zmm{src1}, %zmm{dst}"),
                profile,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} ${imm}, {src2}, {src1}, {dst}"),
            profile,
        );
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} ${imm}, %zmm16, %zmm18, %zmm17 {suffix}"),
            profile,
        );
    }
}

fn add_shufp_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, profile: InputProfile, bcst: &str) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} $0x1b, %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} $0x4e, (%rax), %zmm{src1}, %zmm{dst}"),
                profile,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} $0x1b, {src2}, {src1}, {dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $0x1b, %zmm16, %zmm18, %zmm17 {suffix}"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_zmm_mem_broadcast"),
        format!("{mnemonic} $0x1b, (%rax){{{bcst}}}, %zmm18, %zmm17 {{%k1}}"),
        profile,
    );
}

fn add_shuffle_128_lane_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    bcst: &str,
) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} $0x1b, %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} $0x4e, (%rax), %zmm{src1}, %zmm{dst}"),
                profile,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} $0x1b, {src2}, {src1}, {dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $0x1b, %zmm16, %zmm18, %zmm17 {suffix}"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_zmm_mem_broadcast"),
        format!("{mnemonic} $0x1b, (%rax){{{bcst}}}, %zmm18, %zmm17 {{%k1}}"),
        profile,
    );
}

fn add_permute_var_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    allow_128: bool,
    bcst: Option<&str>,
) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_idx_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_idx_zmm{src1}_mem"),
                format!("{mnemonic} (%rax), %zmm{src1}, %zmm{dst}"),
                profile,
            );
        }
    }

    let vl_cases: &[(&str, &str, &str, &str)] = if allow_128 {
        &[
            ("xmm", "%xmm1", "%xmm2", "%xmm16"),
            ("ymm", "%ymm1", "%ymm2", "%ymm16"),
            ("zmm", "%zmm1", "%zmm2", "%zmm16"),
        ]
    } else {
        &[
            ("ymm", "%ymm1", "%ymm2", "%ymm16"),
            ("zmm", "%zmm1", "%zmm2", "%zmm16"),
        ]
    };
    for &(reg_class, dst, src1, src2) in vl_cases {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src2}, {src1}, {dst}"),
            profile,
        );
    }

    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %zmm18, %zmm17 {suffix}"),
            profile,
        );
    }

    if let Some(bcst) = bcst {
        spec(
            specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{{bcst}}}, %zmm18, %zmm17 {{%k1}}"),
            profile,
        );
    }
}

fn add_two_table_permute_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    bcst: Option<&str>,
) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} (%rax), %zmm{src1}, %zmm{dst}"),
                profile,
            );
        }
    }

    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src2}, {src1}, {dst}"),
            profile,
        );
    }

    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %zmm18, %zmm17 {suffix}"),
            profile,
        );
    }

    if let Some(bcst) = bcst {
        spec(
            specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{{bcst}}}, %zmm18, %zmm17 {{%k1}}"),
            profile,
        );
    }
}

fn add_permute_qword_imm_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, profile: InputProfile) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_imm_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} $0x1b, %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_imm_dst_zmm{dst}_mem"),
            format!("{mnemonic} $0x4e, (%rax), %zmm{dst}"),
            profile,
        );
    }
    for (reg_class, dst, src) in [("ymm", "%ymm1", "%ymm16"), ("zmm", "%zmm1", "%zmm16")] {
        spec(
            specs,
            format!("{mnemonic}_imm_{reg_class}_vl"),
            format!("{mnemonic} $0x1b, {src}, {dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_imm_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $0x1b, %zmm16, %zmm17 {suffix}"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_imm_zmm_mem_broadcast"),
        format!("{mnemonic} $0x1b, (%rax){{1to8}}, %zmm17 {{%k1}}"),
        profile,
    );
}

fn add_permil_imm_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    bcst: &str,
) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_imm_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} $0x1b, %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_imm_dst_zmm{dst}_mem"),
            format!("{mnemonic} $0x4e, (%rax), %zmm{dst}"),
            profile,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%ymm1", "%ymm16"),
        ("zmm", "%zmm1", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_imm_{reg_class}_vl"),
            format!("{mnemonic} $0x1b, {src}, {dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_imm_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $0x1b, %zmm16, %zmm17 {suffix}"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_imm_zmm_mem_broadcast"),
        format!("{mnemonic} $0x1b, (%rax){{{bcst}}}, %zmm17 {{%k1}}"),
        profile,
    );
}

fn add_gf2p8_affine_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} $0x63, %zmm{rm}, %zmm18, %zmm{dst}"),
                InputProfile::Int,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} $0x1b, (%rax), %zmm18, %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm18", "%xmm16"),
        ("ymm", "%ymm1", "%ymm18", "%ymm16"),
        ("zmm", "%zmm1", "%zmm18", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} $0x63, {src2}, {src1}, {dst}"),
            InputProfile::Int,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $0x63, %zmm16, %zmm18, %zmm17 {suffix}"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_zmm_mem_broadcast"),
        format!("{mnemonic} $0x63, (%rax){{1to8}}, %zmm18, %zmm17 {{%k1}}"),
        InputProfile::Int,
    );
}

fn add_vp2intersect_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    bcst: &str,
) {
    for reg_class in ["xmm", "ymm", "zmm"] {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_{reg_class}17_rm_{reg_class}{rm}"),
                format!("{mnemonic} %{reg_class}{rm}, %{reg_class}17, %k2"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_{reg_class}17_mem"),
            format!("{mnemonic} (%rax), %{reg_class}17, %k2"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_zmm17_mem_broadcast"),
        format!("{mnemonic} (%rax){{{bcst}}}, %zmm17, %k2"),
        profile,
    );
}

fn add_move_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, profile: InputProfile) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_load_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
    }
    for src in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_store_src_zmm{src}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{src}, %zmm{rm}"),
                profile,
            );
        }
    }
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_load_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            profile,
        );
    }
    for src in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_store_src_zmm{src}_mem"),
            format!("{mnemonic} %zmm{src}, (%rax)"),
            profile,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%ymm1", "%ymm16"),
        ("zmm", "%zmm1", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src}, {dst}"),
            profile,
        );
    }
}

fn raw_evex_reg_store(map: u8, opcode: u8, p1: u8, ll: u8, dst: usize, src: usize) -> Vec<u8> {
    let mut p0 = map;
    if src & 0x08 == 0 {
        p0 |= 0x80;
    }
    if dst & 0x10 == 0 {
        p0 |= 0x40;
    }
    if dst & 0x08 == 0 {
        p0 |= 0x20;
    }
    if src & 0x10 == 0 {
        p0 |= 0x10;
    }

    vec![
        0x62,
        p0,
        p1,
        0x08 | (ll << 5),
        opcode,
        0xc0 | (((src & 0x7) as u8) << 3) | (dst & 0x7) as u8,
    ]
}

fn add_raw_move_store_reg_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    opcode: u8,
    p1: u8,
    profile: InputProfile,
) {
    for rm in RM_EXT_REGS {
        spec_raw(
            specs,
            format!("{mnemonic}_raw_store_reg_dst_zmm{rm}_src_zmm1"),
            format!("{mnemonic} %zmm1, %zmm{rm}"),
            raw_evex_reg_store(1, opcode, p1, 2, rm, 1),
            profile,
        );
    }
    for (reg_class, ll, dst, src) in [("xmm", 0, 16, 1), ("ymm", 1, 16, 1), ("zmm", 2, 16, 1)] {
        spec_raw(
            specs,
            format!("{mnemonic}_raw_store_reg_{reg_class}_vl"),
            format!("{mnemonic} %{reg_class}{src}, %{reg_class}{dst}"),
            raw_evex_reg_store(1, opcode, p1, ll, dst, src),
            profile,
        );
    }
}

fn add_nt_store_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, profile: InputProfile) {
    for src in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_src_zmm{src}_mem"),
            format!("{mnemonic} %zmm{src}, (%rax)"),
            profile,
        );
    }
    for reg_class in ["xmm", "ymm", "zmm"] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} %{reg_class}16, (%rax)"),
            profile,
        );
    }
}

fn add_nt_load_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for reg_class in ["xmm", "ymm", "zmm"] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} (%rax), %{reg_class}17"),
            InputProfile::Int,
        );
    }
}

fn add_gpr_scalar_move_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    low_gpr: &str,
    high_gpr: &str,
) {
    for (label, gpr) in [("rax", low_gpr), ("r8", high_gpr)] {
        spec(
            specs,
            format!("{mnemonic}_gpr_{label}_to_xmm17"),
            format!("{mnemonic} {gpr}, %xmm17"),
            InputProfile::Int,
        );
        spec(
            specs,
            format!("{mnemonic}_xmm17_to_gpr_{label}"),
            format!("{mnemonic} %xmm17, {gpr}"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mem_to_xmm17"),
        format!("{mnemonic} (%rax), %xmm17"),
        InputProfile::Int,
    );
    spec(
        specs,
        format!("{mnemonic}_xmm17_to_mem"),
        format!("{mnemonic} %xmm17, (%rax)"),
        InputProfile::Int,
    );
}

fn add_movq_vec_family(specs: &mut Vec<CaseSpec>) {
    for rm in RM_EXT_REGS {
        spec(
            specs,
            format!("vmovq_xmm{rm}_to_xmm17"),
            format!("vmovq %xmm{rm}, %xmm17"),
            InputProfile::Int,
        );
        spec_raw(
            specs,
            format!("vmovq_raw_store_reg_xmm17_to_xmm{rm}"),
            format!("vmovq %xmm17, %xmm{rm}"),
            raw_evex_reg_store(1, 0xd6, 0xfd, 0, rm, 17),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        "vmovq_xmm_vl".to_string(),
        "vmovq %xmm16, %xmm17".to_string(),
        InputProfile::Int,
    );
    spec_raw(
        specs,
        "vmovq_raw_store_reg_xmm_vl".to_string(),
        "vmovq %xmm17, %xmm16".to_string(),
        raw_evex_reg_store(1, 0xd6, 0xfd, 0, 16, 17),
        InputProfile::Int,
    );
}

fn add_scalar_move_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, profile: InputProfile) {
    for rm in RM_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_reg_xmm{rm}_xmm18_xmm17"),
            format!("{mnemonic} %xmm{rm}, %xmm18, %xmm17"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_load_mem_xmm17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} (%rax), %xmm17 {suffix}"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_store_xmm17_mem"),
        format!("{mnemonic} %xmm17, (%rax)"),
        profile,
    );
    spec(
        specs,
        format!("{mnemonic}_store_xmm17_mem_k1"),
        format!("{mnemonic} %xmm17, (%rax) {{%k1}}"),
        profile,
    );
}

fn add_ps_high_low_move_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    include_reg: bool,
    include_mem: bool,
) {
    if include_reg {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_reg_xmm{rm}_xmm18_xmm17"),
                format!("{mnemonic} %xmm{rm}, %xmm18, %xmm17"),
                InputProfile::F32,
            );
        }
    }
    if include_mem {
        spec(
            specs,
            format!("{mnemonic}_mem_xmm18_xmm17"),
            format!("{mnemonic} (%rax), %xmm18, %xmm17"),
            InputProfile::F32,
        );
    }
}

fn add_pd_high_low_move_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    spec(
        specs,
        format!("{mnemonic}_mem_xmm18_xmm17"),
        format!("{mnemonic} (%rax), %xmm18, %xmm17"),
        InputProfile::F64,
    );
}

fn add_duplicate_move_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, profile: InputProfile) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm{dst}"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            profile,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%ymm1", "%ymm16"),
        ("zmm", "%zmm1", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src}, {dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %zmm17 {suffix}"),
            profile,
        );
    }
}

fn add_compare_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for src1 in SRC1_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_src1_zmm{src1}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm{src1}, %k3"),
                InputProfile::Int,
            );
        }
    }
    for src1 in SRC1_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_src1_zmm{src1}_mem"),
            format!("{mnemonic} (%rax), %zmm{src1}, %k3"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mask_zmm18_zmm16"),
        format!("{mnemonic} %zmm16, %zmm18, %k3 {{%k1}}"),
        InputProfile::Int,
    );
    for (reg_class, src1, src2) in [
        ("xmm", "%xmm18", "%xmm16"),
        ("ymm", "%ymm18", "%ymm16"),
        ("zmm", "%zmm18", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src2}, {src1}, %k3"),
            InputProfile::Int,
        );
    }
}

fn add_vpcmp_imm_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for src1 in SRC1_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_src1_zmm{src1}_rm_zmm{rm}"),
                format!("{mnemonic} $4, %zmm{rm}, %zmm{src1}, %k3"),
                InputProfile::Int,
            );
        }
    }
    for src1 in SRC1_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_src1_zmm{src1}_mem"),
            format!("{mnemonic} $0, (%rax), %zmm{src1}, %k3"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mask_zmm18_zmm16"),
        format!("{mnemonic} $0, %zmm16, %zmm18, %k3 {{%k1}}"),
        InputProfile::Int,
    );
    for (reg_class, src1, src2) in [
        ("xmm", "%xmm18", "%xmm16"),
        ("ymm", "%ymm18", "%ymm16"),
        ("zmm", "%zmm18", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} $4, {src2}, {src1}, %k3"),
            InputProfile::Int,
        );
    }
}

fn add_fp_compare_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    elem_size: usize,
    scalar: bool,
) {
    let reg_class = if scalar { "xmm" } else { "zmm" };
    for src1 in SRC1_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_src1_{reg_class}{src1}_rm_{reg_class}{rm}"),
                format!("{mnemonic} $4, %{reg_class}{rm}, %{reg_class}{src1}, %k3"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_src1_{reg_class}{src1}_mem"),
            format!("{mnemonic} $0, (%rax), %{reg_class}{src1}, %k3"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mask_{reg_class}18_{reg_class}16"),
        format!("{mnemonic} $0, %{reg_class}16, %{reg_class}18, %k3 {{%k1}}"),
        profile,
    );
    if !scalar {
        let broadcast = 64 / elem_size;
        spec(
            specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} $4, (%rax){{1to{broadcast}}}, %zmm18, %k3"),
            profile,
        );
        for reg_class in ["xmm", "ymm", "zmm"] {
            spec(
                specs,
                format!("{mnemonic}_{reg_class}_vl"),
                format!("{mnemonic} $4, %{reg_class}16, %{reg_class}18, %k3"),
                profile,
            );
        }
    }
}

fn add_fpclass_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    elem_size: usize,
    scalar: bool,
) {
    let reg_class = if scalar { "xmm" } else { "zmm" };
    for rm in RM_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}{rm}"),
            format!("{mnemonic} $4, %{reg_class}{rm}, %k3"),
            profile,
        );
    }
    if scalar {
        spec(
            specs,
            format!("{mnemonic}_mem"),
            format!("{mnemonic} $0, (%rax), %k3"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mask_{reg_class}16"),
        format!("{mnemonic} $4, %{reg_class}16, %k3 {{%k1}}"),
        profile,
    );
    if !scalar {
        let broadcast = 64 / elem_size;
        spec(
            specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} $4, (%rax){{1to{broadcast}}}, %k3"),
            profile,
        );
        for reg_class in ["xmm", "ymm", "zmm"] {
            spec(
                specs,
                format!("{mnemonic}_{reg_class}_vl"),
                format!("{mnemonic} $4, %{reg_class}16, %k3"),
                profile,
            );
        }
    }
}

fn add_align_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} $3, %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} $3, (%rax), %zmm{src1}, %zmm{dst}"),
                InputProfile::Int,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} $1, {src2}, {src1}, {dst}"),
            InputProfile::Int,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $2, %zmm16, %zmm18, %zmm17 {suffix}"),
            InputProfile::Int,
        );
    }
}

fn add_vpclmulqdq_family(specs: &mut Vec<CaseSpec>) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            for imm in [0x00u8, 0x01, 0x10, 0x11] {
                spec(
                    specs,
                    format!("vpclmulqdq_imm{imm:02x}_dst_zmm{dst}_rm_zmm{rm}"),
                    format!("vpclmulqdq ${imm:#x}, %zmm{rm}, %zmm18, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
        }
        spec(
            specs,
            format!("vpclmulqdq_dst_zmm{dst}_mem"),
            format!("vpclmulqdq $0x11, (%rax), %zmm18, %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm18", "%xmm16"),
        ("ymm", "%ymm1", "%ymm18", "%ymm16"),
        ("zmm", "%zmm1", "%zmm18", "%zmm16"),
    ] {
        spec(
            specs,
            format!("vpclmulqdq_{reg_class}_vl"),
            format!("vpclmulqdq $0x11, {src2}, {src1}, {dst}"),
            InputProfile::Int,
        );
    }
}

fn add_vaes_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm18, %zmm{dst}"),
                InputProfile::Int,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm18, %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm18", "%xmm16"),
        ("ymm", "%ymm1", "%ymm18", "%ymm16"),
        ("zmm", "%zmm1", "%zmm18", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src2}, {src1}, {dst}"),
            InputProfile::Int,
        );
    }
}

fn add_rotate_imm_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_imm_dst_zmm{dst}_rm_zmm{rm}"),
                format!("{mnemonic} $5, %zmm{rm}, %zmm{dst}"),
                InputProfile::Int,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_imm_dst_zmm{dst}_mem"),
            format!("{mnemonic} $5, (%rax), %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%ymm1", "%ymm16"),
        ("zmm", "%zmm1", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_imm_{reg_class}_vl"),
            format!("{mnemonic} $5, {src}, {dst}"),
            InputProfile::Int,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_imm_mask_zmm17_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $5, %zmm16, %zmm17 {suffix}"),
            InputProfile::Int,
        );
    }
}

fn add_vpternlog_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("{mnemonic} $0x96, %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("{mnemonic} $0xe2, (%rax), %zmm{src1}, %zmm{dst}"),
                InputProfile::Int,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} $0xe4, {src2}, {src1}, {dst}"),
            InputProfile::Int,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm18_zmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $0x96, %zmm16, %zmm18, %zmm17 {suffix}"),
            InputProfile::Int,
        );
    }
}

fn add_test_mask_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for src1 in SRC1_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_src1_zmm{src1}_rm_zmm{rm}"),
                format!("{mnemonic} %zmm{rm}, %zmm{src1}, %k3"),
                InputProfile::Int,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_src1_zmm{src1}_mem"),
            format!("{mnemonic} (%rax), %zmm{src1}, %k3"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mask_zmm18_zmm16"),
        format!("{mnemonic} %zmm16, %zmm18, %k3 {{%k1}}"),
        InputProfile::Int,
    );
    for (reg_class, src1, src2) in [
        ("xmm", "%xmm18", "%xmm16"),
        ("ymm", "%ymm18", "%ymm16"),
        ("zmm", "%zmm18", "%zmm16"),
    ] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src2}, {src1}, %k3"),
            InputProfile::Int,
        );
    }
}

fn add_broadcast_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, src_reg: &str) {
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_{src_reg}{rm}"),
                format!("{mnemonic} %{src_reg}{rm}, %zmm{dst}"),
                InputProfile::Int,
            );
        }
    }
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mask_zmm17_{src_reg}16"),
        format!("{mnemonic} %{src_reg}16, %zmm17 {{%k1}}"),
        InputProfile::Int,
    );
    let widths: &[(&str, &str)] = if mnemonic == "vbroadcastsd" {
        &[("ymm", "%ymm1"), ("zmm", "%zmm1")]
    } else {
        &[("xmm", "%xmm1"), ("ymm", "%ymm1"), ("zmm", "%zmm1")]
    };
    for (reg_class, dst) in widths {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} %{src_reg}16, {dst}"),
            InputProfile::Int,
        );
    }
}

fn add_block_broadcast_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    profile: InputProfile,
    allow_reg_source: bool,
    widths: &[(&str, &str)],
) {
    if allow_reg_source {
        for dst in DST_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    specs,
                    format!("{mnemonic}_dst_zmm{dst}_rm_xmm{rm}"),
                    format!("{mnemonic} %xmm{rm}, %zmm{dst}"),
                    profile,
                );
            }
        }
    }
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} (%rax), %zmm17 {suffix}"),
            profile,
        );
    }
    for (reg_class, dst) in widths {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} (%rax), {dst}"),
            profile,
        );
    }
}

fn add_mask_to_vec_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_src_k0"),
            format!("{mnemonic} %k0, %zmm{dst}"),
            InputProfile::Int,
        );
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_src_k2"),
            format!("{mnemonic} %k2, %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst) in [("xmm", "%xmm1"), ("ymm", "%ymm1"), ("zmm", "%zmm1")] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} %k0, {dst}"),
            InputProfile::Int,
        );
    }
}

fn add_vec_to_mask_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for rm in RM_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_src_zmm{rm}_dst_k3"),
            format!("{mnemonic} %zmm{rm}, %k3"),
            InputProfile::Int,
        );
    }
    for (reg_class, src) in [("xmm", "%xmm16"), ("ymm", "%ymm16"), ("zmm", "%zmm16")] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} {src}, %k3"),
            InputProfile::Int,
        );
    }
}

fn add_mask_broadcast_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in DST_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_src_k0"),
            format!("{mnemonic} %k0, %zmm{dst}"),
            InputProfile::Int,
        );
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_src_k2"),
            format!("{mnemonic} %k2, %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst) in [("xmm", "%xmm1"), ("ymm", "%ymm1"), ("zmm", "%zmm1")] {
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} %k0, {dst}"),
            InputProfile::Int,
        );
    }
}

fn reg_class_for_bytes(bytes: usize) -> &'static str {
    if bytes <= 16 {
        "xmm"
    } else if bytes <= 32 {
        "ymm"
    } else {
        "zmm"
    }
}

fn vsib_classes(
    index_size: usize,
    data_size: usize,
    vl_bytes: usize,
) -> (&'static str, &'static str) {
    let lanes = vl_bytes / index_size.max(data_size);
    let index_class = reg_class_for_bytes((lanes * index_size).max(16));
    let data_class = reg_class_for_bytes((lanes * data_size).max(16));
    (index_class, data_class)
}

fn add_vsib_gather_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    index_size: usize,
    data_size: usize,
) {
    let profile = if index_size == 4 {
        InputProfile::Vsib32
    } else {
        InputProfile::Vsib64
    };
    for vl_bytes in [16, 32, 64] {
        let (index_class, data_class) = vsib_classes(index_size, data_size, vl_bytes);
        for index in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_{data_class}_idx_{index_class}{index}"),
                format!("{mnemonic} (%rax,%{index_class}{index}), %{data_class}17 {{%k1}}"),
                profile,
            );
        }
    }
}

fn add_vsib_scatter_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    index_size: usize,
    data_size: usize,
) {
    let profile = if index_size == 4 {
        InputProfile::Vsib32
    } else {
        InputProfile::Vsib64
    };
    for vl_bytes in [16, 32, 64] {
        let (index_class, data_class) = vsib_classes(index_size, data_size, vl_bytes);
        for index in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_{data_class}_idx_{index_class}{index}"),
                format!("{mnemonic} %{data_class}17, (%rax,%{index_class}{index}) {{%k1}}"),
                profile,
            );
        }
    }
}

fn add_vsib_prefetch_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    index_size: usize,
    data_size: usize,
) {
    let profile = if index_size == 4 {
        InputProfile::Vsib32
    } else {
        InputProfile::Vsib64
    };
    let (index_class, _) = vsib_classes(index_size, data_size, 64);
    for index in RM_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_idx_{index_class}{index}"),
            format!("{mnemonic} (%rax,%{index_class}{index}) {{%k1}}"),
            profile,
        );
    }
}

fn add_scalar_fp16_complex_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for (dst, src1) in [(1u8, 2u8), (17, 18)] {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_xmm{dst}_src1_xmm{src1}_rm_xmm{rm}"),
                format!("{mnemonic} %xmm{rm}, %xmm{src1}, %xmm{dst} {{%k1}}"),
                InputProfile::F16,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_xmm{dst}_src1_xmm{src1}_mem"),
            format!("{mnemonic} (%rax), %xmm{src1}, %xmm{dst} {{%k1}}"),
            InputProfile::F16,
        );
    }
    for suffix in ["{%k1}", "{%k1} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_xmm17_xmm18_xmm16_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %xmm16, %xmm18, %xmm17 {suffix}"),
            InputProfile::F16,
        );
    }
}

fn add_source_block_f32_family(specs: &mut Vec<CaseSpec>, mnemonic: &str, scalar: bool) {
    let class = if scalar { "xmm" } else { "zmm" };
    let suffixes = ["{%k1}", "{%k1} {z}"];
    for dst in [1u8, 17] {
        for src1 in [0u8, 20] {
            spec(
                specs,
                format!("{mnemonic}_dst_{class}{dst}_srcblock_{class}{src1}_mem"),
                format!("{mnemonic} (%rax), %{class}{src1}, %{class}{dst} {{%k1}}"),
                InputProfile::F32,
            );
        }
    }
    for suffix in suffixes {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_{class}17_{class}20_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} (%rax), %{class}20, %{class}17 {suffix}"),
            InputProfile::F32,
        );
    }
}

fn add_source_block_i32_family(specs: &mut Vec<CaseSpec>, mnemonic: &str) {
    for dst in [1u8, 17] {
        for src1 in [0u8, 20] {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_srcblock_zmm{src1}_mem"),
                format!("{mnemonic} (%rax), %zmm{src1}, %zmm{dst} {{%k1}}"),
                InputProfile::Int,
            );
        }
    }
    for suffix in ["{%k1}", "{%k1} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_zmm20_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} (%rax), %zmm20, %zmm17 {suffix}"),
            InputProfile::Int,
        );
    }
}

fn add_int_extend_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    src_elem_size: usize,
    dst_elem_size: usize,
) {
    let zmm_src_class = reg_class_for_bytes((64 / dst_elem_size) * src_elem_size);
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_zmm{dst}_rm_{zmm_src_class}{rm}"),
                format!("{mnemonic} %{zmm_src_class}{rm}, %zmm{dst}"),
                InputProfile::Int,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_zmm{dst}_mem"),
            format!("{mnemonic} (%rax), %zmm{dst}"),
            InputProfile::Int,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} (%rax), %zmm17 {suffix}"),
            InputProfile::Int,
        );
    }
    for (reg_class, dst, vl_bytes) in [
        ("xmm", "%xmm1", 16),
        ("ymm", "%ymm1", 32),
        ("zmm", "%zmm1", 64),
    ] {
        let src_class = reg_class_for_bytes((vl_bytes / dst_elem_size) * src_elem_size);
        spec(
            specs,
            format!("{mnemonic}_{reg_class}_vl"),
            format!("{mnemonic} %{src_class}16, {dst}"),
            InputProfile::Int,
        );
    }
}

fn add_int_narrow_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    src_elem_size: usize,
    dst_elem_size: usize,
) {
    let zmm_dst_class = reg_class_for_bytes((64 / src_elem_size) * dst_elem_size);
    for src in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_src_zmm{src}_dst_{zmm_dst_class}{rm}"),
                format!("{mnemonic} %zmm{src}, %{zmm_dst_class}{rm}"),
                InputProfile::Int,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_src_zmm{src}_mem"),
            format!("{mnemonic} %zmm{src}, (%rax)"),
            InputProfile::Int,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mem_mask_zmm16_k1"),
        format!("{mnemonic} %zmm16, (%rax) {{%k1}}"),
        InputProfile::Int,
    );
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_zmm16_dst_{}17_{}",
                zmm_dst_class,
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %zmm16, %{zmm_dst_class}17 {suffix}"),
            InputProfile::Int,
        );
    }
    for (src_class, src_vl_bytes) in [("xmm", 16), ("ymm", 32), ("zmm", 64)] {
        let dst_class = reg_class_for_bytes((src_vl_bytes / src_elem_size) * dst_elem_size);
        spec(
            specs,
            format!("{mnemonic}_{src_class}_vl"),
            format!("{mnemonic} %{src_class}16, %{dst_class}17"),
            InputProfile::Int,
        );
    }
}

fn add_extract_chunk_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    chunk_bytes: usize,
    src_classes: &[&str],
    profile: InputProfile,
) {
    let dst_class = reg_class_for_bytes(chunk_bytes);
    let full_src_class = src_classes[src_classes.len() - 1];
    for src in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_src_{full_src_class}{src}_dst_{dst_class}{rm}"),
                format!("{mnemonic} $1, %{full_src_class}{src}, %{dst_class}{rm}"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_src_{full_src_class}{src}_mem"),
            format!("{mnemonic} $1, %{full_src_class}{src}, (%rax)"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_mem_mask"),
        format!("{mnemonic} $1, %{full_src_class}16, (%rax) {{%k1}}"),
        profile,
    );
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_{dst_class}17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $1, %{full_src_class}16, %{dst_class}17 {suffix}"),
            profile,
        );
    }
    for src_class in src_classes {
        spec(
            specs,
            format!("{mnemonic}_{src_class}_vl"),
            format!("{mnemonic} $1, %{src_class}16, %{dst_class}17"),
            profile,
        );
    }
}

fn add_insert_chunk_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    chunk_bytes: usize,
    dst_classes: &[&str],
    profile: InputProfile,
) {
    let chunk_class = reg_class_for_bytes(chunk_bytes);
    let full_dst_class = dst_classes[dst_classes.len() - 1];
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_{full_dst_class}{dst}_rm_{chunk_class}{rm}"),
                format!(
                    "{mnemonic} $1, %{chunk_class}{rm}, %{full_dst_class}18, %{full_dst_class}{dst}"
                ),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_{full_dst_class}{dst}_mem"),
            format!("{mnemonic} $1, (%rax), %{full_dst_class}18, %{full_dst_class}{dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_{full_dst_class}17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!(
                "{mnemonic} $1, %{chunk_class}16, %{full_dst_class}18, %{full_dst_class}17 {suffix}"
            ),
            profile,
        );
    }
    for dst_class in dst_classes {
        spec(
            specs,
            format!("{mnemonic}_{dst_class}_vl"),
            format!("{mnemonic} $1, %{chunk_class}16, %{dst_class}18, %{dst_class}17"),
            profile,
        );
    }
}

fn add_extract_scalar_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    imm: u8,
    low_gpr: &str,
    high_gpr: &str,
    profile: InputProfile,
) {
    spec(
        specs,
        format!("{mnemonic}_src_xmm16_dst_low_gpr"),
        format!("{mnemonic} ${imm}, %xmm16, {low_gpr}"),
        profile,
    );
    spec(
        specs,
        format!("{mnemonic}_src_xmm16_dst_high_gpr"),
        format!("{mnemonic} ${imm}, %xmm16, {high_gpr}"),
        profile,
    );
    spec(
        specs,
        format!("{mnemonic}_src_xmm24_dst_high_gpr"),
        format!("{mnemonic} ${imm}, %xmm24, {high_gpr}"),
        profile,
    );
    spec(
        specs,
        format!("{mnemonic}_src_xmm16_mem"),
        format!("{mnemonic} ${imm}, %xmm16, (%rax)"),
        profile,
    );

    if mnemonic == "vpextrw" {
        spec_raw(
            specs,
            "vpextrw_map3_reg_dst_low_gpr",
            format!("{mnemonic} ${imm}, %xmm16, {low_gpr}"),
            vec![0x62, 0xe3, 0x7d, 0x08, 0x15, 0xc0, imm],
            profile,
        );
        spec_raw(
            specs,
            "vpextrw_map3_reg_dst_high_gpr",
            format!("{mnemonic} ${imm}, %xmm16, {high_gpr}"),
            vec![0x62, 0xc3, 0x7d, 0x08, 0x15, 0xc0, imm],
            profile,
        );
    }
}

fn add_pinsr_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    imm: u8,
    low_gpr: &str,
    high_gpr: &str,
) {
    spec(
        specs,
        format!("{mnemonic}_src_low_gpr"),
        format!("{mnemonic} ${imm}, {low_gpr}, %xmm18, %xmm17"),
        InputProfile::Int,
    );
    spec(
        specs,
        format!("{mnemonic}_src_high_gpr"),
        format!("{mnemonic} ${imm}, {high_gpr}, %xmm18, %xmm17"),
        InputProfile::Int,
    );
    spec(
        specs,
        format!("{mnemonic}_src_mem"),
        format!("{mnemonic} ${imm}, (%rax), %xmm18, %xmm17"),
        InputProfile::Int,
    );
}

fn add_insertps_family(specs: &mut Vec<CaseSpec>) {
    for rm in RM_EXT_REGS {
        spec(
            specs,
            format!("vinsertps_rm_xmm{rm}"),
            format!("vinsertps $0x9d, %xmm{rm}, %xmm18, %xmm17"),
            InputProfile::F32,
        );
    }
    spec(
        specs,
        "vinsertps_src_mem",
        "vinsertps $0x5d, (%rax), %xmm18, %xmm17",
        InputProfile::F32,
    );
}

fn raw_evex_fp_to_gpr_reg(map: u8, opcode: u8, pp: u8, w: bool, rm: usize) -> Vec<u8> {
    let mut p0 = map | 0x10;
    if rm & 0x10 == 0 {
        p0 |= 0x40;
    }
    if rm & 0x08 == 0 {
        p0 |= 0x20;
    }
    let p1 = (if w { 0x80 } else { 0 }) | 0x78 | 0x04 | pp;
    vec![0x62, p0, p1, 0x08, opcode, 0xc0 | (rm & 0x7) as u8]
}

fn raw_evex_fp_to_gpr_mem(map: u8, opcode: u8, pp: u8, w: bool) -> Vec<u8> {
    let p0 = map | 0x70;
    let p1 = (if w { 0x80 } else { 0 }) | 0x78 | 0x04 | pp;
    vec![0x62, p0, p1, 0x08, opcode, 0x00]
}

fn raw_evex_gpr_mem_to_fp(map: u8, opcode: u8, pp: u8, w: bool) -> Vec<u8> {
    let p0 = map | 0xe0;
    let p1 = (if w { 0x80 } else { 0 }) | (0x0d << 3) | 0x04 | pp;
    vec![0x62, p0, p1, 0x00, opcode, 0x08]
}

fn add_fp_to_gpr_convert_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    map: u8,
    opcode: u8,
    pp: u8,
    profile: InputProfile,
) {
    for rm in RM_EXT_REGS {
        spec_raw(
            specs,
            format!("{mnemonic}_rm_xmm{rm}_to_r8d"),
            format!("{mnemonic} %xmm{rm}, %r8d"),
            raw_evex_fp_to_gpr_reg(map, opcode, pp, false, rm),
            profile,
        );
        spec_raw(
            specs,
            format!("{mnemonic}_rm_xmm{rm}_to_r8"),
            format!("{mnemonic} %xmm{rm}, %r8"),
            raw_evex_fp_to_gpr_reg(map, opcode, pp, true, rm),
            profile,
        );
    }
    spec_raw(
        specs,
        format!("{mnemonic}_mem_to_r8d"),
        format!("{mnemonic} (%rax), %r8d"),
        raw_evex_fp_to_gpr_mem(map, opcode, pp, false),
        profile,
    );
    spec_raw(
        specs,
        format!("{mnemonic}_mem_to_r8"),
        format!("{mnemonic} (%rax), %r8"),
        raw_evex_fp_to_gpr_mem(map, opcode, pp, true),
        profile,
    );
}

fn add_gpr_to_fp_convert_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    map: u8,
    opcode: u8,
    pp: u8,
    profile: InputProfile,
) {
    for (label, src32, src64) in [("rax", "%eax", "%rax"), ("r8", "%r8d", "%r8")] {
        spec(
            specs,
            format!("{mnemonic}_{label}d_to_xmm17"),
            format!("{mnemonic} {src32}, %xmm18, %xmm17"),
            profile,
        );
        spec(
            specs,
            format!("{mnemonic}_{label}_to_xmm17"),
            format!("{mnemonic} {src64}, %xmm18, %xmm17"),
            profile,
        );
    }
    spec_raw(
        specs,
        format!("{mnemonic}_mem32_to_xmm17"),
        format!("{mnemonic} (%rax), %xmm18, %xmm17"),
        raw_evex_gpr_mem_to_fp(map, opcode, pp, false),
        profile,
    );
    spec_raw(
        specs,
        format!("{mnemonic}_mem64_to_xmm17"),
        format!("{mnemonic} (%rax), %xmm18, %xmm17"),
        raw_evex_gpr_mem_to_fp(map, opcode, pp, true),
        profile,
    );
}

fn packed_convert_classes(
    src_elem_size: usize,
    dst_elem_size: usize,
    operation_vl_bytes: usize,
) -> (&'static str, &'static str) {
    if dst_elem_size >= src_elem_size {
        let lanes = operation_vl_bytes / dst_elem_size;
        (
            reg_class_for_bytes(lanes * src_elem_size),
            reg_class_for_bytes(operation_vl_bytes),
        )
    } else {
        let lanes = operation_vl_bytes / src_elem_size;
        (
            reg_class_for_bytes(operation_vl_bytes),
            reg_class_for_bytes(lanes * dst_elem_size),
        )
    }
}

fn add_packed_convert_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    src_elem_size: usize,
    dst_elem_size: usize,
    profile: InputProfile,
) {
    let (full_src_class, full_dst_class) = packed_convert_classes(src_elem_size, dst_elem_size, 64);
    let full_mem_operand = if full_dst_class == "xmm" && src_elem_size > dst_elem_size {
        format!("(%rax){{1to{}}}", 64 / src_elem_size)
    } else {
        "(%rax)".to_string()
    };
    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                specs,
                format!("{mnemonic}_dst_{full_dst_class}{dst}_rm_{full_src_class}{rm}"),
                format!("{mnemonic} %{full_src_class}{rm}, %{full_dst_class}{dst}"),
                profile,
            );
        }
        spec(
            specs,
            format!("{mnemonic}_dst_{full_dst_class}{dst}_mem"),
            format!("{mnemonic} {full_mem_operand}, %{full_dst_class}{dst}"),
            profile,
        );
    }
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_{full_dst_class}17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} %{full_src_class}16, %{full_dst_class}17 {suffix}"),
            profile,
        );
    }
    for vl_bytes in [16, 32, 64] {
        let (src_class, dst_class) = packed_convert_classes(src_elem_size, dst_elem_size, vl_bytes);
        spec(
            specs,
            format!("{mnemonic}_{dst_class}_vl"),
            format!("{mnemonic} %{src_class}16, %{dst_class}17"),
            profile,
        );
    }
}

fn add_packed_fp_convert_store_family(
    specs: &mut Vec<CaseSpec>,
    mnemonic: &str,
    src_elem_size: usize,
    dst_elem_size: usize,
    profile: InputProfile,
) {
    let (full_src_class, full_dst_class) = packed_convert_classes(src_elem_size, dst_elem_size, 64);
    for rm in RM_EXT_REGS {
        spec(
            specs,
            format!("{mnemonic}_src_{full_src_class}16_dst_{full_dst_class}{rm}"),
            format!("{mnemonic} $0, %{full_src_class}16, %{full_dst_class}{rm}"),
            profile,
        );
    }
    spec(
        specs,
        format!("{mnemonic}_src_{full_src_class}16_mem"),
        format!("{mnemonic} $0, %{full_src_class}16, (%rax)"),
        profile,
    );
    spec(
        specs,
        format!("{mnemonic}_src_{full_src_class}16_mem_k1"),
        format!("{mnemonic} $4, %{full_src_class}16, (%rax) {{%k1}}"),
        profile,
    );
    for suffix in ["{%k1}", "{%k2} {z}"] {
        spec(
            specs,
            format!(
                "{mnemonic}_mask_{full_dst_class}17_{}",
                suffix.replace(['%', ' ', '{', '}'], "")
            ),
            format!("{mnemonic} $4, %{full_src_class}16, %{full_dst_class}17 {suffix}"),
            profile,
        );
    }
    for vl_bytes in [16, 32, 64] {
        let (src_class, dst_class) = packed_convert_classes(src_elem_size, dst_elem_size, vl_bytes);
        spec(
            specs,
            format!("{mnemonic}_{dst_class}_vl"),
            format!("{mnemonic} $0, %{src_class}16, %{dst_class}17"),
            profile,
        );
    }
}

fn generated_specs() -> Vec<CaseSpec> {
    let mut specs = Vec::new();
    let masked = ["{%k1}", "{%k2} {z}"];

    for mnemonic in ["vmovups", "vmovaps"] {
        add_move_family(&mut specs, mnemonic, InputProfile::F32);
    }
    add_nt_store_family(&mut specs, "vmovntps", InputProfile::F32);
    add_raw_move_store_reg_family(&mut specs, "vmovups", 0x11, 0x7c, InputProfile::F32);
    add_raw_move_store_reg_family(&mut specs, "vmovaps", 0x29, 0x7c, InputProfile::F32);
    for mnemonic in ["vmovupd", "vmovapd"] {
        add_move_family(&mut specs, mnemonic, InputProfile::F64);
    }
    add_nt_store_family(&mut specs, "vmovntpd", InputProfile::F64);
    add_raw_move_store_reg_family(&mut specs, "vmovupd", 0x11, 0xfd, InputProfile::F64);
    add_raw_move_store_reg_family(&mut specs, "vmovapd", 0x29, 0xfd, InputProfile::F64);
    add_nt_store_family(&mut specs, "vmovntdq", InputProfile::Int);
    add_nt_load_family(&mut specs, "vmovntdqa");
    add_gpr_scalar_move_family(&mut specs, "vmovd", "%eax", "%r8d");
    add_gpr_scalar_move_family(&mut specs, "vmovq", "%rax", "%r8");
    add_movq_vec_family(&mut specs);
    add_gpr_scalar_move_family(&mut specs, "vmovw", "%eax", "%r8d");
    add_scalar_move_family(&mut specs, "vmovss", InputProfile::F32);
    add_scalar_move_family(&mut specs, "vmovsd", InputProfile::F64);
    add_scalar_move_family(&mut specs, "vmovsh", InputProfile::F16);
    add_ps_high_low_move_family(&mut specs, "vmovlps", false, true);
    add_ps_high_low_move_family(&mut specs, "vmovhps", false, true);
    add_ps_high_low_move_family(&mut specs, "vmovhlps", true, false);
    add_ps_high_low_move_family(&mut specs, "vmovlhps", true, false);
    add_pd_high_low_move_family(&mut specs, "vmovlpd");
    add_pd_high_low_move_family(&mut specs, "vmovhpd");
    add_duplicate_move_family(&mut specs, "vmovsldup", InputProfile::F32);
    add_duplicate_move_family(&mut specs, "vmovshdup", InputProfile::F32);
    add_duplicate_move_family(&mut specs, "vmovddup", InputProfile::F64);
    add_fp_compare_family(&mut specs, "vcmpps", InputProfile::F32, 4, false);
    add_fp_compare_family(&mut specs, "vcmppd", InputProfile::F64, 8, false);
    add_fp_compare_family(&mut specs, "vcmpph", InputProfile::F16, 2, false);
    add_fp_compare_family(&mut specs, "vcmpss", InputProfile::F32, 4, true);
    add_fp_compare_family(&mut specs, "vcmpsd", InputProfile::F64, 8, true);
    add_fp_compare_family(&mut specs, "vcmpsh", InputProfile::F16, 2, true);
    for mnemonic in ["vcomiss", "vucomiss"] {
        for rm in RM_EXT_REGS {
            spec(
                &mut specs,
                format!("{mnemonic}_xmm17_xmm{rm}"),
                format!("{mnemonic} %xmm{rm}, %xmm17"),
                InputProfile::F32,
            );
        }
        spec(
            &mut specs,
            format!("{mnemonic}_xmm17_mem"),
            format!("{mnemonic} (%rax), %xmm17"),
            InputProfile::F32,
        );
    }
    for mnemonic in ["vcomisd", "vucomisd"] {
        for rm in RM_EXT_REGS {
            spec(
                &mut specs,
                format!("{mnemonic}_xmm17_xmm{rm}"),
                format!("{mnemonic} %xmm{rm}, %xmm17"),
                InputProfile::F64,
            );
        }
        spec(
            &mut specs,
            format!("{mnemonic}_xmm17_mem"),
            format!("{mnemonic} (%rax), %xmm17"),
            InputProfile::F64,
        );
    }
    add_fpclass_family(&mut specs, "vfpclassps", InputProfile::F32, 4, false);
    add_fpclass_family(&mut specs, "vfpclasspd", InputProfile::F64, 8, false);
    add_fpclass_family(&mut specs, "vfpclassph", InputProfile::F16, 2, false);
    add_fpclass_family(&mut specs, "vfpclassss", InputProfile::F32, 4, true);
    add_fpclass_family(&mut specs, "vfpclasssd", InputProfile::F64, 8, true);
    add_fpclass_family(&mut specs, "vfpclasssh", InputProfile::F16, 2, true);

    let fp32_ternary = [
        "vaddps", "vmulps", "vsubps", "vdivps", "vmaxps", "vminps", "vandps", "vandnps", "vorps",
        "vxorps",
    ];
    for mnemonic in fp32_ternary {
        add_ternary_family(&mut specs, mnemonic, InputProfile::F32, &masked);
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::F32,
        );
    }
    for mnemonic in ["vunpcklps", "vunpckhps"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::F32, &masked);
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::F32,
        );
    }
    add_unary_rm_family(&mut specs, "vsqrtps", InputProfile::F32, &masked);
    spec(
        &mut specs,
        "vsqrtps_zmm_mem_broadcast",
        "vsqrtps (%rax){1to16}, %zmm17",
        InputProfile::F32,
    );
    for mnemonic in [
        "vaddss", "vmulss", "vsubss", "vdivss", "vmaxss", "vminss", "vsqrtss",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F32, &masked);
    }
    let fp64_ternary = [
        "vaddpd", "vmulpd", "vsubpd", "vdivpd", "vmaxpd", "vminpd", "vandpd", "vandnpd", "vorpd",
        "vxorpd",
    ];
    for mnemonic in fp64_ternary {
        add_ternary_family(&mut specs, mnemonic, InputProfile::F64, &masked);
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::F64,
        );
    }
    for mnemonic in ["vunpcklpd", "vunpckhpd"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::F64, &masked);
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::F64,
        );
    }
    add_unary_rm_family(&mut specs, "vsqrtpd", InputProfile::F64, &masked);
    spec(
        &mut specs,
        "vsqrtpd_zmm_mem_broadcast",
        "vsqrtpd (%rax){1to8}, %zmm17",
        InputProfile::F64,
    );
    for mnemonic in [
        "vaddsd", "vmulsd", "vsubsd", "vdivsd", "vmaxsd", "vminsd", "vsqrtsd",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F64, &masked);
    }

    for mnemonic in ["vgetexpps", "vrcp14ps", "vrsqrt14ps"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::F32Pow4, &masked);
    }
    for mnemonic in ["vgetexppd", "vrcp14pd", "vrsqrt14pd"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::F64Pow4, &masked);
    }
    for mnemonic in ["vexp2ps", "vrcp28ps", "vrsqrt28ps"] {
        add_unary_zmm_family(&mut specs, mnemonic, InputProfile::F32Pow4, &masked);
    }
    for mnemonic in ["vexp2pd", "vrcp28pd", "vrsqrt28pd"] {
        add_unary_zmm_family(&mut specs, mnemonic, InputProfile::F64Pow4, &masked);
    }
    for mnemonic in [
        "vgetexpss",
        "vrcp14ss",
        "vrsqrt14ss",
        "vrcp28ss",
        "vrsqrt28ss",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F32Pow4, &masked);
    }
    for mnemonic in [
        "vgetexpsd",
        "vrcp14sd",
        "vrsqrt14sd",
        "vrcp28sd",
        "vrsqrt28sd",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F64Pow4, &masked);
    }

    for mnemonic in ["vrndscaleps", "vreduceps", "vgetmantps"] {
        add_unary_imm_family(&mut specs, mnemonic, InputProfile::F32, 0, &masked);
    }
    for mnemonic in ["vrndscalepd", "vreducepd", "vgetmantpd"] {
        add_unary_imm_family(&mut specs, mnemonic, InputProfile::F64, 0, &masked);
    }
    for mnemonic in ["vrndscaless", "vreducess", "vgetmantss"] {
        add_scalar_fp_imm_family(&mut specs, mnemonic, InputProfile::F32, 0, &masked);
    }
    for mnemonic in ["vrndscalesd", "vreducesd", "vgetmantsd"] {
        add_scalar_fp_imm_family(&mut specs, mnemonic, InputProfile::F64, 0, &masked);
    }

    add_ternary_family(&mut specs, "vscalefps", InputProfile::F32, &masked);
    add_ternary_family(&mut specs, "vscalefpd", InputProfile::F64, &masked);
    add_scalar_fp_family(&mut specs, "vscalefss", InputProfile::F32, &masked);
    add_scalar_fp_family(&mut specs, "vscalefsd", InputProfile::F64, &masked);
    add_ternary_imm_family(&mut specs, "vrangeps", InputProfile::F32, 0, &masked);
    add_ternary_imm_family(&mut specs, "vrangepd", InputProfile::F64, 0, &masked);
    add_scalar_fp_imm_family(&mut specs, "vrangess", InputProfile::F32, 0, &masked);
    add_scalar_fp_imm_family(&mut specs, "vrangesd", InputProfile::F64, 0, &masked);
    add_ternary_imm_family(&mut specs, "vfixupimmps", InputProfile::F32, 0x1b, &masked);
    add_ternary_imm_family(&mut specs, "vfixupimmpd", InputProfile::F64, 0x1b, &masked);
    add_scalar_fp_imm_family(&mut specs, "vfixupimmss", InputProfile::F32, 0x1b, &masked);
    add_scalar_fp_imm_family(&mut specs, "vfixupimmsd", InputProfile::F64, 0x1b, &masked);

    for mnemonic in ["vgetexpph", "vrcpph", "vrsqrtph"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::F16Pow4, &masked);
    }
    for mnemonic in ["vgetexpsh", "vrcpsh", "vrsqrtsh"] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F16Pow4, &masked);
    }
    for mnemonic in ["vrndscaleph", "vreduceph", "vgetmantph"] {
        add_unary_imm_family(&mut specs, mnemonic, InputProfile::F16, 0, &masked);
    }
    for mnemonic in ["vrndscalesh", "vreducesh", "vgetmantsh"] {
        add_scalar_fp_imm_family(&mut specs, mnemonic, InputProfile::F16, 0, &masked);
    }
    add_ternary_family(&mut specs, "vscalefph", InputProfile::F16, &masked);
    add_scalar_fp_family(&mut specs, "vscalefsh", InputProfile::F16, &masked);
    for mnemonic in ["vfmulcph", "vfcmulcph", "vfmaddcph", "vfcmaddcph"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::F16, &masked);
    }
    for mnemonic in ["vfmulcsh", "vfcmulcsh", "vfmaddcsh", "vfcmaddcsh"] {
        add_scalar_fp16_complex_family(&mut specs, mnemonic);
    }
    add_source_block_f32_family(&mut specs, "v4fmaddps", false);
    add_source_block_f32_family(&mut specs, "v4fnmaddps", false);
    add_source_block_f32_family(&mut specs, "v4fmaddss", true);
    add_source_block_f32_family(&mut specs, "v4fnmaddss", true);
    add_source_block_i32_family(&mut specs, "vp4dpwssd");
    add_source_block_i32_family(&mut specs, "vp4dpwssds");

    for mnemonic in ["vcvtss2si", "vcvttss2si"] {
        let opcode = if mnemonic == "vcvtss2si" { 0x2d } else { 0x2c };
        add_fp_to_gpr_convert_family(&mut specs, mnemonic, 1, opcode, 2, InputProfile::F32);
    }
    for mnemonic in ["vcvtsd2si", "vcvttsd2si"] {
        let opcode = if mnemonic == "vcvtsd2si" { 0x2d } else { 0x2c };
        add_fp_to_gpr_convert_family(&mut specs, mnemonic, 1, opcode, 3, InputProfile::F64);
    }
    for mnemonic in ["vcvtss2usi", "vcvttss2usi"] {
        let opcode = if mnemonic == "vcvtss2usi" { 0x79 } else { 0x78 };
        add_fp_to_gpr_convert_family(&mut specs, mnemonic, 1, opcode, 2, InputProfile::F32);
    }
    for mnemonic in ["vcvtsd2usi", "vcvttsd2usi"] {
        let opcode = if mnemonic == "vcvtsd2usi" { 0x79 } else { 0x78 };
        add_fp_to_gpr_convert_family(&mut specs, mnemonic, 1, opcode, 3, InputProfile::F64);
    }
    add_gpr_to_fp_convert_family(&mut specs, "vcvtsi2ss", 1, 0x2a, 2, InputProfile::Int);
    add_gpr_to_fp_convert_family(&mut specs, "vcvtsi2sd", 1, 0x2a, 3, InputProfile::Int);
    add_gpr_to_fp_convert_family(&mut specs, "vcvtusi2ss", 1, 0x7b, 2, InputProfile::Int);
    add_gpr_to_fp_convert_family(&mut specs, "vcvtusi2sd", 1, 0x7b, 3, InputProfile::Int);
    add_scalar_fp_family(&mut specs, "vcvtss2sd", InputProfile::F32, &masked);
    add_scalar_fp_family(&mut specs, "vcvtsd2ss", InputProfile::F64, &masked);
    for mnemonic in ["vcvtsh2si", "vcvttsh2si"] {
        let opcode = if mnemonic == "vcvtsh2si" { 0x2d } else { 0x2c };
        add_fp_to_gpr_convert_family(&mut specs, mnemonic, 5, opcode, 2, InputProfile::F16);
    }
    for mnemonic in ["vcvtsh2usi", "vcvttsh2usi"] {
        let opcode = if mnemonic == "vcvtsh2usi" { 0x79 } else { 0x78 };
        add_fp_to_gpr_convert_family(&mut specs, mnemonic, 5, opcode, 2, InputProfile::F16);
    }
    add_gpr_to_fp_convert_family(&mut specs, "vcvtsi2sh", 5, 0x2a, 2, InputProfile::Int);
    add_gpr_to_fp_convert_family(&mut specs, "vcvtusi2sh", 5, 0x7b, 2, InputProfile::Int);
    add_scalar_fp_family(&mut specs, "vcvtss2sh", InputProfile::F32, &masked);
    add_scalar_fp_family(&mut specs, "vcvtsh2ss", InputProfile::F16, &masked);
    add_scalar_fp_family(&mut specs, "vcvtsd2sh", InputProfile::F64, &masked);
    add_scalar_fp_family(&mut specs, "vcvtsh2sd", InputProfile::F16, &masked);
    for mnemonic in ["vcvtps2pd", "vcvtpd2ps"] {
        let (src_elem_size, dst_elem_size, profile) = if mnemonic == "vcvtps2pd" {
            (4, 8, InputProfile::F32)
        } else {
            (8, 4, InputProfile::F64)
        };
        add_packed_convert_family(&mut specs, mnemonic, src_elem_size, dst_elem_size, profile);
    }
    for mnemonic in ["vcvtdq2ps", "vcvtqq2ps", "vcvtdq2pd", "vcvtqq2pd"] {
        let (src_elem_size, dst_elem_size) = match mnemonic {
            "vcvtdq2ps" => (4, 4),
            "vcvtqq2ps" => (8, 4),
            "vcvtdq2pd" => (4, 8),
            "vcvtqq2pd" => (8, 8),
            _ => unreachable!(),
        };
        add_packed_convert_family(
            &mut specs,
            mnemonic,
            src_elem_size,
            dst_elem_size,
            InputProfile::Int,
        );
    }
    for mnemonic in ["vcvtudq2ps", "vcvtuqq2ps", "vcvtudq2pd", "vcvtuqq2pd"] {
        let (src_elem_size, dst_elem_size) = match mnemonic {
            "vcvtudq2ps" => (4, 4),
            "vcvtuqq2ps" => (8, 4),
            "vcvtudq2pd" => (4, 8),
            "vcvtuqq2pd" => (8, 8),
            _ => unreachable!(),
        };
        add_packed_convert_family(
            &mut specs,
            mnemonic,
            src_elem_size,
            dst_elem_size,
            InputProfile::Int,
        );
    }
    for mnemonic in [
        "vcvtps2dq",
        "vcvttps2dq",
        "vcvtps2qq",
        "vcvttps2qq",
        "vcvtps2udq",
        "vcvttps2udq",
        "vcvtps2uqq",
        "vcvttps2uqq",
    ] {
        let dst_elem_size = if mnemonic.ends_with("qq") || mnemonic.ends_with("uqq") {
            8
        } else {
            4
        };
        add_packed_convert_family(&mut specs, mnemonic, 4, dst_elem_size, InputProfile::F32);
    }
    for mnemonic in [
        "vcvtpd2dq",
        "vcvttpd2dq",
        "vcvtpd2qq",
        "vcvttpd2qq",
        "vcvtpd2udq",
        "vcvttpd2udq",
        "vcvtpd2uqq",
        "vcvttpd2uqq",
    ] {
        let dst_elem_size = if mnemonic.ends_with("qq") || mnemonic.ends_with("uqq") {
            8
        } else {
            4
        };
        add_packed_convert_family(&mut specs, mnemonic, 8, dst_elem_size, InputProfile::F64);
    }
    add_packed_convert_family(&mut specs, "vcvtph2ps", 2, 4, InputProfile::F16);
    add_packed_convert_family(&mut specs, "vcvtph2psx", 2, 4, InputProfile::F16);
    add_packed_convert_family(&mut specs, "vcvtph2pd", 2, 8, InputProfile::F16);
    add_packed_convert_family(&mut specs, "vcvtps2phx", 4, 2, InputProfile::F32);
    add_packed_fp_convert_store_family(&mut specs, "vcvtps2ph", 4, 2, InputProfile::F32);
    add_packed_convert_family(&mut specs, "vcvtpd2ph", 8, 2, InputProfile::F64);
    for mnemonic in ["vcvtph2dq", "vcvttph2dq", "vcvtph2udq", "vcvttph2udq"] {
        add_packed_convert_family(&mut specs, mnemonic, 2, 4, InputProfile::F16);
    }
    for mnemonic in ["vcvtph2qq", "vcvttph2qq", "vcvtph2uqq", "vcvttph2uqq"] {
        add_packed_convert_family(&mut specs, mnemonic, 2, 8, InputProfile::F16);
    }
    for mnemonic in ["vcvtph2w", "vcvttph2w", "vcvtph2uw", "vcvttph2uw"] {
        add_packed_convert_family(&mut specs, mnemonic, 2, 2, InputProfile::F16);
    }
    for mnemonic in [
        "vcvtdq2ph",
        "vcvtqq2ph",
        "vcvtudq2ph",
        "vcvtuqq2ph",
        "vcvtw2ph",
        "vcvtuw2ph",
    ] {
        let src_elem_size = if mnemonic.contains("qq") {
            8
        } else if mnemonic.contains('w') {
            2
        } else {
            4
        };
        add_packed_convert_family(&mut specs, mnemonic, src_elem_size, 2, InputProfile::Int);
    }
    for mnemonic in [
        "vfmadd132ps",
        "vfmadd213ps",
        "vfmadd231ps",
        "vfmsub132ps",
        "vfmsub213ps",
        "vfmsub231ps",
        "vfnmadd132ps",
        "vfnmadd213ps",
        "vfnmadd231ps",
        "vfnmsub132ps",
        "vfnmsub213ps",
        "vfnmsub231ps",
        "vfmaddsub132ps",
        "vfmaddsub213ps",
        "vfmaddsub231ps",
        "vfmsubadd132ps",
        "vfmsubadd213ps",
        "vfmsubadd231ps",
    ] {
        add_fma_packed_family(&mut specs, mnemonic, InputProfile::F32, "1to16", &masked);
    }
    for mnemonic in [
        "vfmadd132pd",
        "vfmadd213pd",
        "vfmadd231pd",
        "vfmsub132pd",
        "vfmsub213pd",
        "vfmsub231pd",
        "vfnmadd132pd",
        "vfnmadd213pd",
        "vfnmadd231pd",
        "vfnmsub132pd",
        "vfnmsub213pd",
        "vfnmsub231pd",
        "vfmaddsub132pd",
        "vfmaddsub213pd",
        "vfmaddsub231pd",
        "vfmsubadd132pd",
        "vfmsubadd213pd",
        "vfmsubadd231pd",
    ] {
        add_fma_packed_family(&mut specs, mnemonic, InputProfile::F64, "1to8", &masked);
    }
    for mnemonic in [
        "vfmadd132ph",
        "vfmadd213ph",
        "vfmadd231ph",
        "vfmsub132ph",
        "vfmsub213ph",
        "vfmsub231ph",
        "vfnmadd132ph",
        "vfnmadd213ph",
        "vfnmadd231ph",
        "vfnmsub132ph",
        "vfnmsub213ph",
        "vfnmsub231ph",
        "vfmaddsub132ph",
        "vfmaddsub213ph",
        "vfmaddsub231ph",
        "vfmsubadd132ph",
        "vfmsubadd213ph",
        "vfmsubadd231ph",
    ] {
        add_fma_packed_family(&mut specs, mnemonic, InputProfile::F16, "1to32", &masked);
    }
    for mnemonic in [
        "vfmadd132ss",
        "vfmadd213ss",
        "vfmadd231ss",
        "vfmsub132ss",
        "vfmsub213ss",
        "vfmsub231ss",
        "vfnmadd132ss",
        "vfnmadd213ss",
        "vfnmadd231ss",
        "vfnmsub132ss",
        "vfnmsub213ss",
        "vfnmsub231ss",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F32, &masked);
    }
    for mnemonic in [
        "vfmadd132sd",
        "vfmadd213sd",
        "vfmadd231sd",
        "vfmsub132sd",
        "vfmsub213sd",
        "vfmsub231sd",
        "vfnmadd132sd",
        "vfnmadd213sd",
        "vfnmadd231sd",
        "vfnmsub132sd",
        "vfnmsub213sd",
        "vfnmsub231sd",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F64, &masked);
    }
    for mnemonic in [
        "vfmadd132sh",
        "vfmadd213sh",
        "vfmadd231sh",
        "vfmsub132sh",
        "vfmsub213sh",
        "vfmsub231sh",
        "vfnmadd132sh",
        "vfnmadd213sh",
        "vfnmadd231sh",
        "vfnmsub132sh",
        "vfnmsub213sh",
        "vfnmsub231sh",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F16, &masked);
    }
    add_shufp_family(&mut specs, "vshufps", InputProfile::F32, "1to16");
    add_shufp_family(&mut specs, "vshufpd", InputProfile::F64, "1to8");
    add_shuffle_128_lane_family(&mut specs, "vshuff32x4", InputProfile::F32, "1to16");
    add_shuffle_128_lane_family(&mut specs, "vshuff64x2", InputProfile::F64, "1to8");
    add_shuffle_128_lane_family(&mut specs, "vshufi32x4", InputProfile::Int, "1to16");
    add_shuffle_128_lane_family(&mut specs, "vshufi64x2", InputProfile::Int, "1to8");

    for mnemonic in [
        "vmovdqa32",
        "vmovdqa64",
        "vmovdqu8",
        "vmovdqu16",
        "vmovdqu32",
        "vmovdqu64",
    ] {
        add_move_family(&mut specs, mnemonic, InputProfile::Int);
        spec(
            &mut specs,
            format!("{mnemonic}_masked_load_zmm17_zmm16"),
            format!("{mnemonic} %zmm16, %zmm17 {{%k1}}"),
            InputProfile::Int,
        );
        spec(
            &mut specs,
            format!("{mnemonic}_masked_store_zmm17_zmm16"),
            format!("{mnemonic} %zmm17, %zmm16 {{%k1}}"),
            InputProfile::Int,
        );
        spec(
            &mut specs,
            format!("{mnemonic}_masked_mem_load_zmm17"),
            format!("{mnemonic} (%rax), %zmm17 {{%k1}}"),
            InputProfile::Int,
        );
        spec(
            &mut specs,
            format!("{mnemonic}_masked_mem_store_zmm17"),
            format!("{mnemonic} %zmm17, (%rax) {{%k1}}"),
            InputProfile::Int,
        );
    }
    for (mnemonic, p1) in [
        ("vmovdqa32", 0x7d),
        ("vmovdqa64", 0xfd),
        ("vmovdqu8", 0x7f),
        ("vmovdqu16", 0xff),
        ("vmovdqu32", 0x7e),
        ("vmovdqu64", 0xfe),
    ] {
        add_raw_move_store_reg_family(&mut specs, mnemonic, 0x7f, p1, InputProfile::Int);
    }

    for mnemonic in [
        "vpandd",
        "vpandq",
        "vpandnd",
        "vpandnq",
        "vpord",
        "vporq",
        "vpxord",
        "vpxorq",
        "vpaddb",
        "vpaddw",
        "vpaddd",
        "vpaddq",
        "vpaddsb",
        "vpaddsw",
        "vpaddusb",
        "vpaddusw",
        "vpavgb",
        "vpavgw",
        "vpmaxsb",
        "vpmaxsw",
        "vpmaxsd",
        "vpmaxsq",
        "vpmaxub",
        "vpmaxuw",
        "vpmaxud",
        "vpmaxuq",
        "vpminsb",
        "vpminsw",
        "vpminsd",
        "vpminsq",
        "vpminub",
        "vpminuw",
        "vpminud",
        "vpminuq",
        "vpmaddubsw",
        "vpmaddwd",
        "vpmuldq",
        "vpmuludq",
        "vpmulhuw",
        "vpmulhw",
        "vpmulhrsw",
        "vgf2p8mulb",
        "vpsubb",
        "vpsubw",
        "vpsubd",
        "vpsubq",
        "vpsubsb",
        "vpsubsw",
        "vpsubusb",
        "vpsubusw",
        "vpmullw",
        "vpmulld",
        "vpmullq",
    ] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in ["vgf2p8affineinvqb", "vgf2p8affineqb"] {
        add_gf2p8_affine_family(&mut specs, mnemonic);
    }
    add_vp2intersect_family(&mut specs, "vp2intersectd", InputProfile::Int, "1to16");
    add_vp2intersect_family(&mut specs, "vp2intersectq", InputProfile::Int, "1to8");
    add_vpclmulqdq_family(&mut specs);
    for mnemonic in ["vaesdec", "vaesdeclast", "vaesenc", "vaesenclast"] {
        add_vaes_family(&mut specs, mnemonic);
    }
    for mnemonic in ["vpacksswb", "vpackssdw", "vpackuswb", "vpackusdw"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in [
        "vpunpcklbw",
        "vpunpcklwd",
        "vpunpckldq",
        "vpunpcklqdq",
        "vpunpckhbw",
        "vpunpckhwd",
        "vpunpckhdq",
        "vpunpckhqdq",
    ] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in ["vpunpckldq", "vpunpckhdq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vpunpcklqdq", "vpunpckhqdq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vpackssdw", "vpackusdw"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    add_ternary_family(&mut specs, "vpsadbw", InputProfile::Int, &[]);
    add_ternary_family(&mut specs, "vpshufb", InputProfile::Int, &masked);
    add_ternary_family(&mut specs, "vpmultishiftqb", InputProfile::Int, &masked);
    spec(
        &mut specs,
        "vpmultishiftqb_zmm_mem_broadcast",
        "vpmultishiftqb (%rax){1to8}, %zmm18, %zmm17 {%k1}",
        InputProfile::Int,
    );
    for mnemonic in ["vpshufd", "vpshufhw", "vpshuflw"] {
        add_rotate_imm_family(&mut specs, mnemonic);
    }
    spec(
        &mut specs,
        "vpshufd_zmm_mem_broadcast",
        "vpshufd $0x1b, (%rax){1to16}, %zmm17",
        InputProfile::Int,
    );
    add_align_family(&mut specs, "vpalignr");
    for mnemonic in ["vpmaxsd", "vpmaxud", "vpminsd", "vpminud"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in [
        "vpmaxsq", "vpmaxuq", "vpminsq", "vpminuq", "vpmuldq", "vpmuludq",
    ] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }

    for mnemonic in ["vpabsb", "vpabsw", "vpabsd", "vpabsq"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    spec(
        &mut specs,
        "vpabsd_zmm_mem_broadcast",
        "vpabsd (%rax){1to16}, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vpabsq_zmm_mem_broadcast",
        "vpabsq (%rax){1to8}, %zmm17",
        InputProfile::Int,
    );

    for (mnemonic, src_elem_size, dst_elem_size) in [
        ("vpmovsxbw", 1, 2),
        ("vpmovsxbd", 1, 4),
        ("vpmovsxbq", 1, 8),
        ("vpmovsxwd", 2, 4),
        ("vpmovsxwq", 2, 8),
        ("vpmovsxdq", 4, 8),
        ("vpmovzxbw", 1, 2),
        ("vpmovzxbd", 1, 4),
        ("vpmovzxbq", 1, 8),
        ("vpmovzxwd", 2, 4),
        ("vpmovzxwq", 2, 8),
        ("vpmovzxdq", 4, 8),
    ] {
        add_int_extend_family(&mut specs, mnemonic, src_elem_size, dst_elem_size);
    }

    for (mnemonic, src_elem_size, dst_elem_size) in [
        ("vpmovswb", 2, 1),
        ("vpmovsdb", 4, 1),
        ("vpmovsqb", 8, 1),
        ("vpmovsdw", 4, 2),
        ("vpmovsqw", 8, 2),
        ("vpmovsqd", 8, 4),
        ("vpmovuswb", 2, 1),
        ("vpmovusdb", 4, 1),
        ("vpmovusqb", 8, 1),
        ("vpmovusdw", 4, 2),
        ("vpmovusqw", 8, 2),
        ("vpmovusqd", 8, 4),
        ("vpmovwb", 2, 1),
        ("vpmovdb", 4, 1),
        ("vpmovqb", 8, 1),
        ("vpmovdw", 4, 2),
        ("vpmovqw", 8, 2),
        ("vpmovqd", 8, 4),
    ] {
        add_int_narrow_family(&mut specs, mnemonic, src_elem_size, dst_elem_size);
    }

    for mnemonic in ["vextractf32x4", "vextractf64x2"] {
        add_extract_chunk_family(
            &mut specs,
            mnemonic,
            16,
            &["ymm", "zmm"],
            if mnemonic == "vextractf32x4" {
                InputProfile::F32
            } else {
                InputProfile::F64
            },
        );
    }
    for mnemonic in ["vextracti32x4", "vextracti64x2"] {
        add_extract_chunk_family(&mut specs, mnemonic, 16, &["ymm", "zmm"], InputProfile::Int);
    }
    for mnemonic in ["vextractf32x8", "vextractf64x4"] {
        add_extract_chunk_family(
            &mut specs,
            mnemonic,
            32,
            &["zmm"],
            if mnemonic == "vextractf32x8" {
                InputProfile::F32
            } else {
                InputProfile::F64
            },
        );
    }
    for mnemonic in ["vextracti32x8", "vextracti64x4"] {
        add_extract_chunk_family(&mut specs, mnemonic, 32, &["zmm"], InputProfile::Int);
    }
    for mnemonic in ["vinsertf32x4", "vinsertf64x2"] {
        add_insert_chunk_family(
            &mut specs,
            mnemonic,
            16,
            &["ymm", "zmm"],
            if mnemonic == "vinsertf32x4" {
                InputProfile::F32
            } else {
                InputProfile::F64
            },
        );
    }
    for mnemonic in ["vinserti32x4", "vinserti64x2"] {
        add_insert_chunk_family(&mut specs, mnemonic, 16, &["ymm", "zmm"], InputProfile::Int);
    }
    for mnemonic in ["vinsertf32x8", "vinsertf64x4"] {
        add_insert_chunk_family(
            &mut specs,
            mnemonic,
            32,
            &["zmm"],
            if mnemonic == "vinsertf32x8" {
                InputProfile::F32
            } else {
                InputProfile::F64
            },
        );
    }
    for mnemonic in ["vinserti32x8", "vinserti64x4"] {
        add_insert_chunk_family(&mut specs, mnemonic, 32, &["zmm"], InputProfile::Int);
    }

    add_extract_scalar_family(
        &mut specs,
        "vextractps",
        2,
        "%eax",
        "%r8d",
        InputProfile::F32,
    );
    add_extract_scalar_family(&mut specs, "vpextrb", 7, "%eax", "%r8d", InputProfile::Int);
    add_extract_scalar_family(&mut specs, "vpextrw", 5, "%eax", "%r8d", InputProfile::Int);
    add_extract_scalar_family(&mut specs, "vpextrd", 2, "%eax", "%r8d", InputProfile::Int);
    add_extract_scalar_family(&mut specs, "vpextrq", 1, "%rax", "%r8", InputProfile::Int);
    add_pinsr_family(&mut specs, "vpinsrb", 7, "%eax", "%r8d");
    add_pinsr_family(&mut specs, "vpinsrw", 5, "%eax", "%r8d");
    add_pinsr_family(&mut specs, "vpinsrd", 2, "%eax", "%r8d");
    add_pinsr_family(&mut specs, "vpinsrq", 1, "%rax", "%r8");
    add_insertps_family(&mut specs);

    for mnemonic in [
        "vpcmpeqb", "vpcmpeqw", "vpcmpeqd", "vpcmpeqq", "vpcmpgtb", "vpcmpgtw", "vpcmpgtd",
        "vpcmpgtq",
    ] {
        add_compare_family(&mut specs, mnemonic);
    }
    for mnemonic in [
        "vpcmpub", "vpcmpuw", "vpcmpud", "vpcmpuq", "vpcmpb", "vpcmpw", "vpcmpd", "vpcmpq",
    ] {
        add_vpcmp_imm_family(&mut specs, mnemonic);
    }
    for mnemonic in ["vpternlogd", "vpternlogq"] {
        add_vpternlog_family(&mut specs, mnemonic);
    }
    spec(
        &mut specs,
        "vpternlogd_zmm_mem_broadcast",
        "vpternlogd $0xe2, (%rax){1to16}, %zmm18, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vpternlogq_zmm_mem_broadcast",
        "vpternlogq $0xe4, (%rax){1to8}, %zmm18, %zmm17",
        InputProfile::Int,
    );
    for mnemonic in [
        "vptestmb",
        "vptestmw",
        "vptestmd",
        "vptestmq",
        "vptestnmb",
        "vptestnmw",
        "vptestnmd",
        "vptestnmq",
    ] {
        add_test_mask_family(&mut specs, mnemonic);
    }
    for mnemonic in ["vptestmd", "vptestnmd"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %k3"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vptestmq", "vptestnmq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %k3"),
            InputProfile::Int,
        );
    }

    add_ternary_family(&mut specs, "vblendmps", InputProfile::F32, &masked);
    spec(
        &mut specs,
        "vblendmps_zmm_mem_broadcast",
        "vblendmps (%rax){1to16}, %zmm18, %zmm17 {%k2}",
        InputProfile::F32,
    );
    add_ternary_family(&mut specs, "vblendmpd", InputProfile::F64, &masked);
    spec(
        &mut specs,
        "vblendmpd_zmm_mem_broadcast",
        "vblendmpd (%rax){1to8}, %zmm18, %zmm17 {%k2}",
        InputProfile::F64,
    );
    for mnemonic in ["vpblendmb", "vpblendmw", "vpblendmd", "vpblendmq"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    spec(
        &mut specs,
        "vpblendmd_zmm_mem_broadcast",
        "vpblendmd (%rax){1to16}, %zmm18, %zmm17 {%k2}",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vpblendmq_zmm_mem_broadcast",
        "vpblendmq (%rax){1to8}, %zmm18, %zmm17 {%k2}",
        InputProfile::Int,
    );

    for mnemonic in ["valignd", "valignq"] {
        add_align_family(&mut specs, mnemonic);
    }
    spec(
        &mut specs,
        "valignd_zmm_mem_broadcast",
        "valignd $3, (%rax){1to16}, %zmm18, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "valignq_zmm_mem_broadcast",
        "valignq $2, (%rax){1to8}, %zmm18, %zmm17",
        InputProfile::Int,
    );

    for mnemonic in ["vprold", "vprolq", "vprord", "vprorq"] {
        add_rotate_imm_family(&mut specs, mnemonic);
    }
    spec(
        &mut specs,
        "vprold_zmm_mem_broadcast",
        "vprold $5, (%rax){1to16}, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vprolq_zmm_mem_broadcast",
        "vprolq $5, (%rax){1to8}, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vprord_zmm_mem_broadcast",
        "vprord $5, (%rax){1to16}, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vprorq_zmm_mem_broadcast",
        "vprorq $5, (%rax){1to8}, %zmm17",
        InputProfile::Int,
    );

    for mnemonic in ["vprolvd", "vprolvq", "vprorvd", "vprorvq"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in ["vprolvd", "vprorvd"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vprolvq", "vprorvq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }

    for mnemonic in [
        "vpshldw", "vpshldd", "vpshldq", "vpshrdw", "vpshrdd", "vpshrdq",
    ] {
        add_align_family(&mut specs, mnemonic);
    }
    for mnemonic in ["vpshldd", "vpshrdd"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} $5, (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vpshldq", "vpshrdq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} $5, (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }

    for mnemonic in [
        "vpshldvw", "vpshldvd", "vpshldvq", "vpshrdvw", "vpshrdvd", "vpshrdvq",
    ] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in ["vpshldvd", "vpshrdvd"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vpshldvq", "vpshrdvq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }

    for mnemonic in [
        "vpsrlw", "vpsrld", "vpsrlq", "vpsraw", "vpsrad", "vpsraq", "vpsllw", "vpslld", "vpsllq",
    ] {
        for dst in DST_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    &mut specs,
                    format!("{mnemonic}_imm_dst_zmm{dst}_rm_zmm{rm}"),
                    format!("{mnemonic} $3, %zmm{rm}, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
            spec(
                &mut specs,
                format!("{mnemonic}_imm_dst_zmm{dst}_mem"),
                format!("{mnemonic} $3, (%rax), %zmm{dst}"),
                InputProfile::Int,
            );
        }
        spec(
            &mut specs,
            format!("{mnemonic}_imm_mask_zmm17_zmm16"),
            format!("{mnemonic} $3, %zmm16, %zmm17 {{%k1}}"),
            InputProfile::Int,
        );
        for (reg_class, dst, src) in [
            ("xmm", "%xmm1", "%xmm16"),
            ("ymm", "%ymm1", "%ymm16"),
            ("zmm", "%zmm1", "%zmm16"),
        ] {
            spec(
                &mut specs,
                format!("{mnemonic}_imm_{reg_class}_vl"),
                format!("{mnemonic} $3, {src}, {dst}"),
                InputProfile::Int,
            );
        }
    }
    for mnemonic in ["vpslldq", "vpsrldq"] {
        for dst in DST_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    &mut specs,
                    format!("{mnemonic}_dst_zmm{dst}_rm_zmm{rm}"),
                    format!("{mnemonic} $3, %zmm{rm}, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
            spec(
                &mut specs,
                format!("{mnemonic}_dst_zmm{dst}_mem"),
                format!("{mnemonic} $3, (%rax), %zmm{dst}"),
                InputProfile::Int,
            );
        }
        for (reg_class, dst, src) in [
            ("xmm", "%xmm1", "%xmm16"),
            ("ymm", "%ymm1", "%ymm16"),
            ("zmm", "%zmm1", "%zmm16"),
        ] {
            spec(
                &mut specs,
                format!("{mnemonic}_{reg_class}_vl"),
                format!("{mnemonic} $3, {src}, {dst}"),
                InputProfile::Int,
            );
        }
    }
    for mnemonic in [
        "vpsrlw", "vpsrld", "vpsrlq", "vpsraw", "vpsrad", "vpsraq", "vpsllw", "vpslld", "vpsllq",
    ] {
        for dst in DST_EXT_REGS {
            for src1 in SRC1_EXT_REGS {
                for count in RM_EXT_REGS {
                    spec(
                        &mut specs,
                        format!("{mnemonic}_var_dst_zmm{dst}_src1_zmm{src1}_count_xmm{count}"),
                        format!("{mnemonic} %xmm{count}, %zmm{src1}, %zmm{dst}"),
                        InputProfile::Int,
                    );
                }
                spec(
                    &mut specs,
                    format!("{mnemonic}_var_dst_zmm{dst}_src1_zmm{src1}_mem_count"),
                    format!("{mnemonic} (%rax), %zmm{src1}, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
        }
        for (reg_class, dst, src1, count) in [
            ("xmm", "%xmm1", "%xmm2", "%xmm16"),
            ("ymm", "%ymm1", "%ymm2", "%xmm16"),
            ("zmm", "%zmm1", "%zmm2", "%xmm16"),
        ] {
            spec(
                &mut specs,
                format!("{mnemonic}_var_{reg_class}_vl"),
                format!("{mnemonic} {count}, {src1}, {dst}"),
                InputProfile::Int,
            );
        }
    }
    for mnemonic in [
        "vpsrlvw", "vpsrlvd", "vpsrlvq", "vpsravw", "vpsravd", "vpsravq", "vpsllvw", "vpsllvd",
        "vpsllvq",
    ] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in ["vpsrlvd", "vpsravd", "vpsllvd"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to16}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }
    for mnemonic in ["vpsrlvq", "vpsravq", "vpsllvq"] {
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to8}}, %zmm18, %zmm17"),
            InputProfile::Int,
        );
    }

    for (mnemonic, src_reg) in [
        ("vbroadcastss", "xmm"),
        ("vbroadcastsd", "xmm"),
        ("vpbroadcastb", "xmm"),
        ("vpbroadcastw", "xmm"),
        ("vpbroadcastd", "xmm"),
        ("vpbroadcastq", "xmm"),
    ] {
        add_broadcast_family(&mut specs, mnemonic, src_reg);
    }
    add_block_broadcast_family(
        &mut specs,
        "vbroadcastf32x2",
        InputProfile::F32,
        true,
        &[("ymm", "%ymm1"), ("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcastf32x4",
        InputProfile::F32,
        false,
        &[("ymm", "%ymm1"), ("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcastf32x8",
        InputProfile::F32,
        false,
        &[("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcastf64x2",
        InputProfile::F64,
        false,
        &[("ymm", "%ymm1"), ("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcastf64x4",
        InputProfile::F64,
        false,
        &[("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcasti32x2",
        InputProfile::Int,
        true,
        &[("xmm", "%xmm1"), ("ymm", "%ymm1"), ("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcasti32x4",
        InputProfile::Int,
        false,
        &[("ymm", "%ymm1"), ("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcasti32x8",
        InputProfile::Int,
        false,
        &[("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcasti64x2",
        InputProfile::Int,
        false,
        &[("ymm", "%ymm1"), ("zmm", "%zmm1")],
    );
    add_block_broadcast_family(
        &mut specs,
        "vbroadcasti64x4",
        InputProfile::Int,
        false,
        &[("zmm", "%zmm1")],
    );
    for mnemonic in ["vpmovm2b", "vpmovm2w", "vpmovm2d", "vpmovm2q"] {
        add_mask_to_vec_family(&mut specs, mnemonic);
    }
    for mnemonic in ["vpmovb2m", "vpmovw2m", "vpmovd2m", "vpmovq2m"] {
        add_vec_to_mask_family(&mut specs, mnemonic);
    }
    for mnemonic in ["vpbroadcastmb2q", "vpbroadcastmw2d"] {
        add_mask_broadcast_family(&mut specs, mnemonic);
    }

    for mnemonic in [
        "vexpandps",
        "vexpandpd",
        "vpexpandb",
        "vpexpandw",
        "vpexpandd",
        "vpexpandq",
    ] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    for mnemonic in [
        "vcompressps",
        "vcompresspd",
        "vpcompressb",
        "vpcompressw",
        "vpcompressd",
        "vpcompressq",
    ] {
        for src in DST_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    &mut specs,
                    format!("{mnemonic}_src_zmm{src}_rm_zmm{rm}"),
                    format!("{mnemonic} %zmm{src}, %zmm{rm} {{%k1}}"),
                    InputProfile::Int,
                );
            }
            spec(
                &mut specs,
                format!("{mnemonic}_src_zmm{src}_mem"),
                format!("{mnemonic} %zmm{src}, (%rax) {{%k1}}"),
                InputProfile::Int,
            );
        }
        for (reg_class, dst, src) in [
            ("xmm", "%xmm1", "%xmm16"),
            ("ymm", "%ymm1", "%ymm16"),
            ("zmm", "%zmm1", "%zmm16"),
        ] {
            spec(
                &mut specs,
                format!("{mnemonic}_{reg_class}_vl"),
                format!("{mnemonic} {src}, {dst} {{%k1}}"),
                InputProfile::Int,
            );
        }
    }

    add_permute_var_family(
        &mut specs,
        "vpermps",
        InputProfile::F32,
        false,
        Some("1to16"),
    );
    add_permute_var_family(
        &mut specs,
        "vpermpd",
        InputProfile::F64,
        false,
        Some("1to8"),
    );
    add_permute_qword_imm_family(&mut specs, "vpermpd", InputProfile::F64);
    add_permute_var_family(
        &mut specs,
        "vpermd",
        InputProfile::Int,
        false,
        Some("1to16"),
    );
    add_permute_var_family(&mut specs, "vpermq", InputProfile::Int, false, Some("1to8"));
    add_permute_qword_imm_family(&mut specs, "vpermq", InputProfile::Int);
    add_permute_var_family(&mut specs, "vpermw", InputProfile::Int, true, None);
    add_permute_var_family(
        &mut specs,
        "vpermilps",
        InputProfile::F32,
        true,
        Some("1to16"),
    );
    add_permil_imm_family(&mut specs, "vpermilps", InputProfile::F32, "1to16");
    add_permute_var_family(
        &mut specs,
        "vpermilpd",
        InputProfile::F64,
        true,
        Some("1to8"),
    );
    add_permil_imm_family(&mut specs, "vpermilpd", InputProfile::F64, "1to8");
    add_two_table_permute_family(&mut specs, "vpermi2b", InputProfile::Int, None);
    add_two_table_permute_family(&mut specs, "vpermi2w", InputProfile::Int, None);
    add_two_table_permute_family(&mut specs, "vpermi2d", InputProfile::Int, Some("1to16"));
    add_two_table_permute_family(&mut specs, "vpermi2q", InputProfile::Int, Some("1to8"));
    add_two_table_permute_family(&mut specs, "vpermi2ps", InputProfile::F32, Some("1to16"));
    add_two_table_permute_family(&mut specs, "vpermi2pd", InputProfile::F64, Some("1to8"));
    add_two_table_permute_family(&mut specs, "vpermt2b", InputProfile::Int, None);
    add_two_table_permute_family(&mut specs, "vpermt2w", InputProfile::Int, None);
    add_two_table_permute_family(&mut specs, "vpermt2d", InputProfile::Int, Some("1to16"));
    add_two_table_permute_family(&mut specs, "vpermt2q", InputProfile::Int, Some("1to8"));
    add_two_table_permute_family(&mut specs, "vpermt2ps", InputProfile::F32, Some("1to16"));
    add_two_table_permute_family(&mut specs, "vpermt2pd", InputProfile::F64, Some("1to8"));

    for mnemonic in [
        "vpdpbusd",
        "vpdpbusds",
        "vpdpwssd",
        "vpdpwssds",
        "vpmadd52luq",
        "vpmadd52huq",
        "vpermb",
        "vdpbf16ps",
        "vdbpsadbw",
        "vpdpbssd",
        "vpdpbssds",
        "vpdpbsud",
        "vpdpbsuds",
        "vpdpbuud",
        "vpdpbuuds",
        "vpdpwsud",
        "vpdpwsuds",
        "vpdpwusd",
        "vpdpwusds",
        "vpdpwuud",
        "vpdpwuuds",
    ] {
        let imm = if mnemonic == "vdbpsadbw" { "$3, " } else { "" };
        for dst in DST_EXT_REGS {
            for src1 in SRC1_EXT_REGS {
                for rm in RM_EXT_REGS {
                    spec(
                        &mut specs,
                        format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                        format!("{mnemonic} {imm}%zmm{rm}, %zmm{src1}, %zmm{dst}"),
                        InputProfile::Int,
                    );
                }
                spec(
                    &mut specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                    format!("{mnemonic} {imm}(%rax), %zmm{src1}, %zmm{dst}"),
                    InputProfile::Int,
                );
            }
        }
        for (reg_class, dst, src1, src2) in [
            ("xmm", "%xmm1", "%xmm2", "%xmm16"),
            ("ymm", "%ymm1", "%ymm2", "%ymm16"),
            ("zmm", "%zmm1", "%zmm2", "%zmm16"),
        ] {
            spec(
                &mut specs,
                format!("{mnemonic}_{reg_class}_vl"),
                format!("{mnemonic} {imm}{src2}, {src1}, {dst}"),
                InputProfile::Int,
            );
        }
    }

    for mnemonic in ["vpopcntb", "vpopcntw", "vpopcntd", "vpopcntq"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::Int, &[]);
    }
    spec(
        &mut specs,
        "vpopcntd_zmm_mem_broadcast",
        "vpopcntd (%rax){1to16}, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vpopcntq_zmm_mem_broadcast",
        "vpopcntq (%rax){1to8}, %zmm17",
        InputProfile::Int,
    );
    for mnemonic in ["vplzcntd", "vplzcntq"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    spec(
        &mut specs,
        "vplzcntd_zmm_mem_broadcast",
        "vplzcntd (%rax){1to16}, %zmm17",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vplzcntq_zmm_mem_broadcast",
        "vplzcntq (%rax){1to8}, %zmm17",
        InputProfile::Int,
    );
    for mnemonic in ["vpconflictd", "vpconflictq"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::Int, &masked);
    }
    spec(
        &mut specs,
        "vpconflictd_zmm_mem_broadcast",
        "vpconflictd (%rax){1to16}, %zmm17 {%k1}",
        InputProfile::Int,
    );
    spec(
        &mut specs,
        "vpconflictq_zmm_mem_broadcast",
        "vpconflictq (%rax){1to8}, %zmm17 {%k1}",
        InputProfile::Int,
    );

    for mnemonic in ["vpgatherdd", "vgatherdps"] {
        add_vsib_gather_family(&mut specs, mnemonic, 4, 4);
    }
    for mnemonic in ["vpgatherdq", "vgatherdpd"] {
        add_vsib_gather_family(&mut specs, mnemonic, 4, 8);
    }
    for mnemonic in ["vpgatherqd", "vgatherqps"] {
        add_vsib_gather_family(&mut specs, mnemonic, 8, 4);
    }
    for mnemonic in ["vpgatherqq", "vgatherqpd"] {
        add_vsib_gather_family(&mut specs, mnemonic, 8, 8);
    }
    for mnemonic in ["vpscatterdd", "vscatterdps"] {
        add_vsib_scatter_family(&mut specs, mnemonic, 4, 4);
    }
    for mnemonic in ["vpscatterdq", "vscatterdpd"] {
        add_vsib_scatter_family(&mut specs, mnemonic, 4, 8);
    }
    for mnemonic in ["vpscatterqd", "vscatterqps"] {
        add_vsib_scatter_family(&mut specs, mnemonic, 8, 4);
    }
    for mnemonic in ["vpscatterqq", "vscatterqpd"] {
        add_vsib_scatter_family(&mut specs, mnemonic, 8, 8);
    }
    for mnemonic in [
        "vgatherpf0dps",
        "vgatherpf1dps",
        "vscatterpf0dps",
        "vscatterpf1dps",
    ] {
        add_vsib_prefetch_family(&mut specs, mnemonic, 4, 4);
    }
    for mnemonic in [
        "vgatherpf0dpd",
        "vgatherpf1dpd",
        "vscatterpf0dpd",
        "vscatterpf1dpd",
    ] {
        add_vsib_prefetch_family(&mut specs, mnemonic, 4, 8);
    }
    for mnemonic in [
        "vgatherpf0qps",
        "vgatherpf1qps",
        "vscatterpf0qps",
        "vscatterpf1qps",
    ] {
        add_vsib_prefetch_family(&mut specs, mnemonic, 8, 4);
    }
    for mnemonic in [
        "vgatherpf0qpd",
        "vgatherpf1qpd",
        "vscatterpf0qpd",
        "vscatterpf1qpd",
    ] {
        add_vsib_prefetch_family(&mut specs, mnemonic, 8, 8);
    }

    for src1 in SRC1_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                &mut specs,
                format!("vpshufbitqmb_src1_zmm{src1}_rm_zmm{rm}"),
                format!("vpshufbitqmb %zmm{rm}, %zmm{src1}, %k3"),
                InputProfile::Int,
            );
        }
        spec(
            &mut specs,
            format!("vpshufbitqmb_src1_zmm{src1}_mem"),
            format!("vpshufbitqmb (%rax), %zmm{src1}, %k3"),
            InputProfile::Int,
        );
    }
    spec(
        &mut specs,
        "vpshufbitqmb_mask_zmm18_zmm16",
        "vpshufbitqmb %zmm16, %zmm18, %k3 {%k1}",
        InputProfile::Int,
    );
    for (reg_class, src1, src2) in [
        ("xmm", "%xmm18", "%xmm16"),
        ("ymm", "%ymm18", "%ymm16"),
        ("zmm", "%zmm18", "%zmm16"),
    ] {
        spec(
            &mut specs,
            format!("vpshufbitqmb_{reg_class}_vl"),
            format!("vpshufbitqmb {src2}, {src1}, %k3"),
            InputProfile::Int,
        );
    }

    for dst in DST_EXT_REGS {
        for rm in RM_EXT_REGS {
            spec(
                &mut specs,
                format!("vcvtneps2bf16_dst_ymm{dst}_rm_zmm{rm}"),
                format!("vcvtneps2bf16 %zmm{rm}, %ymm{dst}"),
                InputProfile::F32,
            );
        }
        spec(
            &mut specs,
            format!("vcvtneps2bf16_dst_ymm{dst}_mem"),
            format!("vcvtneps2bf16 (%rax), %ymm{dst}"),
            InputProfile::F32,
        );
    }
    for (reg_class, dst, src) in [
        ("xmm", "%xmm1", "%xmm16"),
        ("ymm", "%xmm1", "%ymm16"),
        ("zmm", "%ymm1", "%zmm16"),
    ] {
        spec(
            &mut specs,
            format!("vcvtneps2bf16_{reg_class}_vl"),
            format!("vcvtneps2bf16 {src}, {dst}"),
            InputProfile::F32,
        );
    }
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    &mut specs,
                    format!("vcvtne2ps2bf16_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                    format!("vcvtne2ps2bf16 %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                    InputProfile::F32,
                );
            }
            spec(
                &mut specs,
                format!("vcvtne2ps2bf16_dst_zmm{dst}_src1_zmm{src1}_mem"),
                format!("vcvtne2ps2bf16 (%rax), %zmm{src1}, %zmm{dst}"),
                InputProfile::F32,
            );
        }
    }
    for (reg_class, dst, src1, src2) in [
        ("xmm", "%xmm1", "%xmm2", "%xmm16"),
        ("ymm", "%ymm1", "%ymm2", "%ymm16"),
        ("zmm", "%zmm1", "%zmm2", "%zmm16"),
    ] {
        spec(
            &mut specs,
            format!("vcvtne2ps2bf16_{reg_class}_vl"),
            format!("vcvtne2ps2bf16 {src2}, {src1}, {dst}"),
            InputProfile::F32,
        );
    }
    for mnemonic in ["vcvttps2ibs", "vcvttps2iubs"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::F32, &[]);
    }
    for mnemonic in ["vcvttpd2qqs", "vcvttpd2uqqs"] {
        add_unary_rm_family(&mut specs, mnemonic, InputProfile::F64, &[]);
    }

    for mnemonic in ["vminmaxps", "vminmaxpd"] {
        let profile = if mnemonic.ends_with("ps") {
            InputProfile::F32
        } else {
            InputProfile::F64
        };
        for dst in DST_EXT_REGS {
            for src1 in SRC1_EXT_REGS {
                for rm in RM_EXT_REGS {
                    spec(
                        &mut specs,
                        format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_rm_zmm{rm}"),
                        format!("{mnemonic} $0, %zmm{rm}, %zmm{src1}, %zmm{dst}"),
                        profile,
                    );
                }
                spec(
                    &mut specs,
                    format!("{mnemonic}_dst_zmm{dst}_src1_zmm{src1}_mem"),
                    format!("{mnemonic} $1, (%rax), %zmm{src1}, %zmm{dst}"),
                    profile,
                );
            }
        }
        for (reg_class, dst, src1, src2) in [
            ("xmm", "%xmm1", "%xmm2", "%xmm16"),
            ("ymm", "%ymm1", "%ymm2", "%ymm16"),
            ("zmm", "%zmm1", "%zmm2", "%zmm16"),
        ] {
            spec(
                &mut specs,
                format!("{mnemonic}_{reg_class}_vl"),
                format!("{mnemonic} $0, {src2}, {src1}, {dst}"),
                profile,
            );
        }
    }
    for dst in DST_EXT_REGS {
        for src1 in SRC1_EXT_REGS {
            for rm in RM_EXT_REGS {
                spec(
                    &mut specs,
                    format!("vminmaxss_dst_xmm{dst}_src1_xmm{src1}_rm_xmm{rm}"),
                    format!("vminmaxss $0, %xmm{rm}, %xmm{src1}, %xmm{dst}"),
                    InputProfile::F32,
                );
                spec(
                    &mut specs,
                    format!("vminmaxsd_dst_xmm{dst}_src1_xmm{src1}_rm_xmm{rm}"),
                    format!("vminmaxsd $0, %xmm{rm}, %xmm{src1}, %xmm{dst}"),
                    InputProfile::F64,
                );
            }
            spec(
                &mut specs,
                format!("vminmaxss_dst_xmm{dst}_src1_xmm{src1}_mem"),
                format!("vminmaxss $1, (%rax), %xmm{src1}, %xmm{dst}"),
                InputProfile::F32,
            );
            spec(
                &mut specs,
                format!("vminmaxsd_dst_xmm{dst}_src1_xmm{src1}_mem"),
                format!("vminmaxsd $1, (%rax), %xmm{src1}, %xmm{dst}"),
                InputProfile::F64,
            );
        }
    }

    for mnemonic in ["vaddph", "vmulph", "vsubph", "vdivph", "vminph", "vmaxph"] {
        add_ternary_family(&mut specs, mnemonic, InputProfile::F16, &masked);
        spec(
            &mut specs,
            format!("{mnemonic}_zmm_mem_broadcast"),
            format!("{mnemonic} (%rax){{1to32}}, %zmm18, %zmm17"),
            InputProfile::F16,
        );
    }
    add_unary_rm_family(&mut specs, "vsqrtph", InputProfile::F16, &masked);
    spec(
        &mut specs,
        "vsqrtph_zmm_mem_broadcast",
        "vsqrtph (%rax){1to32}, %zmm17",
        InputProfile::F16,
    );
    for mnemonic in [
        "vaddsh", "vmulsh", "vsubsh", "vdivsh", "vminsh", "vmaxsh", "vsqrtsh",
    ] {
        add_scalar_fp_family(&mut specs, mnemonic, InputProfile::F16, &masked);
    }

    specs
}

fn set_from_manifest(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn set_from_slice(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn profile_for_spec_mnemonic(mnemonic: &str) -> InputProfile {
    let float_family = [
        "vadd",
        "vsub",
        "vmul",
        "vdiv",
        "vmax",
        "vmin",
        "vsqrt",
        "vrcp",
        "vrsqrt",
        "vscalef",
        "vrange",
        "vreduce",
        "vrndscale",
        "vget",
        "vfixup",
        "vexp",
        "vcmp",
        "vfpclass",
        "vcomi",
        "vucomi",
        "vfm",
        "vfnm",
        "vfcm",
        "vfmul",
        "vcvt",
        "vmovap",
        "vmovu",
        "vmovh",
        "vmovl",
        "vmovs",
        "vunpck",
        "vshuf",
    ];
    if !float_family
        .iter()
        .any(|prefix| mnemonic.starts_with(prefix))
    {
        return InputProfile::Int;
    }
    if mnemonic.contains("ph") || mnemonic.ends_with("sh") {
        InputProfile::F16
    } else if mnemonic.contains("pd") || mnemonic.ends_with("sd") {
        InputProfile::F64
    } else {
        InputProfile::F32
    }
}

fn evex_diff_cases_from_spec_for_mnemonics(
    label_prefix: &str,
    mnemonics: &BTreeSet<String>,
) -> (Vec<DiffCase>, BTreeSet<String>) {
    let mut expected = BTreeSet::new();
    let mut cases = Vec::new();

    for row in avx512_spec_evex_rows() {
        if !mnemonics.contains(&row.key.mnemonic) {
            continue;
        }
        for variant in evex_case_variants_for_row(&row) {
            let case_id = spec_case_variant_id(&row, variant);
            expected.insert(case_id.clone());
            let id = cases.len() as u32;
            cases.push(DiffCase {
                id,
                label: format!("{label_prefix}::{case_id}"),
                asm: row.key.mnemonic.clone(),
                op: raw_evex_spec_bytes_for_variant(&row, variant),
                input: input_for(id, profile_for_spec_mnemonic(&row.key.mnemonic)),
            });
        }
    }

    (cases, expected)
}

fn unimplemented_evex_diff_cases_from_spec() -> (Vec<DiffCase>, BTreeSet<String>) {
    evex_diff_cases_from_spec_for_mnemonics(
        "unimplemented",
        &set_from_manifest(UNIMPLEMENTED_AVX512),
    )
}

fn classified_avx512_evex_mnemonics() -> BTreeSet<String> {
    let mut classified = set_from_slice(RAX_EVEX_SIMD_DIFF_MNEMONICS);
    classified.extend(set_from_manifest(UNIMPLEMENTED_AVX512));
    classified
}

fn classified_avx512_evex_diff_cases_from_spec() -> Vec<DiffCase> {
    evex_diff_cases_from_spec_for_mnemonics("avx512", &classified_avx512_evex_mnemonics()).0
}

fn avx512_spec_case_variant_ids() -> BTreeSet<String> {
    avx512_spec_evex_rows()
        .iter()
        .flat_map(|row| {
            evex_case_variants_for_row(row)
                .into_iter()
                .map(|variant| spec_case_variant_id(row, variant))
        })
        .collect()
}

fn assert_spec_diff_corpus_shape(
    cases: &[DiffCase],
    label_prefix: &str,
    mnemonics: &BTreeSet<String>,
    require_full_shape: bool,
) {
    let actual = cases
        .iter()
        .map(|case| {
            case.label
                .strip_prefix(label_prefix)
                .unwrap_or_else(|| panic!("{} must start with {label_prefix}", case.label))
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    let mut failures = Vec::new();
    let mut register_or_memory_rows = 0usize;
    let mut memory_only_rows = 0usize;
    let mut vsib_memory_rows = 0usize;
    let mut vector_register_rows = 0usize;
    let mut gpr_register_rows = 0usize;
    let mut mask_register_rows = 0usize;
    let mut high_vector_bucket_cases = 0usize;

    for row in avx512_spec_evex_rows() {
        if !mnemonics.contains(&row.key.mnemonic) {
            continue;
        }

        let mut expect_memory = false;
        let mut expect_register = false;
        match row.key.form {
            EvexOperandForm::RegisterOnly => expect_register = true,
            EvexOperandForm::RegisterOrMemory => {
                expect_register = true;
                expect_memory = true;
                register_or_memory_rows += 1;
            }
            EvexOperandForm::MemoryOnly => {
                expect_memory = true;
                memory_only_rows += 1;
            }
            EvexOperandForm::VsibMemory => {
                expect_memory = true;
                vsib_memory_rows += 1;
            }
        }

        if expect_memory {
            let id = spec_case_variant_id(
                &row,
                EvexCaseVariant {
                    mode: EvexAsmMode::Memory,
                    rm_reg: None,
                },
            );
            if !actual.contains(&id) {
                failures.push(format!("missing memory case: {id}"));
            }
        }

        if !expect_register {
            continue;
        }

        let rm_buckets: &[u8] = match evex_rm_register_class(&row) {
            EvexRmRegisterClass::Vector => {
                vector_register_rows += 1;
                &[0, 8, 16, 24]
            }
            EvexRmRegisterClass::Gpr => {
                gpr_register_rows += 1;
                &[0, 8]
            }
            EvexRmRegisterClass::Mask => {
                mask_register_rows += 1;
                &[0]
            }
            EvexRmRegisterClass::Unknown => {
                failures.push(format!(
                    "unknown register r/m class: {} {:?} {}",
                    row.source, row.key, row.cell
                ));
                continue;
            }
        };

        for rm_reg in rm_buckets {
            let id = spec_case_variant_id(
                &row,
                EvexCaseVariant {
                    mode: EvexAsmMode::Register,
                    rm_reg: Some(*rm_reg),
                },
            );
            if !actual.contains(&id) {
                failures.push(format!("missing register bucket case: {id}"));
            }
            if *rm_reg >= 16 {
                high_vector_bucket_cases += 1;
            }
        }
    }

    assert!(
        failures.is_empty(),
        "AVX-512 EVEX differential corpus does not expand every operand form and r/m bucket:\n{}",
        failures.into_iter().take(80).collect::<Vec<_>>().join("\n")
    );
    if require_full_shape {
        assert!(
            register_or_memory_rows > 0
                && memory_only_rows > 0
                && vsib_memory_rows > 0
                && vector_register_rows > 0
                && gpr_register_rows > 0
                && mask_register_rows > 0
                && high_vector_bucket_cases > 0,
            "AVX-512 EVEX differential corpus shape check was unexpectedly degenerate: \
             r/m={register_or_memory_rows}, mem={memory_only_rows}, vsib={vsib_memory_rows}, \
             vector={vector_register_rows}, gpr={gpr_register_rows}, mask={mask_register_rows}, \
             high-vector-buckets={high_vector_bucket_cases}"
        );
    }
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
            "-x86-asm-syntax=att",
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

fn assembled_cases(llvm_mc: &Path) -> Vec<DiffCase> {
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    let specs = generated_specs();
    assert_requested_mnemonic_coverage(&specs);

    for spec in specs {
        let op = if let Some(op) = spec.op {
            op
        } else {
            let Some(op) = assemble_case(llvm_mc, &spec.asm) else {
                failures.push(format!("{}: {}", spec.label, spec.asm));
                continue;
            };
            op
        };
        assert!(
            op.first() == Some(&0x62),
            "{} assembled outside EVEX encoding: {:02x?}",
            spec.label,
            op
        );
        let id = cases.len() as u32;
        cases.push(DiffCase {
            id,
            label: spec.label,
            asm: spec.asm,
            op,
            input: input_for(id, spec.profile),
        });
    }

    assert!(
        failures.is_empty(),
        "EVEX differential corpus failed to assemble:\n{}",
        failures.join("\n")
    );
    assert_assembled_mnemonic_coverage(&cases);
    assert_evex_form_coverage(&cases);

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
    let build_dir = root.join("target/x86_64-evex-diff");
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
            "-mavx512cd",
            "-mavx512fp16",
            "-mavx512vnni",
            "-mavx512ifma",
            "-mavx512vpopcntdq",
            "-mavx512vbmi",
            "-mavx512vbmi2",
            "-mavx512bitalg",
            "-mavx512bf16",
            "-mavxvnni",
            "-mvp2intersect",
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

fn assembled_rax_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    profile: InputProfile,
    id: u32,
) -> DiffCase {
    let op =
        assemble_case(llvm_mc, asm).unwrap_or_else(|| panic!("{label}: failed to assemble {asm}"));
    DiffCase {
        id,
        label: label.to_string(),
        asm: asm.to_string(),
        op,
        input: input_for(id, profile),
    }
}

fn assert_evex_scalar_f32_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    op: fn(f32, f32) -> f32,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::F32, 0x5100);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let mut expected = [0u8; 64];
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = zmm_to_bytes(case.input.zmm[src2]);
    expected[4..16].copy_from_slice(&src1_bytes[4..16]);

    let lane_active = mask
        .map(|(k, _)| (case.input.k[k] & 1) != 0)
        .unwrap_or(true);
    if lane_active {
        let a = f32::from_le_bytes(src1_bytes[0..4].try_into().unwrap());
        let b = f32::from_le_bytes(src2_bytes[0..4].try_into().unwrap());
        expected[0..4].copy_from_slice(&op(a, b).to_le_bytes());
    } else if mask.is_some_and(|(_, zeroing)| zeroing) {
        expected[0..4].fill(0);
    } else {
        expected[0..4].copy_from_slice(&dst_bytes[0..4]);
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_scalar_f64_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    op: fn(f64, f64) -> f64,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::F64, 0x5101);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let mut expected = [0u8; 64];
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = zmm_to_bytes(case.input.zmm[src2]);
    expected[8..16].copy_from_slice(&src1_bytes[8..16]);

    let lane_active = mask
        .map(|(k, _)| (case.input.k[k] & 1) != 0)
        .unwrap_or(true);
    if lane_active {
        let a = f64::from_le_bytes(src1_bytes[0..8].try_into().unwrap());
        let b = f64::from_le_bytes(src2_bytes[0..8].try_into().unwrap());
        expected[0..8].copy_from_slice(&op(a, b).to_le_bytes());
    } else if mask.is_some_and(|(_, zeroing)| zeroing) {
        expected[0..8].fill(0);
    } else {
        expected[0..8].copy_from_slice(&dst_bytes[0..8]);
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn x86_min_f32(a: f32, b: f32) -> f32 {
    if (a == 0.0 && b == 0.0) || a.is_nan() || b.is_nan() {
        b
    } else if a < b {
        a
    } else {
        b
    }
}

fn x86_max_f32(a: f32, b: f32) -> f32 {
    if (a == 0.0 && b == 0.0) || a.is_nan() || b.is_nan() {
        b
    } else if a > b {
        a
    } else {
        b
    }
}

fn x86_min_f64(a: f64, b: f64) -> f64 {
    if (a == 0.0 && b == 0.0) || a.is_nan() || b.is_nan() {
        b
    } else if a < b {
        a
    } else {
        b
    }
}

fn x86_max_f64(a: f64, b: f64) -> f64 {
    if (a == 0.0 && b == 0.0) || a.is_nan() || b.is_nan() {
        b
    } else if a > b {
        a
    } else {
        b
    }
}

fn assert_evex_scalar_f32_bits(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    src1_bits: u32,
    src2_bits: u32,
    expected_bits: u32,
) {
    let mut case = assembled_rax_case(llvm_mc, label, asm, InputProfile::F32, 0x5300);
    let mut src1_bytes = zmm_to_bytes(case.input.zmm[18]);
    src1_bytes[0..4].copy_from_slice(&src1_bits.to_le_bytes());
    case.input.zmm[18] = zmm_from_bytes(src1_bytes);
    let mut src2_bytes = zmm_to_bytes(case.input.zmm[16]);
    src2_bytes[0..4].copy_from_slice(&src2_bits.to_le_bytes());
    case.input.zmm[16] = zmm_from_bytes(src2_bytes);

    let out = run_rax(&case);
    let out_bytes = zmm_to_bytes(out.zmm[17]);
    assert_eq!(
        u32::from_le_bytes(out_bytes[0..4].try_into().unwrap()),
        expected_bits,
        "{label}"
    );
}

fn assert_evex_scalar_f64_bits(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    src1_bits: u64,
    src2_bits: u64,
    expected_bits: u64,
) {
    let mut case = assembled_rax_case(llvm_mc, label, asm, InputProfile::F64, 0x5301);
    let mut src1_bytes = zmm_to_bytes(case.input.zmm[18]);
    src1_bytes[0..8].copy_from_slice(&src1_bits.to_le_bytes());
    case.input.zmm[18] = zmm_from_bytes(src1_bytes);
    let mut src2_bytes = zmm_to_bytes(case.input.zmm[16]);
    src2_bytes[0..8].copy_from_slice(&src2_bits.to_le_bytes());
    case.input.zmm[16] = zmm_from_bytes(src2_bytes);

    let out = run_rax(&case);
    let out_bytes = zmm_to_bytes(out.zmm[17]);
    assert_eq!(
        u64::from_le_bytes(out_bytes[0..8].try_into().unwrap()),
        expected_bits,
        "{label}"
    );
}

fn assert_evex_fp_bitwise_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    profile: InputProfile,
    elem_size: usize,
    op: fn(u8, u8) -> u8,
    broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, profile, 0x5200);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = if broadcast {
        let mut bytes = [0u8; 64];
        for lane in 0..(64 / elem_size) {
            let base = lane * elem_size;
            bytes[base..base + elem_size].copy_from_slice(&case.input.scratch[..elem_size]);
        }
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        let lane_active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if lane_active {
            for byte in 0..elem_size {
                expected[base + byte] = op(src1_bytes[base + byte], src2_bytes[base + byte]);
            }
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            expected[base..base + elem_size].fill(0);
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn ternlog_bytes(dest: &[u8; 64], src1: &[u8; 64], src2: &[u8; 64], imm: u8) -> [u8; 64] {
    let mut expected = [0u8; 64];
    for byte in 0..64 {
        let a = dest[byte];
        let b = src1[byte];
        let c = src2[byte];
        for bit in 0..8 {
            let idx = (((a >> bit) & 1) << 2) | (((b >> bit) & 1) << 1) | ((c >> bit) & 1);
            expected[byte] |= ((imm >> idx) & 1) << bit;
        }
    }
    expected
}

fn assert_evex_ternlog_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    imm: u8,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5400);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = zmm_to_bytes(case.input.zmm[src2]);
    let raw = ternlog_bytes(&dst_bytes, &src1_bytes, &src2_bytes, imm);

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        let lane_active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if lane_active {
            expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            expected[base..base + elem_size].fill(0);
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_test_mask_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    inverted: bool,
    src2_from_broadcast: bool,
    writemask: Option<usize>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5401);
    let out = run_rax(&case);
    let src1_bytes = zmm_to_bytes(case.input.zmm[18]);
    let src2_bytes = if src2_from_broadcast {
        let mut bytes = [0u8; 64];
        for lane in 0..(64 / elem_size) {
            let base = lane * elem_size;
            bytes[base..base + elem_size].copy_from_slice(&case.input.scratch[..elem_size]);
        }
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[16])
    };

    let mut expected = 0u64;
    for lane in 0..(64 / elem_size) {
        let active = writemask
            .map(|k| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if !active {
            continue;
        }
        let base = lane * elem_size;
        let any =
            (0..elem_size).any(|byte| (src1_bytes[base + byte] & src2_bytes[base + byte]) != 0);
        if any != inverted {
            expected |= 1u64 << lane;
        }
    }

    assert_eq!(out.k[3], expected, "{label}");
}

fn read_elem(bytes: &[u8; 64], lane: usize, elem_size: usize) -> u64 {
    let base = lane * elem_size;
    let mut le = [0u8; 8];
    le[..elem_size].copy_from_slice(&bytes[base..base + elem_size]);
    u64::from_le_bytes(le)
}

fn write_elem(bytes: &mut [u8; 64], lane: usize, elem_size: usize, value: u64) {
    let base = lane * elem_size;
    bytes[base..base + elem_size].copy_from_slice(&value.to_le_bytes()[..elem_size]);
}

fn rotate_value(value: u64, count: u64, elem_size: usize, left: bool) -> u64 {
    match elem_size {
        4 => {
            let value = value as u32;
            let count = (count & 31) as u32;
            if left {
                value.rotate_left(count) as u64
            } else {
                value.rotate_right(count) as u64
            }
        }
        8 => {
            let count = (count & 63) as u32;
            if left {
                value.rotate_left(count)
            } else {
                value.rotate_right(count)
            }
        }
        _ => unreachable!("rotate tests cover dword/qword elements"),
    }
}

fn elem_mask(elem_size: usize) -> u128 {
    let bits = elem_size * 8;
    if bits == 64 {
        u64::MAX as u128
    } else {
        (1u128 << bits) - 1
    }
}

fn funnel_shift_value(
    primary: u64,
    secondary: u64,
    count: u64,
    elem_size: usize,
    left: bool,
) -> u64 {
    let bits = (elem_size * 8) as u32;
    let count = (count & (bits as u64 - 1)) as u32;
    let mask = elem_mask(elem_size);
    let primary = primary as u128 & mask;
    let secondary = secondary as u128 & mask;
    let result = if count == 0 {
        primary
    } else if left {
        (primary << count) | (secondary >> (bits - count))
    } else {
        (primary >> count) | (secondary << (bits - count))
    };

    (result & mask) as u64
}

fn broadcast_bytes(input: &InCase, elem_size: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        bytes[base..base + elem_size].copy_from_slice(&input.scratch[..elem_size]);
    }
    bytes
}

fn assert_evex_rotate_imm_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    left: bool,
    count: u8,
    src_from_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5500);
    let out = run_rax(&case);
    let dst = 17;
    let src = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src_bytes = if src_from_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else {
        zmm_to_bytes(case.input.zmm[src])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            let value = read_elem(&src_bytes, lane, elem_size);
            write_elem(
                &mut expected,
                lane,
                elem_size,
                rotate_value(value, count as u64, elem_size, left),
            );
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            let base = lane * elem_size;
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_rotate_per_elem_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    left: bool,
    count_from_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5501);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let count_src = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let count_bytes = if count_from_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else {
        zmm_to_bytes(case.input.zmm[count_src])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            let value = read_elem(&src1_bytes, lane, elem_size);
            let count = read_elem(&count_bytes, lane, elem_size);
            write_elem(
                &mut expected,
                lane,
                elem_size,
                rotate_value(value, count, elem_size, left),
            );
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            let base = lane * elem_size;
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_funnel_shift_imm_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    left: bool,
    count: u8,
    src2_from_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5504);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let primary_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let secondary_bytes = if src2_from_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            let primary = read_elem(&primary_bytes, lane, elem_size);
            let secondary = read_elem(&secondary_bytes, lane, elem_size);
            expected[base..base + elem_size].copy_from_slice(
                &funnel_shift_value(primary, secondary, count as u64, elem_size, left)
                    .to_le_bytes()[..elem_size],
            );
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_funnel_shift_var_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    left: bool,
    count_from_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5505);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let count_src = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let secondary_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let count_bytes = if count_from_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else {
        zmm_to_bytes(case.input.zmm[count_src])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            let primary = read_elem(&dst_bytes, lane, elem_size);
            let secondary = read_elem(&secondary_bytes, lane, elem_size);
            let count = read_elem(&count_bytes, lane, elem_size);
            expected[base..base + elem_size].copy_from_slice(
                &funnel_shift_value(primary, secondary, count, elem_size, left).to_le_bytes()
                    [..elem_size],
            );
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn count_value(value: u64, elem_size: usize, leading_zeroes: bool) -> u64 {
    if leading_zeroes {
        match elem_size {
            4 => (value as u32).leading_zeros() as u64,
            8 => value.leading_zeros() as u64,
            _ => unreachable!("LZCNT tests cover dword/qword elements"),
        }
    } else {
        value.count_ones() as u64
    }
}

fn assert_evex_count_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    leading_zeroes: bool,
    src_from_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5503);
    let out = run_rax(&case);
    let dst = 17;
    let src = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src_bytes = if src_from_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else {
        zmm_to_bytes(case.input.zmm[src])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            let value = read_elem(&src_bytes, lane, elem_size);
            expected[base..base + elem_size].copy_from_slice(
                &count_value(value, elem_size, leading_zeroes).to_le_bytes()[..elem_size],
            );
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_blend_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    profile: InputProfile,
    elem_size: usize,
    src2_memory: bool,
    src2_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, profile, 0x5506);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = if src2_memory {
        if src2_broadcast {
            broadcast_bytes(&case.input, elem_size)
        } else {
            let mut bytes = [0u8; 64];
            bytes.copy_from_slice(&case.input.scratch[..64]);
            bytes
        }
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };

    let mut expected = [0u8; 64];
    for lane in 0..(64 / elem_size) {
        let base = lane * elem_size;
        let select_src2 = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if select_src2 {
            expected[base..base + elem_size].copy_from_slice(&src2_bytes[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&src1_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

#[derive(Clone, Copy)]
enum TestPackKind {
    SignedWordToSignedByte,
    SignedDwordToSignedWord,
    UnsignedWordToUnsignedByte,
    UnsignedDwordToUnsignedWord,
}

impl TestPackKind {
    fn src_elem_size(self) -> usize {
        match self {
            TestPackKind::SignedWordToSignedByte | TestPackKind::UnsignedWordToUnsignedByte => 2,
            TestPackKind::SignedDwordToSignedWord | TestPackKind::UnsignedDwordToUnsignedWord => 4,
        }
    }

    fn dst_elem_size(self) -> usize {
        match self {
            TestPackKind::SignedWordToSignedByte | TestPackKind::UnsignedWordToUnsignedByte => 1,
            TestPackKind::SignedDwordToSignedWord | TestPackKind::UnsignedDwordToUnsignedWord => 2,
        }
    }
}

fn pack_source_bytes(kind: TestPackKind, salt: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    match kind.src_elem_size() {
        2 => {
            let values: [i16; 16] = [
                -400, -129, -128, -1, 0, 1, 127, 128, 255, 256, 512, -255, 42, -42, 200, -200,
            ];
            for lane in 0..32 {
                let value = values[(lane + salt) % values.len()];
                bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        4 => {
            let values: [i32; 16] = [
                -100_000, -32_769, -32_768, -1, 0, 1, 32_767, 32_768, 65_535, 65_536, 100_000,
                -65_535, 1234, -1234, 50_000, -50_000,
            ];
            for lane in 0..16 {
                let value = values[(lane + salt) % values.len()];
                bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => unreachable!("pack source elements are word or dword sized"),
    }
    bytes
}

fn test_pack_saturate_elem(kind: TestPackKind, src: &[u8]) -> u64 {
    match kind {
        TestPackKind::SignedWordToSignedByte => {
            let value = i16::from_le_bytes([src[0], src[1]]);
            value.clamp(i8::MIN as i16, i8::MAX as i16) as i8 as u8 as u64
        }
        TestPackKind::UnsignedWordToUnsignedByte => {
            let value = i16::from_le_bytes([src[0], src[1]]);
            value.clamp(0, u8::MAX as i16) as u8 as u64
        }
        TestPackKind::SignedDwordToSignedWord => {
            let value = i32::from_le_bytes([src[0], src[1], src[2], src[3]]);
            value.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16 as u64
        }
        TestPackKind::UnsignedDwordToUnsignedWord => {
            let value = i32::from_le_bytes([src[0], src[1], src[2], src[3]]);
            value.clamp(0, u16::MAX as i32) as u16 as u64
        }
    }
}

fn assert_evex_pack_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    kind: TestPackKind,
    vl_bytes: usize,
    src2_memory: bool,
    src2_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let mut case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550d);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let src_elem_size = kind.src_elem_size();
    let dst_elem_size = kind.dst_elem_size();

    let dst_old = pack_source_bytes(kind, 11);
    let src1_bytes = pack_source_bytes(kind, 3);
    let src2_bytes = pack_source_bytes(kind, 7);
    case.input.zmm[dst] = zmm_from_bytes(dst_old);
    case.input.zmm[src1] = zmm_from_bytes(src1_bytes);
    case.input.zmm[src2] = zmm_from_bytes(src2_bytes);
    if src2_memory {
        case.input.scratch[..64].copy_from_slice(&src2_bytes);
        if src2_broadcast {
            let broadcast_value: i32 = match kind {
                TestPackKind::SignedDwordToSignedWord => -50_000,
                TestPackKind::UnsignedDwordToUnsignedWord => 70_000,
                _ => unreachable!("word-to-byte pack instructions do not broadcast"),
            };
            case.input.scratch[..4].copy_from_slice(&broadcast_value.to_le_bytes());
        }
    }

    let out = run_rax(&case);
    let src2_effective = if src2_memory {
        if src2_broadcast {
            broadcast_bytes(&case.input, src_elem_size)
        } else {
            let mut bytes = [0u8; 64];
            bytes.copy_from_slice(&case.input.scratch[..64]);
            bytes
        }
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };

    let mut raw = [0u8; 64];
    let src_elems_per_128 = 16 / src_elem_size;
    for block_base in (0..vl_bytes).step_by(16) {
        for lane in 0..src_elems_per_128 {
            let src_base = block_base + lane * src_elem_size;
            let dst_base = block_base + lane * dst_elem_size;
            raw[dst_base..dst_base + dst_elem_size].copy_from_slice(
                &test_pack_saturate_elem(kind, &src1_bytes[src_base..src_base + src_elem_size])
                    .to_le_bytes()[..dst_elem_size],
            );
        }
        for lane in 0..src_elems_per_128 {
            let src_base = block_base + lane * src_elem_size;
            let dst_base = block_base + (src_elems_per_128 + lane) * dst_elem_size;
            raw[dst_base..dst_base + dst_elem_size].copy_from_slice(
                &test_pack_saturate_elem(kind, &src2_effective[src_base..src_base + src_elem_size])
                    .to_le_bytes()[..dst_elem_size],
            );
        }
    }

    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / dst_elem_size) {
        let base = lane * dst_elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + dst_elem_size].copy_from_slice(&raw[base..base + dst_elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + dst_elem_size]
                .copy_from_slice(&dst_old[base..base + dst_elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

#[derive(Clone, Copy)]
enum TestShuffleImmKind {
    Dword,
    HighWord,
    LowWord,
}

impl TestShuffleImmKind {
    fn elem_size(self) -> usize {
        match self {
            TestShuffleImmKind::Dword => 4,
            TestShuffleImmKind::HighWord | TestShuffleImmKind::LowWord => 2,
        }
    }
}

fn shuffle_control_bytes() -> [u8; 64] {
    let mut control = [0u8; 64];
    for block_base in (0..64).step_by(16) {
        for lane in 0..16 {
            control[block_base + lane] = if lane % 5 == 0 {
                0x80
            } else {
                (15 - lane) as u8
            };
        }
    }
    control
}

fn assert_evex_pshufb_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    vl_bytes: usize,
    control_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let mut case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550e);
    let dst = 17;
    let src1 = 18;
    let control_reg = 16;
    let control = shuffle_control_bytes();
    case.input.zmm[control_reg] = zmm_from_bytes(control);
    if control_memory {
        case.input.scratch[..64].copy_from_slice(&control);
    }

    let out = run_rax(&case);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let control_bytes = if control_memory {
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&case.input.scratch[..64]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[control_reg])
    };
    let dst_old = zmm_to_bytes(case.input.zmm[dst]);
    let mut raw = [0u8; 64];

    for block_base in (0..vl_bytes).step_by(16) {
        for lane in 0..16 {
            let selector = control_bytes[block_base + lane];
            raw[block_base + lane] = if selector & 0x80 != 0 {
                0
            } else {
                src1_bytes[block_base + (selector & 0x0f) as usize]
            };
        }
    }

    let mut expected = [0u8; 64];
    for lane in 0..vl_bytes {
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[lane] = raw[lane];
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[lane] = dst_old[lane];
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_palignr_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    vl_bytes: usize,
    imm: usize,
    src2_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550f);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let dst_old = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = if src2_memory {
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&case.input.scratch[..64]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };
    let mut raw = [0u8; 64];

    for block_base in (0..vl_bytes).step_by(16) {
        let mut concatenated = [0u8; 32];
        concatenated[..16].copy_from_slice(&src2_bytes[block_base..block_base + 16]);
        concatenated[16..].copy_from_slice(&src1_bytes[block_base..block_base + 16]);
        for lane in 0..16 {
            let idx = imm + lane;
            raw[block_base + lane] = if idx < 32 { concatenated[idx] } else { 0 };
        }
    }

    let mut expected = [0u8; 64];
    for lane in 0..vl_bytes {
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[lane] = raw[lane];
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[lane] = dst_old[lane];
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_shuffle_imm_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    kind: TestShuffleImmKind,
    vl_bytes: usize,
    imm: u8,
    src_memory: bool,
    src_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5510);
    let out = run_rax(&case);
    let dst = 17;
    let src = 16;
    let elem_size = kind.elem_size();
    let dst_old = zmm_to_bytes(case.input.zmm[dst]);
    let src_bytes = if src_memory {
        if src_broadcast {
            broadcast_bytes(&case.input, elem_size)
        } else {
            let mut bytes = [0u8; 64];
            bytes.copy_from_slice(&case.input.scratch[..64]);
            bytes
        }
    } else {
        zmm_to_bytes(case.input.zmm[src])
    };

    let mut raw = [0u8; 64];
    for block_base in (0..vl_bytes).step_by(16) {
        match kind {
            TestShuffleImmKind::Dword => {
                for dst_lane in 0..4 {
                    let src_lane = ((imm >> (dst_lane * 2)) & 0x3) as usize;
                    let dst_base = block_base + dst_lane * 4;
                    let src_base = block_base + src_lane * 4;
                    raw[dst_base..dst_base + 4].copy_from_slice(&src_bytes[src_base..src_base + 4]);
                }
            }
            TestShuffleImmKind::HighWord => {
                raw[block_base..block_base + 8]
                    .copy_from_slice(&src_bytes[block_base..block_base + 8]);
                for dst_lane in 0..4 {
                    let src_lane = ((imm >> (dst_lane * 2)) & 0x3) as usize;
                    let dst_base = block_base + 8 + dst_lane * 2;
                    let src_base = block_base + 8 + src_lane * 2;
                    raw[dst_base..dst_base + 2].copy_from_slice(&src_bytes[src_base..src_base + 2]);
                }
            }
            TestShuffleImmKind::LowWord => {
                for dst_lane in 0..4 {
                    let src_lane = ((imm >> (dst_lane * 2)) & 0x3) as usize;
                    let dst_base = block_base + dst_lane * 2;
                    let src_base = block_base + src_lane * 2;
                    raw[dst_base..dst_base + 2].copy_from_slice(&src_bytes[src_base..src_base + 2]);
                }
                raw[block_base + 8..block_base + 16]
                    .copy_from_slice(&src_bytes[block_base + 8..block_base + 16]);
            }
        }
    }

    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_old[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_psadbw_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    vl_bytes: usize,
    src2_memory: bool,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5511);
    let out = run_rax(&case);
    let src1_bytes = zmm_to_bytes(case.input.zmm[18]);
    let src2_bytes = if src2_memory {
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&case.input.scratch[..64]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[16])
    };
    let mut expected = [0u8; 64];

    for group_base in (0..vl_bytes).step_by(8) {
        let mut sum = 0u16;
        for lane in 0..8 {
            let a = src1_bytes[group_base + lane] as i16;
            let b = src2_bytes[group_base + lane] as i16;
            sum = sum.wrapping_add((a - b).unsigned_abs());
        }
        expected[group_base..group_base + 2].copy_from_slice(&sum.to_le_bytes());
    }

    assert_eq!(out.zmm[17], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_mask_to_vec_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    vl_bytes: usize,
    src_mask: usize,
    dst: usize,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5507);
    let out = run_rax(&case);
    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        if (case.input.k[src_mask] >> lane) & 1 != 0 {
            expected[base..base + elem_size].fill(0xff);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_vec_to_mask_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    vl_bytes: usize,
    src: usize,
    dst_mask: usize,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5508);
    let out = run_rax(&case);
    let src_bytes = zmm_to_bytes(case.input.zmm[src]);
    let mut expected = 0u64;
    for lane in 0..(vl_bytes / elem_size) {
        if src_bytes[lane * elem_size + elem_size - 1] & 0x80 != 0 {
            expected |= 1u64 << lane;
        }
    }

    assert_eq!(out.k[dst_mask], expected, "{label}");
}

fn assert_evex_mask_broadcast_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    vl_bytes: usize,
    src_mask_bits: usize,
    src_mask: usize,
    dst: usize,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5509);
    let out = run_rax(&case);
    let mask = if src_mask_bits == 8 { 0xff } else { 0xffff };
    let value = case.input.k[src_mask] & mask;
    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / elem_size) {
        write_elem(&mut expected, lane, elem_size, value);
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn test_extend_int_value(src: &[u8], src_elem_size: usize, signed: bool) -> u64 {
    if signed {
        match src_elem_size {
            1 => src[0] as i8 as i64 as u64,
            2 => i16::from_le_bytes([src[0], src[1]]) as i64 as u64,
            4 => i32::from_le_bytes([src[0], src[1], src[2], src[3]]) as i64 as u64,
            _ => unreachable!("integer extend tests cover byte/word/dword sources"),
        }
    } else {
        match src_elem_size {
            1 => src[0] as u64,
            2 => u16::from_le_bytes([src[0], src[1]]) as u64,
            4 => u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as u64,
            _ => unreachable!("integer extend tests cover byte/word/dword sources"),
        }
    }
}

fn assert_evex_int_extend_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    src_elem_size: usize,
    dst_elem_size: usize,
    vl_bytes: usize,
    signed: bool,
    src_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550f);
    let out = run_rax(&case);
    let dst = 17;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src_bytes = if src_memory {
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&case.input.scratch[..64]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[16])
    };
    let num_elems = vl_bytes / dst_elem_size;
    let mut raw = [0u8; 64];

    for lane in 0..num_elems {
        let src_base = lane * src_elem_size;
        let dst_base = lane * dst_elem_size;
        let value = test_extend_int_value(
            &src_bytes[src_base..src_base + src_elem_size],
            src_elem_size,
            signed,
        );
        raw[dst_base..dst_base + dst_elem_size]
            .copy_from_slice(&value.to_le_bytes()[..dst_elem_size]);
    }

    let mut expected = [0u8; 64];
    for lane in 0..num_elems {
        let base = lane * dst_elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + dst_elem_size].copy_from_slice(&raw[base..base + dst_elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + dst_elem_size]
                .copy_from_slice(&dst_bytes[base..base + dst_elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

#[derive(Clone, Copy)]
enum TestNarrowMode {
    Truncate,
    SignedSaturate,
    UnsignedSaturate,
}

fn narrow_source_bytes(src_elem_size: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    match src_elem_size {
        2 => {
            let values: [i16; 16] = [
                -400, -129, -128, -1, 0, 1, 127, 128, 255, 256, 512, -255, 42, -42, 200, -200,
            ];
            for lane in 0..32 {
                let value = values[lane % values.len()];
                bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        4 => {
            let values: [i32; 16] = [
                -100_000,
                -32_769,
                -32_768,
                -1,
                0,
                1,
                127,
                128,
                255,
                256,
                32_767,
                32_768,
                65_535,
                65_536,
                i32::MAX,
                i32::MIN,
            ];
            for lane in 0..16 {
                let value = values[lane % values.len()];
                bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        8 => {
            let values: [i64; 8] = [
                -100_000_000_000,
                -32_769,
                -1,
                0,
                255,
                65_536,
                i32::MAX as i64 + 1,
                i64::MAX,
            ];
            for lane in 0..8 {
                let value = values[lane % values.len()];
                bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => unreachable!("integer narrow tests cover word/dword/qword sources"),
    }
    bytes
}

fn test_elem_signed(src: &[u8], elem_size: usize) -> i128 {
    match elem_size {
        2 => i16::from_le_bytes([src[0], src[1]]) as i128,
        4 => i32::from_le_bytes([src[0], src[1], src[2], src[3]]) as i128,
        8 => i64::from_le_bytes([
            src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
        ]) as i128,
        _ => unreachable!("integer narrow tests cover word/dword/qword sources"),
    }
}

fn test_elem_unsigned(src: &[u8], elem_size: usize) -> u128 {
    match elem_size {
        2 => u16::from_le_bytes([src[0], src[1]]) as u128,
        4 => u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as u128,
        8 => u64::from_le_bytes([
            src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
        ]) as u128,
        _ => unreachable!("integer narrow tests cover word/dword/qword sources"),
    }
}

fn test_narrow_int_value(
    src: &[u8],
    src_elem_size: usize,
    dst_elem_size: usize,
    mode: TestNarrowMode,
) -> u64 {
    match mode {
        TestNarrowMode::Truncate => test_elem_unsigned(src, src_elem_size) as u64,
        TestNarrowMode::SignedSaturate => {
            let value = test_elem_signed(src, src_elem_size);
            let dst_bits = (dst_elem_size * 8) as u32;
            let min = -(1i128 << (dst_bits - 1));
            let max = (1i128 << (dst_bits - 1)) - 1;
            value.clamp(min, max) as i64 as u64
        }
        TestNarrowMode::UnsignedSaturate => {
            let value = test_elem_signed(src, src_elem_size);
            let dst_bits = (dst_elem_size * 8) as u32;
            let max = (1i128 << dst_bits) - 1;
            value.clamp(0, max) as u64
        }
    }
}

fn test_reg_vl_for_bytes(bytes: usize) -> usize {
    if bytes <= 16 {
        16
    } else if bytes <= 32 {
        32
    } else {
        64
    }
}

fn assert_evex_int_narrow_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    src_elem_size: usize,
    dst_elem_size: usize,
    src_vl_bytes: usize,
    mode: TestNarrowMode,
    dst_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let mut case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5512);
    let src_bytes = narrow_source_bytes(src_elem_size);
    set_zmm_bytes(&mut case.input, 16, src_bytes);
    let out = run_rax(&case);

    let num_elems = src_vl_bytes / src_elem_size;
    let mut raw = [0u8; 64];
    for lane in 0..num_elems {
        let src_base = lane * src_elem_size;
        let dst_base = lane * dst_elem_size;
        let value = test_narrow_int_value(
            &src_bytes[src_base..src_base + src_elem_size],
            src_elem_size,
            dst_elem_size,
            mode,
        );
        raw[dst_base..dst_base + dst_elem_size]
            .copy_from_slice(&value.to_le_bytes()[..dst_elem_size]);
    }

    if dst_memory {
        let mut expected = case.input.scratch;
        for lane in 0..num_elems {
            let active = mask
                .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
                .unwrap_or(true);
            if active {
                let base = lane * dst_elem_size;
                expected[base..base + dst_elem_size]
                    .copy_from_slice(&raw[base..base + dst_elem_size]);
            }
        }
        assert_eq!(out.scratch, expected, "{label}");
    } else {
        let dst = 17;
        let dst_bytes_len = num_elems * dst_elem_size;
        let dst_reg_vl_bytes = test_reg_vl_for_bytes(dst_bytes_len);
        let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
        let mut expected = [0u8; 64];
        for lane in 0..num_elems {
            let base = lane * dst_elem_size;
            let active = mask
                .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
                .unwrap_or(true);
            if active {
                expected[base..base + dst_elem_size]
                    .copy_from_slice(&raw[base..base + dst_elem_size]);
            } else if mask.is_some_and(|(_, zeroing)| zeroing) {
                // Zeroing: leave as 0.
            } else {
                expected[base..base + dst_elem_size]
                    .copy_from_slice(&dst_bytes[base..base + dst_elem_size]);
            }
        }
        expected[dst_reg_vl_bytes..].fill(0);
        assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
    }
}

fn assert_evex_extract_chunk_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    chunk_bytes: usize,
    src_vl_bytes: usize,
    imm: usize,
    dst_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5513);
    let out = run_rax(&case);
    let src = 16;
    let dst = 17;
    let num_chunks = src_vl_bytes / chunk_bytes;
    let chunk = imm & (num_chunks - 1);
    let src_base = chunk * chunk_bytes;
    let src_bytes = zmm_to_bytes(case.input.zmm[src]);
    let mut raw = [0u8; 64];
    raw[..chunk_bytes].copy_from_slice(&src_bytes[src_base..src_base + chunk_bytes]);

    if dst_memory {
        let mut expected = case.input.scratch;
        for lane in 0..(chunk_bytes / elem_size) {
            let active = mask
                .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
                .unwrap_or(true);
            if active {
                let base = lane * elem_size;
                expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
            }
        }
        assert_eq!(out.scratch, expected, "{label}");
    } else {
        let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
        let mut expected = [0u8; 64];
        for lane in 0..(chunk_bytes / elem_size) {
            let base = lane * elem_size;
            let active = mask
                .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
                .unwrap_or(true);
            if active {
                expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
            } else if mask.is_some_and(|(_, zeroing)| zeroing) {
                // Zeroing: leave as 0.
            } else {
                expected[base..base + elem_size]
                    .copy_from_slice(&dst_bytes[base..base + elem_size]);
            }
        }
        assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
    }
}

fn assert_evex_insert_chunk_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    chunk_bytes: usize,
    vl_bytes: usize,
    imm: usize,
    src_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5514);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let num_chunks = vl_bytes / chunk_bytes;
    let chunk = imm & (num_chunks - 1);
    let dst_base = chunk * chunk_bytes;
    let mut raw = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = if src_memory {
        let mut bytes = [0u8; 64];
        bytes[..chunk_bytes].copy_from_slice(&case.input.scratch[..chunk_bytes]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };
    raw[dst_base..dst_base + chunk_bytes].copy_from_slice(&src2_bytes[..chunk_bytes]);

    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_nt_store_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    profile: InputProfile,
    vl_bytes: usize,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, profile, 0x5515);
    let out = run_rax(&case);
    let src_bytes = zmm_to_bytes(case.input.zmm[16]);
    let mut expected = case.input.scratch;
    expected[..vl_bytes].copy_from_slice(&src_bytes[..vl_bytes]);
    assert_eq!(out.scratch, expected, "{label}");
}

fn assert_evex_nt_load_case(llvm_mc: &Path, label: &str, asm: &str, vl_bytes: usize) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5516);
    let out = run_rax(&case);
    let mut expected = [0u8; 64];
    expected[..vl_bytes].copy_from_slice(&case.input.scratch[..vl_bytes]);
    assert_eq!(out.zmm[17], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_block_broadcast_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    profile: InputProfile,
    elem_size: usize,
    block_bytes: usize,
    vl_bytes: usize,
    src_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, profile, 0x550d);
    let out = run_rax(&case);
    let dst = 17;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let mut block = [0u8; 32];
    if src_memory {
        block[..block_bytes].copy_from_slice(&case.input.scratch[..block_bytes]);
    } else {
        let src_bytes = zmm_to_bytes(case.input.zmm[16]);
        block[..block_bytes].copy_from_slice(&src_bytes[..block_bytes]);
    }

    let mut raw = [0u8; 64];
    for base in (0..vl_bytes).step_by(block_bytes) {
        raw[base..base + block_bytes].copy_from_slice(&block[..block_bytes]);
    }

    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_unpack_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    profile: InputProfile,
    elem_size: usize,
    vl_bytes: usize,
    high_half: bool,
    src2_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, profile, 0x550e);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = if src2_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else if asm.contains("(%rax)") {
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&case.input.scratch[..64]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };

    let elems_per_128 = 16 / elem_size;
    let half = elems_per_128 / 2;
    let first = if high_half { half } else { 0 };
    let mut raw = [0u8; 64];
    for block_base in (0..vl_bytes).step_by(16) {
        for lane in 0..half {
            let src_elem = first + lane;
            let dst0 = block_base + (lane * 2) * elem_size;
            let dst1 = dst0 + elem_size;
            let src_base = block_base + src_elem * elem_size;
            raw[dst0..dst0 + elem_size]
                .copy_from_slice(&src1_bytes[src_base..src_base + elem_size]);
            raw[dst1..dst1 + elem_size]
                .copy_from_slice(&src2_bytes[src_base..src_base + elem_size]);
        }
    }

    let mut expected = [0u8; 64];
    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_expand_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    vl_bytes: usize,
    src_memory: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550a);
    let out = run_rax(&case);
    let dst = 17;
    let src = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src_bytes = if src_memory {
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&case.input.scratch[..64]);
        bytes
    } else {
        zmm_to_bytes(case.input.zmm[src])
    };
    let mut expected = [0u8; 64];
    let mut src_index = 0usize;

    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            let src_base = src_index * elem_size;
            expected[base..base + elem_size]
                .copy_from_slice(&src_bytes[src_base..src_base + elem_size]);
            src_index += 1;
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_compress_reg_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    vl_bytes: usize,
    mask: usize,
    zeroing: bool,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550b);
    let out = run_rax(&case);
    let src = 17;
    let dst = 16;
    let src_bytes = zmm_to_bytes(case.input.zmm[src]);
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let mut expected = [0u8; 64];
    let mut out_index = 0usize;

    for lane in 0..(vl_bytes / elem_size) {
        if (case.input.k[mask] >> lane) & 1 != 0 {
            let src_base = lane * elem_size;
            let dst_base = out_index * elem_size;
            expected[dst_base..dst_base + elem_size]
                .copy_from_slice(&src_bytes[src_base..src_base + elem_size]);
            out_index += 1;
        }
    }
    let compressed_len = out_index * elem_size;
    if !zeroing {
        expected[compressed_len..vl_bytes].copy_from_slice(&dst_bytes[compressed_len..vl_bytes]);
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn assert_evex_compress_mem_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    vl_bytes: usize,
    mask: usize,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x550c);
    let out = run_rax(&case);
    let src_bytes = zmm_to_bytes(case.input.zmm[17]);
    let mut expected = case.input.scratch;
    let mut out_index = 0usize;

    for lane in 0..(vl_bytes / elem_size) {
        if (case.input.k[mask] >> lane) & 1 != 0 {
            let src_base = lane * elem_size;
            let dst_base = out_index * elem_size;
            expected[dst_base..dst_base + elem_size]
                .copy_from_slice(&src_bytes[src_base..src_base + elem_size]);
            out_index += 1;
        }
    }

    assert_eq!(out.scratch, expected, "{label}");
}

fn assert_evex_valign_case(
    llvm_mc: &Path,
    label: &str,
    asm: &str,
    elem_size: usize,
    imm: u8,
    src2_from_broadcast: bool,
    mask: Option<(usize, bool)>,
) {
    let case = assembled_rax_case(llvm_mc, label, asm, InputProfile::Int, 0x5502);
    let out = run_rax(&case);
    let dst = 17;
    let src1 = 18;
    let src2 = 16;
    let dst_bytes = zmm_to_bytes(case.input.zmm[dst]);
    let src1_bytes = zmm_to_bytes(case.input.zmm[src1]);
    let src2_bytes = if src2_from_broadcast {
        broadcast_bytes(&case.input, elem_size)
    } else {
        zmm_to_bytes(case.input.zmm[src2])
    };
    let num_elems = 64 / elem_size;
    let shift = match elem_size {
        4 => (imm & 0x0f) as usize,
        8 => (imm & 0x07) as usize,
        _ => unreachable!("VALIGN tests cover dword/qword elements"),
    };

    let mut raw = [0u8; 64];
    for lane in 0..num_elems {
        let src_index = lane + shift;
        let dst_base = lane * elem_size;
        if src_index < num_elems {
            let src_base = src_index * elem_size;
            raw[dst_base..dst_base + elem_size]
                .copy_from_slice(&src2_bytes[src_base..src_base + elem_size]);
        } else if src_index < num_elems * 2 {
            let src_base = (src_index - num_elems) * elem_size;
            raw[dst_base..dst_base + elem_size]
                .copy_from_slice(&src1_bytes[src_base..src_base + elem_size]);
        }
    }

    let mut expected = [0u8; 64];
    for lane in 0..num_elems {
        let base = lane * elem_size;
        let active = mask
            .map(|(k, _)| ((case.input.k[k] >> lane) & 1) != 0)
            .unwrap_or(true);
        if active {
            expected[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if mask.is_some_and(|(_, zeroing)| zeroing) {
            // Zeroing: leave as 0.
        } else {
            expected[base..base + elem_size].copy_from_slice(&dst_bytes[base..base + elem_size]);
        }
    }

    assert_eq!(out.zmm[dst], zmm_from_bytes(expected), "{label}");
}

fn write_f32_lane(bytes: &mut [u8; 64], lane: usize, value: f32) {
    let base = lane * 4;
    bytes[base..base + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_f32_lane(bytes: &[u8; 64], lane: usize) -> f32 {
    let base = lane * 4;
    f32::from_le_bytes(bytes[base..base + 4].try_into().unwrap())
}

fn write_i32_lane(bytes: &mut [u8; 64], lane: usize, value: i32) {
    let base = lane * 4;
    bytes[base..base + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i16_lane(bytes: &mut [u8; 64], lane: usize, value: i16) {
    let base = lane * 2;
    bytes[base..base + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_f16_lane_bits(bytes: &mut [u8; 64], lane: usize, bits: u16) {
    let base = lane * 2;
    bytes[base..base + 2].copy_from_slice(&bits.to_le_bytes());
}

fn write_f16_pair_bits(bytes: &mut [u8; 64], pair: usize, real: u16, imag: u16) {
    write_f16_lane_bits(bytes, pair * 2, real);
    write_f16_lane_bits(bytes, pair * 2 + 1, imag);
}

fn assert_evex_fixupimm_contracts(llvm_mc: &Path) {
    let mut packed = assembled_rax_case(
        llvm_mc,
        "vfixupimmps_table_contract",
        "vfixupimmps $0, %zmm16, %zmm18, %zmm17",
        InputProfile::F32,
        0x5600,
    );
    let mut src1 = [0u8; 64];
    let mut src2 = [0u8; 64];
    let mut expected = [0u8; 64];
    for lane in 0..16 {
        write_f32_lane(&mut src1, lane, 0.0);
        write_elem(&mut src2, lane, 4, 0x0a00);
        write_f32_lane(&mut expected, lane, 1.0);
    }
    packed.input.zmm[18] = zmm_from_bytes(src1);
    packed.input.zmm[16] = zmm_from_bytes(src2);
    let out = run_rax(&packed);
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vfixupimmps_table_contract"
    );

    let mut scalar = assembled_rax_case(
        llvm_mc,
        "vfixupimmss_scalar_high_source_contract",
        "vfixupimmss $0, %xmm16, %xmm18, %xmm17 {%k1} {z}",
        InputProfile::F32,
        0x5601,
    );
    let mut src1 = zmm_to_bytes(scalar.input.zmm[18]);
    let mut src2 = [0u8; 64];
    write_f32_lane(&mut src1, 0, f32::INFINITY);
    write_f32_lane(&mut src1, 1, 7.0);
    write_f32_lane(&mut src1, 2, 8.0);
    write_f32_lane(&mut src1, 3, 9.0);
    write_elem(&mut src2, 0, 4, 0x4 << 20);
    scalar.input.zmm[18] = zmm_from_bytes(src1);
    scalar.input.zmm[16] = zmm_from_bytes(src2);

    let out = run_rax(&scalar);
    let mut expected = [0u8; 64];
    write_f32_lane(&mut expected, 0, f32::NEG_INFINITY);
    expected[4..16].copy_from_slice(&src1[4..16]);
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vfixupimmss_scalar_high_source_contract"
    );
}

fn assert_evex_fp16_complex_contracts(llvm_mc: &Path) {
    const H_ZERO: u16 = 0x0000;
    const H_ONE: u16 = 0x3c00;
    const H_TWO: u16 = 0x4000;
    const H_THREE: u16 = 0x4200;
    const H_NEG_THREE: u16 = 0xc200;
    const H_SEVEN: u16 = 0x4700;

    let mut packed = assembled_rax_case(
        llvm_mc,
        "vfmulcph_packed_contract",
        "vfmulcph %zmm16, %zmm18, %zmm17",
        InputProfile::F16,
        0x5610,
    );
    let mut src1 = [0u8; 64];
    let mut src2 = [0u8; 64];
    let mut expected = [0u8; 64];
    for pair in 0..16 {
        write_f16_pair_bits(&mut src1, pair, H_ONE, H_ZERO);
        write_f16_pair_bits(&mut src2, pair, H_TWO, H_THREE);
        write_f16_pair_bits(&mut expected, pair, H_TWO, H_THREE);
    }
    packed.input.zmm[18] = zmm_from_bytes(src1);
    packed.input.zmm[16] = zmm_from_bytes(src2);
    let out = run_rax(&packed);
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vfmulcph_packed_contract"
    );

    let mut conjugate = assembled_rax_case(
        llvm_mc,
        "vfcmulcph_conjugate_contract",
        "vfcmulcph %zmm16, %zmm18, %zmm17",
        InputProfile::F16,
        0x5611,
    );
    conjugate.input.zmm[18] = zmm_from_bytes(src1);
    conjugate.input.zmm[16] = zmm_from_bytes(src2);
    let mut expected = [0u8; 64];
    for pair in 0..16 {
        write_f16_pair_bits(&mut expected, pair, H_TWO, H_NEG_THREE);
    }
    let out = run_rax(&conjugate);
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vfcmulcph_conjugate_contract"
    );

    let mut scalar = assembled_rax_case(
        llvm_mc,
        "vfmaddcsh_scalar_contract",
        "vfmaddcsh %xmm16, %xmm18, %xmm17",
        InputProfile::F16,
        0x5612,
    );
    let mut dst = [0u8; 64];
    let mut src1 = zmm_to_bytes(scalar.input.zmm[18]);
    let mut src2 = [0u8; 64];
    write_f16_pair_bits(&mut dst, 0, 0x4500, H_ZERO);
    write_f16_pair_bits(&mut src1, 0, H_ONE, H_ZERO);
    write_f16_pair_bits(&mut src2, 0, H_TWO, H_ZERO);
    scalar.input.zmm[17] = zmm_from_bytes(dst);
    scalar.input.zmm[18] = zmm_from_bytes(src1);
    scalar.input.zmm[16] = zmm_from_bytes(src2);

    let out = run_rax(&scalar);
    let mut expected = [0u8; 64];
    write_f16_pair_bits(&mut expected, 0, H_SEVEN, H_ZERO);
    expected[4..16].copy_from_slice(&src1[4..16]);
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vfmaddcsh_scalar_contract"
    );
}

fn seed_4fma_case(case: &mut DiffCase) {
    let mut dst = [0u8; 64];
    for lane in 0..16 {
        write_f32_lane(&mut dst, lane, 10.0 + lane as f32);
    }
    case.input.zmm[17] = zmm_from_bytes(dst);

    for block in 0..4 {
        let mut src = [0u8; 64];
        for lane in 0..16 {
            write_f32_lane(&mut src, lane, (block + 1) as f32);
        }
        case.input.zmm[20 + block] = zmm_from_bytes(src);
        let base = block * 4;
        case.input.scratch[base..base + 4].copy_from_slice(&((block + 2) as f32).to_le_bytes());
    }
}

fn assert_evex_source_block_fma_contracts(llvm_mc: &Path) {
    let mut packed = assembled_rax_case(
        llvm_mc,
        "v4fmaddps_source_block_contract",
        "v4fmaddps (%rax), %zmm20, %zmm17",
        InputProfile::F32,
        0x5620,
    );
    seed_4fma_case(&mut packed);
    let out = run_rax(&packed);
    let mut expected = [0u8; 64];
    for lane in 0..16 {
        write_f32_lane(&mut expected, lane, 50.0 + lane as f32);
    }
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "v4fmaddps_source_block_contract"
    );

    let mut negative = assembled_rax_case(
        llvm_mc,
        "v4fnmaddps_source_block_contract",
        "v4fnmaddps (%rax), %zmm20, %zmm17",
        InputProfile::F32,
        0x5621,
    );
    seed_4fma_case(&mut negative);
    let out = run_rax(&negative);
    let mut expected = [0u8; 64];
    for lane in 0..16 {
        write_f32_lane(&mut expected, lane, -30.0 + lane as f32);
    }
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "v4fnmaddps_source_block_contract"
    );

    let mut scalar = assembled_rax_case(
        llvm_mc,
        "v4fmaddss_zero_mask_preserves_high_lanes",
        "v4fmaddss (%rax), %xmm20, %xmm17 {%k1} {z}",
        InputProfile::F32,
        0x5622,
    );
    seed_4fma_case(&mut scalar);
    let out = run_rax(&scalar);
    let dst = zmm_to_bytes(scalar.input.zmm[17]);
    let mut expected = [0u8; 64];
    write_f32_lane(&mut expected, 0, 50.0);
    expected[4..16].copy_from_slice(&dst[4..16]);
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "v4fmaddss_zero_mask_preserves_high_lanes"
    );

    let out_bytes = zmm_to_bytes(out.zmm[17]);
    assert_eq!(
        read_f32_lane(&out_bytes, 1),
        read_f32_lane(&dst, 1),
        "v4fmaddss lane 1 must not be zeroed by k1 zero-mask"
    );
}

fn seed_4vnniw_case(case: &mut DiffCase, saturating: bool) {
    let dst_value = if saturating { i32::MAX } else { 10 };
    let source_pairs: [(i16, i16); 4] = if saturating {
        [(1, 0), (0, 0), (0, 0), (0, 0)]
    } else {
        [(1, 2), (5, 6), (1, -1), (2, 3)]
    };
    let mem_pairs: [(i16, i16); 4] = if saturating {
        [(1, 0), (0, 0), (0, 0), (0, 0)]
    } else {
        [(3, 4), (7, 8), (10, 11), (4, 5)]
    };

    let mut dst = [0u8; 64];
    for lane in 0..16 {
        write_i32_lane(&mut dst, lane, dst_value);
    }
    case.input.zmm[17] = zmm_from_bytes(dst);

    for (block, (lo, hi)) in source_pairs.into_iter().enumerate() {
        let mut src = [0u8; 64];
        for lane in 0..16 {
            write_i16_lane(&mut src, lane * 2, lo);
            write_i16_lane(&mut src, lane * 2 + 1, hi);
        }
        case.input.zmm[20 + block] = zmm_from_bytes(src);
    }

    case.input.scratch[..16].fill(0);
    for (block, (lo, hi)) in mem_pairs.into_iter().enumerate() {
        let base = block * 4;
        case.input.scratch[base..base + 2].copy_from_slice(&lo.to_le_bytes());
        case.input.scratch[base + 2..base + 4].copy_from_slice(&hi.to_le_bytes());
    }
}

fn assert_evex_4vnniw_contracts(llvm_mc: &Path) {
    let mut dot = assembled_rax_case(
        llvm_mc,
        "vp4dpwssd_source_block_contract",
        "vp4dpwssd (%rax), %zmm20, %zmm17",
        InputProfile::Int,
        0x5630,
    );
    seed_4vnniw_case(&mut dot, false);
    let out = run_rax(&dot);
    let mut expected = [0u8; 64];
    for lane in 0..16 {
        write_i32_lane(&mut expected, lane, 126);
    }
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vp4dpwssd_source_block_contract"
    );

    let mut saturating = assembled_rax_case(
        llvm_mc,
        "vp4dpwssds_saturating_contract",
        "vp4dpwssds (%rax), %zmm20, %zmm17",
        InputProfile::Int,
        0x5631,
    );
    seed_4vnniw_case(&mut saturating, true);
    let out = run_rax(&saturating);
    let mut expected = [0u8; 64];
    for lane in 0..16 {
        write_i32_lane(&mut expected, lane, i32::MAX);
    }
    assert_eq!(
        out.zmm[17],
        zmm_from_bytes(expected),
        "vp4dpwssds_saturating_contract"
    );
}

#[test]
fn evex_generated_corpus_covers_supported_selectors_and_forms() {
    let specs = generated_specs();
    assert_requested_mnemonic_coverage(&specs);

    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX assembly coverage check");
        return;
    };
    let cases = assembled_cases(&llvm_mc);
    assert!(!cases.is_empty(), "EVEX differential corpus is empty");
}

#[test]
fn evex_late_avx512_instruction_families_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping late EVEX AVX-512 semantic checks");
        return;
    };

    assert_evex_fixupimm_contracts(&llvm_mc);
    assert_evex_fp16_complex_contracts(&llvm_mc);
    assert_evex_source_block_fma_contracts(&llvm_mc);
    assert_evex_4vnniw_contracts(&llvm_mc);
}

#[test]
fn evex_scalar_fp_arith_matches_scalar_lane_contract() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX scalar semantic checks");
        return;
    };

    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vaddss_active_high_regs",
        "vaddss %xmm16, %xmm18, %xmm17",
        |a, b| a + b,
        None,
    );
    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vsubss_merge_mask",
        "vsubss %xmm16, %xmm18, %xmm17 {%k2}",
        |a, b| a - b,
        Some((2, false)),
    );
    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vmulss_zero_mask",
        "vmulss %xmm16, %xmm18, %xmm17 {%k2} {z}",
        |a, b| a * b,
        Some((2, true)),
    );
    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vdivss_active_high_regs",
        "vdivss %xmm16, %xmm18, %xmm17",
        |a, b| a / b,
        None,
    );
    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vminss_active_high_regs",
        "vminss %xmm16, %xmm18, %xmm17",
        x86_min_f32,
        None,
    );
    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vmaxss_active_high_regs",
        "vmaxss %xmm16, %xmm18, %xmm17",
        x86_max_f32,
        None,
    );
    assert_evex_scalar_f32_case(
        &llvm_mc,
        "vsqrtss_active_high_regs",
        "vsqrtss %xmm16, %xmm18, %xmm17",
        |_, b| b.sqrt(),
        None,
    );

    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vaddsd_active_high_regs",
        "vaddsd %xmm16, %xmm18, %xmm17",
        |a, b| a + b,
        None,
    );
    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vsubsd_merge_mask",
        "vsubsd %xmm16, %xmm18, %xmm17 {%k2}",
        |a, b| a - b,
        Some((2, false)),
    );
    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vmulsd_zero_mask",
        "vmulsd %xmm16, %xmm18, %xmm17 {%k2} {z}",
        |a, b| a * b,
        Some((2, true)),
    );
    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vdivsd_active_high_regs",
        "vdivsd %xmm16, %xmm18, %xmm17",
        |a, b| a / b,
        None,
    );
    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vminsd_active_high_regs",
        "vminsd %xmm16, %xmm18, %xmm17",
        x86_min_f64,
        None,
    );
    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vmaxsd_active_high_regs",
        "vmaxsd %xmm16, %xmm18, %xmm17",
        x86_max_f64,
        None,
    );
    assert_evex_scalar_f64_case(
        &llvm_mc,
        "vsqrtsd_active_high_regs",
        "vsqrtsd %xmm16, %xmm18, %xmm17",
        |_, b| b.sqrt(),
        None,
    );

    assert_evex_scalar_f32_bits(
        &llvm_mc,
        "vminss_signed_zero_uses_src2",
        "vminss %xmm16, %xmm18, %xmm17",
        0x0000_0000,
        0x8000_0000,
        0x8000_0000,
    );
    assert_evex_scalar_f32_bits(
        &llvm_mc,
        "vmaxss_src1_nan_uses_src2",
        "vmaxss %xmm16, %xmm18, %xmm17",
        0x7fc0_1234,
        0x3f80_0000,
        0x3f80_0000,
    );
    assert_evex_scalar_f64_bits(
        &llvm_mc,
        "vminsd_src2_nan_uses_src2",
        "vminsd %xmm16, %xmm18, %xmm17",
        0x3ff0_0000_0000_0000,
        0x7ff8_0000_0000_1234,
        0x7ff8_0000_0000_1234,
    );
}

#[test]
fn evex_fp_logical_matches_mask_and_broadcast_contract() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX FP logical semantic checks");
        return;
    };

    assert_evex_fp_bitwise_case(
        &llvm_mc,
        "vandps_high_regs",
        "vandps %zmm16, %zmm18, %zmm17",
        InputProfile::F32,
        4,
        |a, b| a & b,
        false,
        None,
    );
    assert_evex_fp_bitwise_case(
        &llvm_mc,
        "vandnps_mem_broadcast",
        "vandnps (%rax){1to16}, %zmm18, %zmm17",
        InputProfile::F32,
        4,
        |a, b| (!a) & b,
        true,
        None,
    );
    assert_evex_fp_bitwise_case(
        &llvm_mc,
        "vorpd_merge_mask",
        "vorpd %zmm16, %zmm18, %zmm17 {%k2}",
        InputProfile::F64,
        8,
        |a, b| a | b,
        false,
        Some((2, false)),
    );
    assert_evex_fp_bitwise_case(
        &llvm_mc,
        "vxorpd_zero_mask",
        "vxorpd %zmm16, %zmm18, %zmm17 {%k2} {z}",
        InputProfile::F64,
        8,
        |a, b| a ^ b,
        false,
        Some((2, true)),
    );
}

#[test]
fn evex_ternlog_and_testmask_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX ternlog/testmask semantic checks");
        return;
    };

    assert_evex_ternlog_case(
        &llvm_mc,
        "vpternlogd_high_regs",
        "vpternlogd $0xe2, %zmm16, %zmm18, %zmm17",
        4,
        0xe2,
        None,
    );
    assert_evex_ternlog_case(
        &llvm_mc,
        "vpternlogq_high_regs",
        "vpternlogq $0xe4, %zmm16, %zmm18, %zmm17",
        8,
        0xe4,
        None,
    );
    assert_evex_ternlog_case(
        &llvm_mc,
        "vpternlogd_zero_mask",
        "vpternlogd $0x96, %zmm16, %zmm18, %zmm17 {%k2} {z}",
        4,
        0x96,
        Some((2, true)),
    );

    assert_evex_test_mask_case(
        &llvm_mc,
        "vptestmb_mask_high_regs",
        "vptestmb %zmm16, %zmm18, %k3 {%k1}",
        1,
        false,
        false,
        Some(1),
    );
    assert_evex_test_mask_case(
        &llvm_mc,
        "vptestnmd_mem_broadcast",
        "vptestnmd (%rax){1to16}, %zmm18, %k3",
        4,
        true,
        true,
        None,
    );
    assert_evex_test_mask_case(
        &llvm_mc,
        "vptestmq_mem_broadcast",
        "vptestmq (%rax){1to8}, %zmm18, %k3",
        8,
        false,
        true,
        None,
    );
}

#[test]
fn evex_valign_and_rotate_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX VALIGN/rotate semantic checks");
        return;
    };

    assert_evex_valign_case(
        &llvm_mc,
        "valignd_high_regs",
        "valignd $3, %zmm16, %zmm18, %zmm17",
        4,
        3,
        false,
        None,
    );
    assert_evex_valign_case(
        &llvm_mc,
        "valignq_zero_mask",
        "valignq $2, %zmm16, %zmm18, %zmm17 {%k2} {z}",
        8,
        2,
        false,
        Some((2, true)),
    );
    assert_evex_valign_case(
        &llvm_mc,
        "valignd_mem_broadcast",
        "valignd $3, (%rax){1to16}, %zmm18, %zmm17",
        4,
        3,
        true,
        None,
    );

    assert_evex_rotate_imm_case(
        &llvm_mc,
        "vprold_high_regs",
        "vprold $5, %zmm16, %zmm17",
        4,
        true,
        5,
        false,
        None,
    );
    assert_evex_rotate_imm_case(
        &llvm_mc,
        "vprorq_zero_mask",
        "vprorq $7, %zmm16, %zmm17 {%k2} {z}",
        8,
        false,
        7,
        false,
        Some((2, true)),
    );
    assert_evex_rotate_imm_case(
        &llvm_mc,
        "vprord_mem_broadcast",
        "vprord $5, (%rax){1to16}, %zmm17",
        4,
        false,
        5,
        true,
        None,
    );

    assert_evex_rotate_per_elem_case(
        &llvm_mc,
        "vprolvd_high_regs",
        "vprolvd %zmm16, %zmm18, %zmm17",
        4,
        true,
        false,
        None,
    );
    assert_evex_rotate_per_elem_case(
        &llvm_mc,
        "vprorvq_mem_broadcast",
        "vprorvq (%rax){1to8}, %zmm18, %zmm17",
        8,
        false,
        true,
        None,
    );
    assert_evex_rotate_per_elem_case(
        &llvm_mc,
        "vprolvq_merge_mask",
        "vprolvq %zmm16, %zmm18, %zmm17 {%k1}",
        8,
        true,
        false,
        Some((1, false)),
    );
}

#[test]
fn evex_count_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX count semantic checks");
        return;
    };

    assert_evex_count_case(
        &llvm_mc,
        "vplzcntd_high_regs",
        "vplzcntd %zmm16, %zmm17",
        4,
        true,
        false,
        None,
    );
    assert_evex_count_case(
        &llvm_mc,
        "vplzcntq_zero_mask",
        "vplzcntq %zmm16, %zmm17 {%k2} {z}",
        8,
        true,
        false,
        Some((2, true)),
    );
    assert_evex_count_case(
        &llvm_mc,
        "vplzcntd_mem_broadcast",
        "vplzcntd (%rax){1to16}, %zmm17",
        4,
        true,
        true,
        None,
    );
    assert_evex_count_case(
        &llvm_mc,
        "vpopcntq_mem_broadcast",
        "vpopcntq (%rax){1to8}, %zmm17",
        8,
        false,
        true,
        None,
    );
    assert_evex_count_case(
        &llvm_mc,
        "vpopcntd_merge_mask",
        "vpopcntd %zmm16, %zmm17 {%k1}",
        4,
        false,
        false,
        Some((1, false)),
    );
}

#[test]
fn evex_blend_mask_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX blend-mask semantic checks");
        return;
    };

    assert_evex_blend_case(
        &llvm_mc,
        "vblendmps_mask_selects_src2",
        "vblendmps %zmm16, %zmm18, %zmm17 {%k2}",
        InputProfile::F32,
        4,
        false,
        false,
        Some((2, false)),
    );
    assert_evex_blend_case(
        &llvm_mc,
        "vblendmpd_zero_broadcast",
        "vblendmpd (%rax){1to8}, %zmm18, %zmm17 {%k2} {z}",
        InputProfile::F64,
        8,
        true,
        true,
        Some((2, true)),
    );
    assert_evex_blend_case(
        &llvm_mc,
        "vpblendmb_mask_selects_src2",
        "vpblendmb %zmm16, %zmm18, %zmm17 {%k2}",
        InputProfile::Int,
        1,
        false,
        false,
        Some((2, false)),
    );
    assert_evex_blend_case(
        &llvm_mc,
        "vpblendmw_full_memory",
        "vpblendmw (%rax), %zmm18, %zmm17 {%k1}",
        InputProfile::Int,
        2,
        true,
        false,
        Some((1, false)),
    );
    assert_evex_blend_case(
        &llvm_mc,
        "vpblendmd_mem_broadcast",
        "vpblendmd (%rax){1to16}, %zmm18, %zmm17 {%k2}",
        InputProfile::Int,
        4,
        true,
        true,
        Some((2, false)),
    );
    assert_evex_blend_case(
        &llvm_mc,
        "vpblendmq_zero_mask",
        "vpblendmq %zmm16, %zmm18, %zmm17 {%k2} {z}",
        InputProfile::Int,
        8,
        false,
        false,
        Some((2, true)),
    );
}

#[test]
fn evex_saturating_pack_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX pack semantic checks");
        return;
    };

    assert_evex_pack_case(
        &llvm_mc,
        "vpacksswb_high_regs",
        "vpacksswb %zmm16, %zmm18, %zmm17",
        TestPackKind::SignedWordToSignedByte,
        64,
        false,
        false,
        None,
    );
    assert_evex_pack_case(
        &llvm_mc,
        "vpackuswb_xmm_memory_merge",
        "vpackuswb (%rax), %xmm18, %xmm17 {%k1}",
        TestPackKind::UnsignedWordToUnsignedByte,
        16,
        true,
        false,
        Some((1, false)),
    );
    assert_evex_pack_case(
        &llvm_mc,
        "vpackuswb_zero_mask",
        "vpackuswb %zmm16, %zmm18, %zmm17 {%k2} {z}",
        TestPackKind::UnsignedWordToUnsignedByte,
        64,
        false,
        false,
        Some((2, true)),
    );
    assert_evex_pack_case(
        &llvm_mc,
        "vpackssdw_ymm_vl",
        "vpackssdw %ymm16, %ymm18, %ymm17",
        TestPackKind::SignedDwordToSignedWord,
        32,
        false,
        false,
        None,
    );
    assert_evex_pack_case(
        &llvm_mc,
        "vpackssdw_mem_broadcast_merge",
        "vpackssdw (%rax){1to16}, %zmm18, %zmm17 {%k1}",
        TestPackKind::SignedDwordToSignedWord,
        64,
        true,
        true,
        Some((1, false)),
    );
    assert_evex_pack_case(
        &llvm_mc,
        "vpackusdw_mem_broadcast_zero",
        "vpackusdw (%rax){1to16}, %zmm18, %zmm17 {%k2} {z}",
        TestPackKind::UnsignedDwordToUnsignedWord,
        64,
        true,
        true,
        Some((2, true)),
    );
}

#[test]
fn evex_shuffle_align_sad_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX shuffle/align/SAD checks");
        return;
    };

    assert_evex_pshufb_case(
        &llvm_mc,
        "vpshufb_zero_mask",
        "vpshufb %zmm16, %zmm18, %zmm17 {%k2} {z}",
        64,
        false,
        Some((2, true)),
    );
    assert_evex_pshufb_case(
        &llvm_mc,
        "vpshufb_memory_merge",
        "vpshufb (%rax), %zmm18, %zmm17 {%k1}",
        64,
        true,
        Some((1, false)),
    );
    assert_evex_palignr_case(
        &llvm_mc,
        "vpalignr_high_regs",
        "vpalignr $5, %zmm16, %zmm18, %zmm17",
        64,
        5,
        false,
        None,
    );
    assert_evex_palignr_case(
        &llvm_mc,
        "vpalignr_zero_mask",
        "vpalignr $19, %zmm16, %zmm18, %zmm17 {%k2} {z}",
        64,
        19,
        false,
        Some((2, true)),
    );
    assert_evex_shuffle_imm_case(
        &llvm_mc,
        "vpshufd_mem_broadcast",
        "vpshufd $0x1b, (%rax){1to16}, %zmm17 {%k1}",
        TestShuffleImmKind::Dword,
        64,
        0x1b,
        true,
        true,
        Some((1, false)),
    );
    assert_evex_shuffle_imm_case(
        &llvm_mc,
        "vpshufhw_zero_mask",
        "vpshufhw $0x1b, %zmm16, %zmm17 {%k2} {z}",
        TestShuffleImmKind::HighWord,
        64,
        0x1b,
        false,
        false,
        Some((2, true)),
    );
    assert_evex_shuffle_imm_case(
        &llvm_mc,
        "vpshuflw_ymm_memory",
        "vpshuflw $0x4e, (%rax), %ymm17",
        TestShuffleImmKind::LowWord,
        32,
        0x4e,
        true,
        false,
        None,
    );
    assert_evex_psadbw_case(
        &llvm_mc,
        "vpsadbw_high_regs",
        "vpsadbw %zmm16, %zmm18, %zmm17",
        64,
        false,
    );
    assert_evex_psadbw_case(
        &llvm_mc,
        "vpsadbw_xmm_memory",
        "vpsadbw (%rax), %xmm18, %xmm17",
        16,
        true,
    );
}

#[test]
fn evex_mask_vector_transfer_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX mask/vector transfer semantic checks");
        return;
    };

    assert_evex_mask_to_vec_case(
        &llvm_mc,
        "vpmovm2b_high_dest",
        "vpmovm2b %k2, %zmm17",
        1,
        64,
        2,
        17,
    );
    assert_evex_mask_to_vec_case(
        &llvm_mc,
        "vpmovm2w_high_dest",
        "vpmovm2w %k2, %zmm17",
        2,
        64,
        2,
        17,
    );
    assert_evex_mask_to_vec_case(
        &llvm_mc,
        "vpmovm2d_xmm_vl",
        "vpmovm2d %k1, %xmm17",
        4,
        16,
        1,
        17,
    );
    assert_evex_mask_to_vec_case(
        &llvm_mc,
        "vpmovm2q_ymm_vl",
        "vpmovm2q %k1, %ymm17",
        8,
        32,
        1,
        17,
    );

    assert_evex_vec_to_mask_case(
        &llvm_mc,
        "vpmovb2m_high_src",
        "vpmovb2m %zmm16, %k3",
        1,
        64,
        16,
        3,
    );
    assert_evex_vec_to_mask_case(
        &llvm_mc,
        "vpmovw2m_high_src",
        "vpmovw2m %zmm16, %k3",
        2,
        64,
        16,
        3,
    );
    assert_evex_vec_to_mask_case(
        &llvm_mc,
        "vpmovd2m_xmm_vl",
        "vpmovd2m %xmm16, %k3",
        4,
        16,
        16,
        3,
    );
    assert_evex_vec_to_mask_case(
        &llvm_mc,
        "vpmovq2m_ymm_vl",
        "vpmovq2m %ymm16, %k3",
        8,
        32,
        16,
        3,
    );

    assert_evex_mask_broadcast_case(
        &llvm_mc,
        "vpbroadcastmb2q_high_dest",
        "vpbroadcastmb2q %k2, %zmm17",
        8,
        64,
        8,
        2,
        17,
    );
    assert_evex_mask_broadcast_case(
        &llvm_mc,
        "vpbroadcastmw2d_high_dest",
        "vpbroadcastmw2d %k2, %zmm17",
        4,
        64,
        16,
        2,
        17,
    );
    assert_evex_mask_broadcast_case(
        &llvm_mc,
        "vpbroadcastmb2q_xmm_vl",
        "vpbroadcastmb2q %k1, %xmm17",
        8,
        16,
        8,
        1,
        17,
    );
}

#[test]
fn evex_integer_extend_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX integer extend semantic checks");
        return;
    };

    assert_evex_int_extend_case(
        &llvm_mc,
        "vpmovsxbw_reg_merge",
        "vpmovsxbw %ymm16, %zmm17 {%k1}",
        1,
        2,
        64,
        true,
        false,
        Some((1, false)),
    );
    assert_evex_int_extend_case(
        &llvm_mc,
        "vpmovsxbd_mem_zero",
        "vpmovsxbd (%rax), %zmm17 {%k2} {z}",
        1,
        4,
        64,
        true,
        true,
        Some((2, true)),
    );
    assert_evex_int_extend_case(
        &llvm_mc,
        "vpmovsxdq_ymm_vl",
        "vpmovsxdq %xmm16, %ymm17",
        4,
        8,
        32,
        true,
        false,
        None,
    );
    assert_evex_int_extend_case(
        &llvm_mc,
        "vpmovzxbq_mem",
        "vpmovzxbq (%rax), %zmm17",
        1,
        8,
        64,
        false,
        true,
        None,
    );
    assert_evex_int_extend_case(
        &llvm_mc,
        "vpmovzxwd_ymm_merge",
        "vpmovzxwd %xmm16, %ymm17 {%k1}",
        2,
        4,
        32,
        false,
        false,
        Some((1, false)),
    );
    assert_evex_int_extend_case(
        &llvm_mc,
        "vpmovzxdq_xmm_zero",
        "vpmovzxdq %xmm16, %xmm17 {%k2} {z}",
        4,
        8,
        16,
        false,
        false,
        Some((2, true)),
    );
}

#[test]
fn evex_integer_narrow_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX integer narrow semantic checks");
        return;
    };

    assert_evex_int_narrow_case(
        &llvm_mc,
        "vpmovswb_reg_merge",
        "vpmovswb %zmm16, %ymm17 {%k1}",
        2,
        1,
        64,
        TestNarrowMode::SignedSaturate,
        false,
        Some((1, false)),
    );
    assert_evex_int_narrow_case(
        &llvm_mc,
        "vpmovusdb_reg_zero",
        "vpmovusdb %zmm16, %xmm17 {%k2} {z}",
        4,
        1,
        64,
        TestNarrowMode::UnsignedSaturate,
        false,
        Some((2, true)),
    );
    assert_evex_int_narrow_case(
        &llvm_mc,
        "vpmovqd_reg_truncate",
        "vpmovqd %zmm16, %ymm17",
        8,
        4,
        64,
        TestNarrowMode::Truncate,
        false,
        None,
    );
    assert_evex_int_narrow_case(
        &llvm_mc,
        "vpmovusqw_mem_mask",
        "vpmovusqw %zmm16, (%rax) {%k1}",
        8,
        2,
        64,
        TestNarrowMode::UnsignedSaturate,
        true,
        Some((1, false)),
    );
    assert_evex_int_narrow_case(
        &llvm_mc,
        "vpmovqb_ymm_vl",
        "vpmovqb %ymm16, %xmm17",
        8,
        1,
        32,
        TestNarrowMode::Truncate,
        false,
        None,
    );
    assert_evex_int_narrow_case(
        &llvm_mc,
        "vpmovsdw_mem",
        "vpmovsdw %zmm16, (%rax)",
        4,
        2,
        64,
        TestNarrowMode::SignedSaturate,
        true,
        None,
    );
}

#[test]
fn evex_extract_insert_chunk_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX extract/insert semantic checks");
        return;
    };

    assert_evex_extract_chunk_case(
        &llvm_mc,
        "vextractf32x4_reg_merge",
        "vextractf32x4 $1, %zmm16, %xmm17 {%k1}",
        4,
        16,
        64,
        1,
        false,
        Some((1, false)),
    );
    assert_evex_extract_chunk_case(
        &llvm_mc,
        "vextracti64x2_mem_mask",
        "vextracti64x2 $2, %zmm16, (%rax) {%k1}",
        8,
        16,
        64,
        2,
        true,
        Some((1, false)),
    );
    assert_evex_extract_chunk_case(
        &llvm_mc,
        "vextractf64x4_reg_zero",
        "vextractf64x4 $1, %zmm16, %ymm17 {%k2} {z}",
        8,
        32,
        64,
        1,
        false,
        Some((2, true)),
    );
    assert_evex_insert_chunk_case(
        &llvm_mc,
        "vinserti32x4_reg_merge",
        "vinserti32x4 $1, %xmm16, %zmm18, %zmm17 {%k1}",
        4,
        16,
        64,
        1,
        false,
        Some((1, false)),
    );
    assert_evex_insert_chunk_case(
        &llvm_mc,
        "vinsertf64x2_mem_zero",
        "vinsertf64x2 $1, (%rax), %zmm18, %zmm17 {%k2} {z}",
        8,
        16,
        64,
        1,
        true,
        Some((2, true)),
    );
    assert_evex_insert_chunk_case(
        &llvm_mc,
        "vinsertf32x4_ymm_vl",
        "vinsertf32x4 $1, %xmm16, %ymm18, %ymm17",
        4,
        16,
        32,
        1,
        false,
        None,
    );
}

#[test]
fn evex_non_temporal_moves_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX non-temporal move checks");
        return;
    };

    assert_evex_nt_store_case(
        &llvm_mc,
        "vmovntps_zmm_store",
        "vmovntps %zmm16, (%rax)",
        InputProfile::F32,
        64,
    );
    assert_evex_nt_store_case(
        &llvm_mc,
        "vmovntpd_ymm_store",
        "vmovntpd %ymm16, (%rax)",
        InputProfile::F64,
        32,
    );
    assert_evex_nt_store_case(
        &llvm_mc,
        "vmovntdq_xmm_store",
        "vmovntdq %xmm16, (%rax)",
        InputProfile::Int,
        16,
    );
    assert_evex_nt_load_case(
        &llvm_mc,
        "vmovntdqa_zmm_load",
        "vmovntdqa (%rax), %zmm17",
        64,
    );
    assert_evex_nt_load_case(
        &llvm_mc,
        "vmovntdqa_ymm_load",
        "vmovntdqa (%rax), %ymm17",
        32,
    );
}

#[test]
fn evex_block_broadcast_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX block broadcast semantic checks");
        return;
    };

    assert_evex_block_broadcast_case(
        &llvm_mc,
        "vbroadcastf32x2_reg_merge",
        "vbroadcastf32x2 %xmm16, %zmm17 {%k1}",
        InputProfile::F32,
        4,
        8,
        64,
        false,
        Some((1, false)),
    );
    assert_evex_block_broadcast_case(
        &llvm_mc,
        "vbroadcastf32x4_mem_zero",
        "vbroadcastf32x4 (%rax), %zmm17 {%k2} {z}",
        InputProfile::F32,
        4,
        16,
        64,
        true,
        Some((2, true)),
    );
    assert_evex_block_broadcast_case(
        &llvm_mc,
        "vbroadcastf64x4_zmm_mem",
        "vbroadcastf64x4 (%rax), %zmm17",
        InputProfile::F64,
        8,
        32,
        64,
        true,
        None,
    );
    assert_evex_block_broadcast_case(
        &llvm_mc,
        "vbroadcasti32x2_xmm_mem",
        "vbroadcasti32x2 (%rax), %xmm17",
        InputProfile::Int,
        4,
        8,
        16,
        true,
        None,
    );
    assert_evex_block_broadcast_case(
        &llvm_mc,
        "vbroadcasti64x2_ymm_mem_merge",
        "vbroadcasti64x2 (%rax), %ymm17 {%k1}",
        InputProfile::Int,
        8,
        16,
        32,
        true,
        Some((1, false)),
    );
    assert_evex_block_broadcast_case(
        &llvm_mc,
        "vbroadcasti32x8_zmm_mem_zero",
        "vbroadcasti32x8 (%rax), %zmm17 {%k2} {z}",
        InputProfile::Int,
        4,
        32,
        64,
        true,
        Some((2, true)),
    );
}

#[test]
fn evex_unpack_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX unpack semantic checks");
        return;
    };

    assert_evex_unpack_case(
        &llvm_mc,
        "vunpcklps_mem_broadcast_merge",
        "vunpcklps (%rax){1to16}, %zmm18, %zmm17 {%k1}",
        InputProfile::F32,
        4,
        64,
        false,
        true,
        Some((1, false)),
    );
    assert_evex_unpack_case(
        &llvm_mc,
        "vunpckhpd_zero_high_regs",
        "vunpckhpd %zmm16, %zmm18, %zmm17 {%k2} {z}",
        InputProfile::F64,
        8,
        64,
        true,
        false,
        Some((2, true)),
    );
    assert_evex_unpack_case(
        &llvm_mc,
        "vpunpcklbw_memory",
        "vpunpcklbw (%rax), %zmm18, %zmm17",
        InputProfile::Int,
        1,
        64,
        false,
        false,
        None,
    );
    assert_evex_unpack_case(
        &llvm_mc,
        "vpunpckhwd_ymm_zero",
        "vpunpckhwd %ymm16, %ymm18, %ymm17 {%k2} {z}",
        InputProfile::Int,
        2,
        32,
        true,
        false,
        Some((2, true)),
    );
    assert_evex_unpack_case(
        &llvm_mc,
        "vpunpckldq_mem_broadcast",
        "vpunpckldq (%rax){1to16}, %zmm18, %zmm17",
        InputProfile::Int,
        4,
        64,
        false,
        true,
        None,
    );
    assert_evex_unpack_case(
        &llvm_mc,
        "vpunpckhqdq_ymm_merge",
        "vpunpckhqdq %ymm16, %ymm18, %ymm17 {%k1}",
        InputProfile::Int,
        8,
        32,
        true,
        false,
        Some((1, false)),
    );
}

#[test]
fn evex_byte_word_expand_compress_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX byte/word expand-compress checks");
        return;
    };

    assert_evex_expand_case(
        &llvm_mc,
        "vpexpandb_merge_high_regs",
        "vpexpandb %zmm16, %zmm17 {%k1}",
        1,
        64,
        false,
        Some((1, false)),
    );
    assert_evex_expand_case(
        &llvm_mc,
        "vpexpandw_zero_memory",
        "vpexpandw (%rax), %zmm17 {%k2} {z}",
        2,
        64,
        true,
        Some((2, true)),
    );

    assert_evex_compress_reg_case(
        &llvm_mc,
        "vpcompressb_merge_high_regs",
        "vpcompressb %zmm17, %zmm16 {%k1}",
        1,
        64,
        1,
        false,
    );
    assert_evex_compress_reg_case(
        &llvm_mc,
        "vpcompressw_zero_high_regs",
        "vpcompressw %zmm17, %zmm16 {%k2} {z}",
        2,
        64,
        2,
        true,
    );
    assert_evex_compress_mem_case(
        &llvm_mc,
        "vpcompressw_memory",
        "vpcompressw %zmm17, (%rax) {%k1}",
        2,
        64,
        1,
    );
}

#[test]
fn evex_funnel_shift_instructions_match_contracts() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX funnel shift semantic checks");
        return;
    };

    assert_evex_funnel_shift_imm_case(
        &llvm_mc,
        "vpshldw_high_regs",
        "vpshldw $5, %zmm16, %zmm18, %zmm17",
        2,
        true,
        5,
        false,
        None,
    );
    assert_evex_funnel_shift_imm_case(
        &llvm_mc,
        "vpshldq_zero_mask",
        "vpshldq $5, %zmm16, %zmm18, %zmm17 {%k2} {z}",
        8,
        true,
        5,
        false,
        Some((2, true)),
    );
    assert_evex_funnel_shift_imm_case(
        &llvm_mc,
        "vpshrdd_mem_broadcast",
        "vpshrdd $5, (%rax){1to16}, %zmm18, %zmm17",
        4,
        false,
        5,
        true,
        None,
    );
    assert_evex_funnel_shift_imm_case(
        &llvm_mc,
        "vpshrdq_merge_mask",
        "vpshrdq $7, %zmm16, %zmm18, %zmm17 {%k1}",
        8,
        false,
        7,
        false,
        Some((1, false)),
    );

    assert_evex_funnel_shift_var_case(
        &llvm_mc,
        "vpshldvd_high_regs",
        "vpshldvd %zmm16, %zmm18, %zmm17",
        4,
        true,
        false,
        None,
    );
    assert_evex_funnel_shift_var_case(
        &llvm_mc,
        "vpshldvq_mem_broadcast",
        "vpshldvq (%rax){1to8}, %zmm18, %zmm17",
        8,
        true,
        true,
        None,
    );
    assert_evex_funnel_shift_var_case(
        &llvm_mc,
        "vpshrdvw_high_regs",
        "vpshrdvw %zmm16, %zmm18, %zmm17",
        2,
        false,
        false,
        None,
    );
    assert_evex_funnel_shift_var_case(
        &llvm_mc,
        "vpshrdvq_zero_mask",
        "vpshrdvq %zmm16, %zmm18, %zmm17 {%k2} {z}",
        8,
        false,
        false,
        Some((2, true)),
    );
}

#[test]
fn evex_avx512_spec_diff_corpus_covers_every_spec_case_variant() {
    let cases = classified_avx512_evex_diff_cases_from_spec();
    let expected = avx512_spec_case_variant_ids();
    let classified = classified_avx512_evex_mnemonics();
    let actual = cases
        .iter()
        .map(|case| {
            case.label
                .strip_prefix("avx512::")
                .expect("AVX-512 spec case label prefix")
                .to_string()
        })
        .collect::<BTreeSet<_>>();

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
        "AVX-512 EVEX differential corpus coverage mismatch\nmissing:\n{}\nunexpected:\n{}",
        missing.into_iter().take(80).collect::<Vec<_>>().join("\n"),
        unexpected
            .into_iter()
            .take(80)
            .collect::<Vec<_>>()
            .join("\n")
    );

    let mnemonic_count = avx512_spec_evex_rows()
        .into_iter()
        .map(|row| row.key.mnemonic)
        .collect::<BTreeSet<_>>()
        .len();
    assert!(
        cases.len() > mnemonic_count,
        "AVX-512 EVEX differential corpus must be case-variant-level, not mnemonic-level"
    );
    for case in &cases {
        assert!(
            case.op.len() >= 6 && case.op[0] == 0x62,
            "{} generated outside EVEX encoding: {:02x?}",
            case.label,
            case.op
        );
    }

    assert_spec_diff_corpus_shape(&cases, "avx512::", &classified, true);
}

#[test]
fn evex_unimplemented_avx512_diff_corpus_covers_every_spec_case_variant() {
    let (cases, expected) = unimplemented_evex_diff_cases_from_spec();
    let unimplemented = set_from_manifest(UNIMPLEMENTED_AVX512);
    let actual = cases
        .iter()
        .map(|case| {
            case.label
                .strip_prefix("unimplemented::")
                .expect("unimplemented case label prefix")
                .to_string()
        })
        .collect::<BTreeSet<_>>();

    if unimplemented.is_empty() {
        assert!(
            expected.is_empty() && cases.is_empty(),
            "unimplemented EVEX differential corpus must be empty when the manifest is empty"
        );
        return;
    }

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
        "unimplemented EVEX differential corpus coverage mismatch\nmissing:\n{}\nunexpected:\n{}",
        missing.into_iter().take(80).collect::<Vec<_>>().join("\n"),
        unexpected
            .into_iter()
            .take(80)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        cases.len() > set_from_manifest(UNIMPLEMENTED_AVX512).len(),
        "unimplemented EVEX differential corpus must be case-variant-level, not mnemonic-level"
    );
    for case in &cases {
        assert!(
            case.op.len() >= 6 && case.op[0] == 0x62,
            "{} generated outside EVEX encoding: {:02x?}",
            case.label,
            case.op
        );
    }

    assert_spec_diff_corpus_shape(&cases, "unimplemented::", &unimplemented, false);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn qemu_evex_generated_corpus_matches_rax() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc unavailable; skipping EVEX differential corpus");
        return;
    };

    let cases = assembled_cases(&llvm_mc);
    if cases.is_empty() {
        eprintln!("[skip] llvm-mc did not assemble any EVEX differential cases");
        return;
    }

    let Some(qemu) = qemu_path() else {
        eprintln!("[skip] qemu-x86_64 unavailable; skipping EVEX differential corpus");
        return;
    };

    let Some(oracle) = oracle_path(&cases) else {
        eprintln!("[skip] EVEX oracle build failed or compiler unavailable");
        return;
    };
    let Some(outputs) = run_oracle(&qemu, &oracle, &cases) else {
        eprintln!("[skip] qemu-x86_64 could not run the EVEX oracle");
        return;
    };

    let mut compared = 0usize;
    let mut qemu_unsupported = 0usize;
    for (case, oracle) in cases.iter().zip(outputs.iter()) {
        assert_eq!(oracle.id, case.id, "{}: oracle case id", case.label);
        if oracle.valid == 0 {
            qemu_unsupported += 1;
            continue;
        }
        let rax = run_rax(case);
        assert_same_snapshot(case, &rax, oracle);
        compared += 1;
    }

    if compared == 0 {
        eprintln!(
            "[skip] qemu rejected all {} generated EVEX differential cases",
            qemu_unsupported
        );
    } else {
        eprintln!("[info] compared {compared} EVEX cases; qemu rejected {qemu_unsupported}");
    }
}

#[test]
#[ignore = "semantic audit for AVX-512 instructions intentionally still listed as unimplemented"]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn qemu_evex_unimplemented_avx512_corpus_matches_rax_when_enabled() {
    let (cases, _) = unimplemented_evex_diff_cases_from_spec();
    assert!(
        !cases.is_empty(),
        "unimplemented AVX-512 EVEX differential corpus is empty"
    );

    let Some(qemu) = qemu_path() else {
        eprintln!("[skip] qemu-x86_64 unavailable; skipping unimplemented EVEX differential audit");
        return;
    };

    let Some(oracle) = oracle_path(&cases) else {
        eprintln!("[skip] EVEX oracle build failed or compiler unavailable");
        return;
    };
    let Some(outputs) = run_oracle(&qemu, &oracle, &cases) else {
        eprintln!("[skip] qemu-x86_64 could not run the unimplemented EVEX oracle");
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
        "qemu rejected all {} unimplemented EVEX cases",
        qemu_unsupported
    );
    assert!(
        rax_failures.is_empty(),
        "unimplemented EVEX differential audit found rax execution failures\ncompared: {compared}\nqemu rejected: {qemu_unsupported}\nfailures:\n{}",
        rax_failures
            .into_iter()
            .take(80)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
