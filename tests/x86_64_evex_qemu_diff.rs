//! EVEX r/m register-extension differential tests: rax vs. qemu-x86_64.
//!
//! These tests exercise EVEX register r/m operands where the architectural
//! register number is EVEX.X:EVEX.B:ModRM.rm. They self-skip unless a Linux
//! qemu-x86_64 user-mode oracle and a C compiler are available.

#![cfg(all(feature = "x86_64-suite", target_os = "linux", target_arch = "x86_64"))]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[path = "x86_64/common/mod.rs"]
mod common;

use common::{Bytes, GuestAddress, run_until_hlt, setup_vm};

const WIRE_MAGIC: u32 = 0x5845_5645; // 'E','V','E','X'

const CASE_VMOVAPS_ZMM0: u32 = 0;
const CASE_VMOVAPS_ZMM8: u32 = 1;
const CASE_VMOVAPS_ZMM16: u32 = 2;
const CASE_VMOVAPS_ZMM24: u32 = 3;
const CASE_VADDPS_ZMM16: u32 = 4;
const CASE_VADDPS_ZMM24: u32 = 5;

const ZMM0_ADDR: u64 = 0x3000;
const ZMM2_ADDR: u64 = 0x3040;
const ZMM8_ADDR: u64 = 0x3080;
const ZMM16_ADDR: u64 = 0x30c0;
const ZMM24_ADDR: u64 = 0x3100;
const RESULT_ADDR: u64 = 0x3140;

#[repr(C)]
#[derive(Clone, Copy)]
struct InCase {
    id: u32,
    reserved: u32,
    zmm0: [u64; 8],
    zmm2: [u64; 8],
    zmm8: [u64; 8],
    zmm16: [u64; 8],
    zmm24: [u64; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct OutCase {
    id: u32,
    valid: u32,
    result: [u64; 8],
}

#[derive(Clone, Copy)]
struct DiffCase {
    label: &'static str,
    op: &'static [u8],
    input: InCase,
}

fn pattern(tag: u64) -> [u64; 8] {
    [
        0x0011_2233_4455_6600 | tag,
        0x1011_2233_4455_6600 | tag,
        0x2011_2233_4455_6600 | tag,
        0x3011_2233_4455_6600 | tag,
        0x4011_2233_4455_6600 | tag,
        0x5011_2233_4455_6600 | tag,
        0x6011_2233_4455_6600 | tag,
        0x7011_2233_4455_6600 | tag,
    ]
}

fn zmm_from_bytes(bytes: [u8; 64]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    out
}

fn splat_f32(value: f32) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    for chunk in bytes.chunks_exact_mut(4) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    zmm_from_bytes(bytes)
}

fn base_input(id: u32) -> InCase {
    InCase {
        id,
        reserved: 0,
        zmm0: pattern(0x00),
        zmm2: splat_f32(1.0),
        zmm8: pattern(0x08),
        zmm16: pattern(0x10),
        zmm24: pattern(0x18),
    }
}

fn vadd_input(id: u32) -> InCase {
    InCase {
        id,
        reserved: 0,
        zmm0: splat_f32(-64.0),
        zmm2: splat_f32(1.0),
        zmm8: splat_f32(-128.0),
        zmm16: splat_f32(2.0),
        zmm24: splat_f32(4.0),
    }
}

fn cases() -> Vec<DiffCase> {
    vec![
        DiffCase {
            label: "vmovaps_zmm1_zmm0",
            op: &[0x62, 0xf1, 0x7c, 0x48, 0x28, 0xc8],
            input: base_input(CASE_VMOVAPS_ZMM0),
        },
        DiffCase {
            label: "vmovaps_zmm1_zmm8",
            op: &[0x62, 0xd1, 0x7c, 0x48, 0x28, 0xc8],
            input: base_input(CASE_VMOVAPS_ZMM8),
        },
        DiffCase {
            label: "vmovaps_zmm1_zmm16",
            op: &[0x62, 0xb1, 0x7c, 0x48, 0x28, 0xc8],
            input: base_input(CASE_VMOVAPS_ZMM16),
        },
        DiffCase {
            label: "vmovaps_zmm1_zmm24",
            op: &[0x62, 0x91, 0x7c, 0x48, 0x28, 0xc8],
            input: base_input(CASE_VMOVAPS_ZMM24),
        },
        DiffCase {
            label: "vaddps_zmm1_zmm2_zmm16",
            op: &[0x62, 0xb1, 0x6c, 0x48, 0x58, 0xc8],
            input: vadd_input(CASE_VADDPS_ZMM16),
        },
        DiffCase {
            label: "vaddps_zmm1_zmm2_zmm24",
            op: &[0x62, 0x91, 0x6c, 0x48, 0x58, 0xc8],
            input: vadd_input(CASE_VADDPS_ZMM24),
        },
    ]
}

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

fn cc_path() -> Option<PathBuf> {
    std::env::var_os("CC")
        .map(PathBuf::from)
        .or_else(|| which("clang"))
        .or_else(|| which("cc"))
}

fn oracle_path() -> Option<PathBuf> {
    let cc = cc_path()?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("tools/x86_64-evex-diff/oracle.c");
    let build_dir = root.join("target/x86_64-evex-diff");
    let oracle = build_dir.join("oracle");
    std::fs::create_dir_all(&build_dir).ok()?;

    let needs_build = match (oracle.metadata(), src.metadata()) {
        (Ok(bin), Ok(source)) => match (bin.modified(), source.modified()) {
            (Ok(bin_time), Ok(source_time)) => bin_time < source_time,
            _ => true,
        },
        _ => true,
    };

    if needs_build {
        let status = Command::new(cc)
            .args(["-O2", "-std=c11", "-Wall", "-Wextra", "-mavx512f", "-o"])
            .arg(&oracle)
            .arg(&src)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
    }

    oracle.exists().then_some(oracle)
}

fn as_bytes<T: Copy>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn read_struct<T: Copy>(buf: &[u8], offset: usize) -> T {
    unsafe { std::ptr::read_unaligned(buf[offset..].as_ptr() as *const T) }
}

fn run_oracle(qemu: &PathBuf, oracle: &PathBuf, cases: &[DiffCase]) -> Option<Vec<OutCase>> {
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

fn zmm_to_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (i, word) in value.iter().enumerate() {
        bytes[i * 8..i * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn mov_imm64(code: &mut Vec<u8>, reg: u8, value: u64) {
    code.push(0x48);
    code.push(0xb8 + reg);
    code.extend_from_slice(&value.to_le_bytes());
}

fn evex_program(op: &[u8]) -> Vec<u8> {
    let mut code = Vec::new();
    mov_imm64(&mut code, 0, ZMM0_ADDR); // rax
    mov_imm64(&mut code, 2, ZMM2_ADDR); // rdx
    mov_imm64(&mut code, 3, ZMM8_ADDR); // rbx
    mov_imm64(&mut code, 6, ZMM16_ADDR); // rsi
    mov_imm64(&mut code, 7, ZMM24_ADDR); // rdi
    mov_imm64(&mut code, 1, RESULT_ADDR); // rcx

    code.extend_from_slice(&[0x62, 0xf1, 0x7c, 0x48, 0x10, 0x00]); // VMOVUPS zmm0, [rax]
    code.extend_from_slice(&[0x62, 0xf1, 0x7c, 0x48, 0x10, 0x12]); // VMOVUPS zmm2, [rdx]
    code.extend_from_slice(&[0x62, 0x71, 0x7c, 0x48, 0x10, 0x03]); // VMOVUPS zmm8, [rbx]
    code.extend_from_slice(&[0x62, 0xe1, 0x7c, 0x48, 0x10, 0x06]); // VMOVUPS zmm16, [rsi]
    code.extend_from_slice(&[0x62, 0x61, 0x7c, 0x48, 0x10, 0x07]); // VMOVUPS zmm24, [rdi]
    code.extend_from_slice(op);
    code.extend_from_slice(&[0x62, 0xf1, 0x7c, 0x48, 0x11, 0x09]); // VMOVUPS [rcx], zmm1
    code.push(0xf4);
    code
}

fn run_rax(case: &DiffCase) -> [u64; 8] {
    let code = evex_program(case.op);
    let (mut vcpu, mem) = setup_vm(&code, None);
    mem.write_slice(&zmm_to_bytes(case.input.zmm0), GuestAddress(ZMM0_ADDR))
        .unwrap();
    mem.write_slice(&zmm_to_bytes(case.input.zmm2), GuestAddress(ZMM2_ADDR))
        .unwrap();
    mem.write_slice(&zmm_to_bytes(case.input.zmm8), GuestAddress(ZMM8_ADDR))
        .unwrap();
    mem.write_slice(&zmm_to_bytes(case.input.zmm16), GuestAddress(ZMM16_ADDR))
        .unwrap();
    mem.write_slice(&zmm_to_bytes(case.input.zmm24), GuestAddress(ZMM24_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();

    let mut result = [0u8; 64];
    mem.read_slice(&mut result, GuestAddress(RESULT_ADDR))
        .unwrap();
    zmm_from_bytes(result)
}

#[test]
fn qemu_evex_rm_register_extension_matches_rax() {
    let Some(qemu) = qemu_path() else {
        eprintln!("[skip] qemu-x86_64 unavailable; skipping EVEX differential cases");
        return;
    };
    let Some(oracle) = oracle_path() else {
        eprintln!("[skip] EVEX oracle build failed or compiler unavailable");
        return;
    };

    let cases = cases();
    let Some(outputs) = run_oracle(&qemu, &oracle, &cases) else {
        eprintln!("[skip] qemu-x86_64 could not run the EVEX oracle");
        return;
    };

    for (case, oracle) in cases.iter().zip(outputs.iter()) {
        assert_eq!(oracle.id, case.input.id, "{}: oracle case id", case.label);
        assert_eq!(oracle.valid, 1, "{}: oracle rejected case", case.label);
        assert_eq!(
            run_rax(case),
            oracle.result,
            "{} diverged from qemu",
            case.label
        );
    }
}
