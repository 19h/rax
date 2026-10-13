//! GENERATOR: memory operands followed by an immediate.
//!
//! A RIP-relative displacement is relative to the end of the whole
//! instruction, so the immediate after it is part of the distance (Intel SDM
//! Vol. 2A section 2.2.1.6). `fuzz_mem_addressing` covers every addressing
//! form with MOV, which has no immediate; this crosses every form, weighted
//! toward RIP-relative, with instructions that end in one: the legacy SSE
//! imm8 forms (0F 70, C2, C4, C6, and the 0F3A map) and the one-byte-map and
//! 0F immediate groups (Ib, and Iz with 16- and 32-bit operand sizes). A
//! wrong decoded address surfaces as a wrong register or scratch result. KVM
//! is the oracle; VEX and EVEX forms need OSXSAVE, which this harness's CR4
//! leaves clear, and are covered by `vex_rip_relative`/`evex_rip_relative`
//! and the library's RIP-relative sweep.

use super::addr_form::{ADDR_FORMS, AddrForm, encode_ea_operand};
use super::{CompareOpts, DATA_ADDR, FLAG_MASK, HLT, Harness, set_reg};
use rax::isa::x86_64::flags;
use rax::vm::vcpu::Registers;

/// The host feature an instruction needs to execute natively under KVM.
#[derive(Clone, Copy, Debug)]
enum Feature {
    Sse2,
    Ssse3,
    Sse41,
    Sse42,
    Aes,
    Pclmulqdq,
    Sha,
    Gfni,
}

impl Feature {
    fn present(self) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            match self {
                Feature::Sse2 => true,
                Feature::Ssse3 => std::is_x86_feature_detected!("ssse3"),
                Feature::Sse41 => std::is_x86_feature_detected!("sse4.1"),
                Feature::Sse42 => std::is_x86_feature_detected!("sse4.2"),
                Feature::Aes => std::is_x86_feature_detected!("aes"),
                Feature::Pclmulqdq => std::is_x86_feature_detected!("pclmulqdq"),
                Feature::Sha => std::is_x86_feature_detected!("sha"),
                Feature::Gfni => std::is_x86_feature_detected!("gfni"),
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = self;
            false
        }
    }
}

/// How register and memory inputs are drawn.
#[derive(Clone, Copy)]
enum Data {
    /// Arbitrary bit patterns.
    Bits,
    /// Small integers as f32 lanes, so floating-point results are exact.
    F32,
    /// Small integers as f64 lanes.
    F64,
}

impl Data {
    /// Sixteen bytes of input.
    fn draw(self, rng: &mut super::Rng) -> [u8; 16] {
        let mut out = [0u8; 16];
        match self {
            Data::Bits => out.copy_from_slice(
                &[rng.next_u64().to_le_bytes(), rng.next_u64().to_le_bytes()].concat(),
            ),
            Data::F32 => {
                for lane in out.chunks_mut(4) {
                    let value = rng.below(512) as f32 - 256.0;
                    lane.copy_from_slice(&value.to_bits().to_le_bytes());
                }
            }
            Data::F64 => {
                for lane in out.chunks_mut(8) {
                    let value = rng.below(512) as f64 - 256.0;
                    lane.copy_from_slice(&value.to_bits().to_le_bytes());
                }
            }
        }
        out
    }
}

/// What ModR/M.reg names.
#[derive(Clone, Copy)]
enum Reg {
    /// An XMM register (0-15, via REX.R).
    Xmm,
    /// A GPR other than the addressing registers rbx/rsi and rsp/rbp.
    Gpr,
    /// A fixed opcode extension.
    Digit(u8),
}

#[derive(Clone, Copy)]
struct ImmOp {
    name: &'static str,
    /// Mandatory or operand-size prefix (66/F2/F3), before any REX.
    prefix: &'static [u8],
    rex_w: bool,
    /// Opcode bytes, including the 0F / 0F 3A escapes.
    opcode: &'static [u8],
    reg: Reg,
    /// Immediate length in bytes.
    imm_len: usize,
    /// Mask of the immediate bits the case may set (reserved bits stay 0).
    imm_mask: u32,
    /// Memory operand length in bytes.
    width: u64,
    /// The legacy SSE m128 operand must be 16-byte aligned.
    aligned: bool,
    feature: Feature,
    /// Architecturally defined RFLAGS bits the instruction writes.
    flag_mask: u64,
    data: Data,
}

const SSE: ImmOp = ImmOp {
    name: "",
    prefix: &[0x66],
    rex_w: false,
    opcode: &[],
    reg: Reg::Xmm,
    imm_len: 1,
    imm_mask: 0xFF,
    width: 16,
    aligned: true,
    feature: Feature::Sse41,
    flag_mask: FLAG_MASK,
    data: Data::Bits,
};

const GPR: ImmOp = ImmOp {
    name: "",
    prefix: &[],
    rex_w: false,
    opcode: &[],
    reg: Reg::Digit(0),
    imm_len: 1,
    imm_mask: 0xFF,
    width: 4,
    aligned: false,
    feature: Feature::Sse2,
    flag_mask: FLAG_MASK,
    data: Data::Bits,
};

const CF_OF: u64 = flags::bits::CF | flags::bits::OF;

const OPS: &[ImmOp] = &[
    // 0F3A inserts and extracts (unaligned scalar operands).
    ImmOp {
        name: "pinsrb",
        opcode: &[0x0F, 0x3A, 0x20],
        width: 1,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "pinsrd",
        opcode: &[0x0F, 0x3A, 0x22],
        width: 4,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "pinsrq",
        rex_w: true,
        opcode: &[0x0F, 0x3A, 0x22],
        width: 8,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "insertps",
        opcode: &[0x0F, 0x3A, 0x21],
        width: 4,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "pextrb",
        opcode: &[0x0F, 0x3A, 0x14],
        width: 1,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "pextrw",
        opcode: &[0x0F, 0x3A, 0x15],
        width: 2,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "pextrd",
        opcode: &[0x0F, 0x3A, 0x16],
        width: 4,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "pextrq",
        rex_w: true,
        opcode: &[0x0F, 0x3A, 0x16],
        width: 8,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "extractps",
        opcode: &[0x0F, 0x3A, 0x17],
        width: 4,
        aligned: false,
        ..SSE
    },
    // 0F3A packed operations (aligned m128).
    ImmOp {
        name: "palignr",
        opcode: &[0x0F, 0x3A, 0x0F],
        feature: Feature::Ssse3,
        ..SSE
    },
    ImmOp {
        name: "roundps",
        data: Data::F32,
        opcode: &[0x0F, 0x3A, 0x08],
        imm_mask: 0x0F,
        ..SSE
    },
    ImmOp {
        name: "roundpd",
        data: Data::F64,
        opcode: &[0x0F, 0x3A, 0x09],
        imm_mask: 0x0F,
        ..SSE
    },
    ImmOp {
        name: "roundss",
        data: Data::F32,
        opcode: &[0x0F, 0x3A, 0x0A],
        imm_mask: 0x0F,
        width: 4,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "roundsd",
        data: Data::F64,
        opcode: &[0x0F, 0x3A, 0x0B],
        imm_mask: 0x0F,
        width: 8,
        aligned: false,
        ..SSE
    },
    ImmOp {
        name: "blendps",
        opcode: &[0x0F, 0x3A, 0x0C],
        imm_mask: 0x0F,
        ..SSE
    },
    ImmOp {
        name: "blendpd",
        opcode: &[0x0F, 0x3A, 0x0D],
        imm_mask: 0x03,
        ..SSE
    },
    ImmOp {
        name: "pblendw",
        opcode: &[0x0F, 0x3A, 0x0E],
        ..SSE
    },
    ImmOp {
        name: "dpps",
        data: Data::F32,
        opcode: &[0x0F, 0x3A, 0x40],
        ..SSE
    },
    ImmOp {
        name: "dppd",
        data: Data::F64,
        opcode: &[0x0F, 0x3A, 0x41],
        imm_mask: 0x33,
        ..SSE
    },
    ImmOp {
        name: "mpsadbw",
        opcode: &[0x0F, 0x3A, 0x42],
        imm_mask: 0x07,
        ..SSE
    },
    ImmOp {
        name: "pclmulqdq",
        opcode: &[0x0F, 0x3A, 0x44],
        imm_mask: 0x11,
        feature: Feature::Pclmulqdq,
        ..SSE
    },
    ImmOp {
        name: "pcmpestrm",
        opcode: &[0x0F, 0x3A, 0x60],
        imm_mask: 0x7F,
        aligned: false,
        feature: Feature::Sse42,
        ..SSE
    },
    ImmOp {
        name: "pcmpestri",
        opcode: &[0x0F, 0x3A, 0x61],
        imm_mask: 0x7F,
        aligned: false,
        feature: Feature::Sse42,
        ..SSE
    },
    ImmOp {
        name: "pcmpistrm",
        opcode: &[0x0F, 0x3A, 0x62],
        imm_mask: 0x7F,
        aligned: false,
        feature: Feature::Sse42,
        ..SSE
    },
    ImmOp {
        name: "pcmpistri",
        opcode: &[0x0F, 0x3A, 0x63],
        imm_mask: 0x7F,
        aligned: false,
        feature: Feature::Sse42,
        ..SSE
    },
    ImmOp {
        name: "gf2p8affineqb",
        opcode: &[0x0F, 0x3A, 0xCE],
        feature: Feature::Gfni,
        ..SSE
    },
    ImmOp {
        name: "gf2p8affineinvqb",
        opcode: &[0x0F, 0x3A, 0xCF],
        feature: Feature::Gfni,
        ..SSE
    },
    ImmOp {
        name: "aeskeygenassist",
        opcode: &[0x0F, 0x3A, 0xDF],
        feature: Feature::Aes,
        ..SSE
    },
    ImmOp {
        name: "sha1rnds4",
        prefix: &[],
        opcode: &[0x0F, 0x3A, 0xCC],
        imm_mask: 0x03,
        feature: Feature::Sha,
        ..SSE
    },
    // The 0F-map imm8 forms.
    ImmOp {
        name: "pshufd",
        opcode: &[0x0F, 0x70],
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "pshufhw",
        prefix: &[0xF3],
        opcode: &[0x0F, 0x70],
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "pshuflw",
        prefix: &[0xF2],
        opcode: &[0x0F, 0x70],
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "cmpps",
        data: Data::F32,
        prefix: &[],
        opcode: &[0x0F, 0xC2],
        imm_mask: 0x07,
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "cmppd",
        data: Data::F64,
        opcode: &[0x0F, 0xC2],
        imm_mask: 0x07,
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "cmpss",
        data: Data::F32,
        prefix: &[0xF3],
        opcode: &[0x0F, 0xC2],
        imm_mask: 0x07,
        width: 4,
        aligned: false,
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "cmpsd",
        data: Data::F64,
        prefix: &[0xF2],
        opcode: &[0x0F, 0xC2],
        imm_mask: 0x07,
        width: 8,
        aligned: false,
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "pinsrw",
        opcode: &[0x0F, 0xC4],
        imm_mask: 0x07,
        width: 2,
        aligned: false,
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "shufps",
        prefix: &[],
        opcode: &[0x0F, 0xC6],
        feature: Feature::Sse2,
        ..SSE
    },
    ImmOp {
        name: "shufpd",
        opcode: &[0x0F, 0xC6],
        imm_mask: 0x03,
        feature: Feature::Sse2,
        ..SSE
    },
    // One-byte-map Ib / Iz groups and the 0F immediate groups.
    ImmOp {
        name: "add.b imm8",
        opcode: &[0x80],
        width: 1,
        ..GPR
    },
    ImmOp {
        name: "sub.w imm16",
        prefix: &[0x66],
        opcode: &[0x81],
        reg: Reg::Digit(5),
        imm_len: 2,
        imm_mask: 0xFFFF,
        width: 2,
        ..GPR
    },
    ImmOp {
        name: "and.d imm32",
        opcode: &[0x81],
        reg: Reg::Digit(4),
        imm_len: 4,
        imm_mask: u32::MAX,
        ..GPR
    },
    ImmOp {
        name: "cmp.q imm32",
        rex_w: true,
        opcode: &[0x81],
        reg: Reg::Digit(7),
        imm_len: 4,
        imm_mask: u32::MAX,
        width: 8,
        ..GPR
    },
    ImmOp {
        name: "xor.q imm8",
        rex_w: true,
        opcode: &[0x83],
        reg: Reg::Digit(6),
        width: 8,
        ..GPR
    },
    ImmOp {
        name: "mov.b imm8",
        opcode: &[0xC6],
        width: 1,
        flag_mask: 0,
        ..GPR
    },
    ImmOp {
        name: "mov.w imm16",
        prefix: &[0x66],
        opcode: &[0xC7],
        imm_len: 2,
        imm_mask: 0xFFFF,
        width: 2,
        flag_mask: 0,
        ..GPR
    },
    ImmOp {
        name: "mov.q imm32",
        rex_w: true,
        opcode: &[0xC7],
        imm_len: 4,
        imm_mask: u32::MAX,
        width: 8,
        flag_mask: 0,
        ..GPR
    },
    ImmOp {
        name: "test.d imm32",
        opcode: &[0xF7],
        imm_len: 4,
        imm_mask: u32::MAX,
        flag_mask: FLAG_MASK & !flags::bits::AF,
        ..GPR
    },
    ImmOp {
        name: "test.b imm8",
        opcode: &[0xF6],
        width: 1,
        flag_mask: FLAG_MASK & !flags::bits::AF,
        ..GPR
    },
    // Rotates and shifts with an imm8 count: only the memory result is
    // compared (some flags are undefined for some counts).
    ImmOp {
        name: "rol.b imm8",
        opcode: &[0xC0],
        reg: Reg::Digit(0),
        width: 1,
        flag_mask: 0,
        ..GPR
    },
    ImmOp {
        name: "shl.d imm8",
        opcode: &[0xC1],
        reg: Reg::Digit(4),
        flag_mask: 0,
        ..GPR
    },
    ImmOp {
        name: "imul.w imm16",
        prefix: &[0x66],
        opcode: &[0x69],
        reg: Reg::Gpr,
        imm_len: 2,
        imm_mask: 0xFFFF,
        width: 2,
        flag_mask: CF_OF,
        ..GPR
    },
    ImmOp {
        name: "imul.d imm32",
        opcode: &[0x69],
        reg: Reg::Gpr,
        imm_len: 4,
        imm_mask: u32::MAX,
        flag_mask: CF_OF,
        ..GPR
    },
    ImmOp {
        name: "imul.q imm8",
        rex_w: true,
        opcode: &[0x6B],
        reg: Reg::Gpr,
        width: 8,
        flag_mask: CF_OF,
        ..GPR
    },
    ImmOp {
        name: "bt.d imm8",
        opcode: &[0x0F, 0xBA],
        reg: Reg::Digit(4),
        flag_mask: flags::bits::CF,
        ..GPR
    },
    ImmOp {
        name: "bts.q imm8",
        rex_w: true,
        opcode: &[0x0F, 0xBA],
        reg: Reg::Digit(5),
        width: 8,
        flag_mask: flags::bits::CF,
        ..GPR
    },
    ImmOp {
        name: "shld.d imm8",
        opcode: &[0x0F, 0xA4],
        reg: Reg::Gpr,
        flag_mask: 0,
        ..GPR
    },
    ImmOp {
        name: "shrd.q imm8",
        rex_w: true,
        opcode: &[0x0F, 0xAC],
        reg: Reg::Gpr,
        width: 8,
        flag_mask: 0,
        ..GPR
    },
];

/// `movdqu xmmN, [rip+disp32]` for N = 0..=15, each with its disp32 left
/// zero: the XMM inputs are loaded by the guest itself, from a block placed
/// after the HLT, so both backends start from the same registers whatever
/// path KVM uses to load vector state. Returns the code and, per register,
/// the offset of its disp32.
fn xmm_load_prologue() -> (Vec<u8>, Vec<usize>) {
    let mut code = Vec::new();
    let mut disps = Vec::new();
    for n in 0..16u8 {
        code.push(0xF3);
        if n >= 8 {
            code.push(0x44); // REX.R
        }
        code.extend_from_slice(&[0x0F, 0x6F, ((n & 7) << 3) | 0b101]);
        disps.push(code.len());
        code.extend_from_slice(&[0; 4]);
    }
    (code, disps)
}

/// Appends the sixteen XMM inputs to `code` (after its HLT) and points the
/// prologue's loads at them.
fn patch_xmm_loads(code: &mut Vec<u8>, disps: &[usize], inputs: &[[u8; 16]]) {
    let block = code.len();
    for input in inputs {
        code.extend_from_slice(input);
    }
    for (n, &at) in disps.iter().enumerate() {
        let next = at + 4;
        let disp = (block + 16 * n) as i64 - next as i64;
        code[at..at + 4].copy_from_slice(&(disp as i32).to_le_bytes());
    }
}

/// GPRs usable in ModR/M.reg: not rsp/rbp, nor the addressing registers.
const FREE_GPRS: [u8; 4] = [0, 1, 2, 7];

#[test]
fn fuzz_imm_rip_relative() {
    const CASES: usize = 2400;
    let mut h = Harness::new(0x1A3E_D1A7_E0FF_5E7B);
    let ops: Vec<ImmOp> = OPS
        .iter()
        .copied()
        .filter(|op| op.feature.present())
        .collect();
    let missing: Vec<_> = OPS
        .iter()
        .filter(|op| !op.feature.present())
        .map(|op| format!("{} ({:?})", op.name, op.feature))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "[skip] imm_rip_relative: host lacks the feature for {} of {} ops: {}",
            missing.len(),
            OPS.len(),
            missing.join(", ")
        );
    }

    for _ in 0..CASES {
        let op = *h.rng.pick(&ops);
        // Half of the cases are RIP-relative; the rest use every form.
        let form = if h.rng.below(2) == 0 {
            AddrForm::RipRel
        } else {
            *h.rng.pick(&ADDR_FORMS)
        };
        let target_off = if op.aligned {
            16 * h.rng.below(4)
        } else {
            h.rng.below(64 - op.width + 1)
        };
        let ea = DATA_ADDR + target_off;
        let scale_bits = h.rng.below(4) as u8;
        let idxv = h.rng.below(8);

        let mut r = Registers::default();
        let xmm_inputs: Vec<[u8; 16]> = (0..16).map(|_| op.data.draw(&mut h.rng)).collect();
        // PCMPESTR* string lengths; IMUL/SHLD sources.
        set_reg(&mut r, 0, h.rng.below(33).wrapping_sub(16));
        set_reg(&mut r, 2, h.rng.below(33).wrapping_sub(16));
        set_reg(&mut r, 1, h.rng.operand());
        set_reg(&mut r, 7, h.rng.operand());

        let (reg, rex_r) = match op.reg {
            Reg::Xmm => {
                let n = h.rng.below(16) as u8;
                (n & 7, n >= 8)
            }
            Reg::Gpr => (*h.rng.pick(&FREE_GPRS), false),
            Reg::Digit(digit) => (digit, false),
        };
        let (mut code, xmm_loads) = xmm_load_prologue();
        code.extend_from_slice(op.prefix);
        let rex = 0x40 | (u8::from(op.rex_w) << 3) | (u8::from(rex_r) << 2);
        if rex != 0x40 {
            code.push(rex);
        }
        code.extend_from_slice(op.opcode);
        encode_ea_operand(
            &mut h.rng, form, reg, ea, scale_bits, idxv, op.imm_len, &mut code, &mut r,
        );
        let imm = h.rng.next_u32() & op.imm_mask;
        code.extend_from_slice(&imm.to_le_bytes()[..op.imm_len]);
        code.push(HLT);
        patch_xmm_loads(&mut code, &xmm_loads, &xmm_inputs);

        let mut scratch = [0u8; 64];
        for chunk in scratch.chunks_mut(16) {
            chunk.copy_from_slice(&op.data.draw(&mut h.rng));
        }

        let inputs = format!(
            "{} form#{} ea={ea:#x} off={target_off} scale={} idxv={idxv} imm={imm:#x}",
            op.name,
            form as u8,
            1u64 << scale_bits,
        );
        let opts = CompareOpts {
            flag_mask: op.flag_mask,
            xmm_count: 16,
            scratch: true,
            ..CompareOpts::default()
        };
        if !h.run_case("imm_rip_relative", &code, r, scratch, opts, inputs, &[]) {
            break;
        }
    }
    // `finish` lists the first divergences; show backend errors too, which
    // would otherwise hide behind them.
    for error in h
        .mismatches
        .iter()
        .filter(|m| m.label.ends_with("backend error)"))
        .take(5)
    {
        eprintln!("{}", error.render());
    }
    h.finish("imm_rip_relative");
}
