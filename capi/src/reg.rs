//! Register access by stable, architecture-namespaced register id.
//!
//! Register ids use a compact, range-based encoding documented in `rax.h` (the
//! header exposes both `RAX_*_REG_*(i)` family macros and named aliases that
//! evaluate to the same numbers, so the C constants and this decoder cannot
//! drift). Each architecture's id space starts fresh; the engine interprets ids
//! according to its own architecture.
//!
//! Values are transferred as little-endian byte sequences sized to the natural
//! width of the register (queryable via `rax_reg_size`). Wide vector registers
//! (XMM/YMM/ZMM, AArch64 V, Hexagon V/Q) are transferred as raw byte arrays.

use std::os::raw::c_int;

use rax_engine::cpu::{CpuState, Registers, Segment, SystemRegisters};

use crate::arch::RaxArch;
use crate::engine::{engine_mut, engine_ref, Engine};
use crate::guard;
use crate::status::RaxStatus;

const MAX_REG_BYTES: usize = 64;

// --- LE helpers ------------------------------------------------------------

fn put_uint(out: &mut [u8], v: u64, width: usize) {
    let b = v.to_le_bytes();
    out[..width].copy_from_slice(&b[..width]);
}
fn get_uint(inp: &[u8], width: usize) -> u64 {
    let mut b = [0u8; 8];
    b[..width].copy_from_slice(&inp[..width]);
    u64::from_le_bytes(b)
}

// ===========================================================================
// x86 register decode
// ===========================================================================

// Family bases (see rax.h). Index ranges are documented per family.
const X86_GPR64: i32 = 0x0100;
const X86_GPR32: i32 = 0x0200;
const X86_GPR16: i32 = 0x0300;
const X86_GPR8L: i32 = 0x0400;
const X86_GPR8H: i32 = 0x0500;
const X86_SEG_SEL: i32 = 0x0600;
const X86_SEG_BASE: i32 = 0x0700;
const X86_SEG_LIMIT: i32 = 0x0800;
const X86_CR: i32 = 0x0900;
const X86_DR: i32 = 0x0A00;
const X86_XMM: i32 = 0x0B00;
const X86_YMM: i32 = 0x0C00;
const X86_ZMM: i32 = 0x0D00;
const X86_K: i32 = 0x0E00;
const X86_MM: i32 = 0x0F00;

const X86_RIP: i32 = 0x0010;
const X86_EIP: i32 = 0x0011;
const X86_RFLAGS: i32 = 0x0012;
const X86_EFLAGS: i32 = 0x0013;
const X86_FLAGS: i32 = 0x0014;

// MSR / system scalars.
const X86_EFER: i32 = 0x1000;
const X86_STAR: i32 = 0x1001;
const X86_LSTAR: i32 = 0x1002;
const X86_CSTAR: i32 = 0x1003;
const X86_FMASK: i32 = 0x1004;
const X86_SYSENTER_CS: i32 = 0x1006;
const X86_SYSENTER_ESP: i32 = 0x1007;
const X86_SYSENTER_EIP: i32 = 0x1008;
const X86_FS_BASE: i32 = 0x1009;
const X86_GS_BASE: i32 = 0x100A;
const X86_GDT_BASE: i32 = 0x1100;
const X86_GDT_LIMIT: i32 = 0x1101;
const X86_IDT_BASE: i32 = 0x1102;
const X86_IDT_LIMIT: i32 = 0x1103;
const X86_LDTR_SEL: i32 = 0x1104;
const X86_LDTR_BASE: i32 = 0x1105;
const X86_LDTR_LIMIT: i32 = 0x1106;
const X86_TR_SEL: i32 = 0x1107;
const X86_TR_BASE: i32 = 0x1108;
const X86_TR_LIMIT: i32 = 0x1109;

fn x86_gpr_get(r: &Registers, idx: usize) -> u64 {
    match idx {
        0 => r.rax,
        1 => r.rcx,
        2 => r.rdx,
        3 => r.rbx,
        4 => r.rsp,
        5 => r.rbp,
        6 => r.rsi,
        7 => r.rdi,
        8 => r.r8,
        9 => r.r9,
        10 => r.r10,
        11 => r.r11,
        12 => r.r12,
        13 => r.r13,
        14 => r.r14,
        15 => r.r15,
        16 => r.r16,
        17 => r.r17,
        18 => r.r18,
        19 => r.r19,
        20 => r.r20,
        21 => r.r21,
        22 => r.r22,
        23 => r.r23,
        24 => r.r24,
        25 => r.r25,
        26 => r.r26,
        27 => r.r27,
        28 => r.r28,
        29 => r.r29,
        30 => r.r30,
        31 => r.r31,
        _ => 0,
    }
}
fn x86_gpr_set(r: &mut Registers, idx: usize, v: u64) {
    match idx {
        0 => r.rax = v,
        1 => r.rcx = v,
        2 => r.rdx = v,
        3 => r.rbx = v,
        4 => r.rsp = v,
        5 => r.rbp = v,
        6 => r.rsi = v,
        7 => r.rdi = v,
        8 => r.r8 = v,
        9 => r.r9 = v,
        10 => r.r10 = v,
        11 => r.r11 = v,
        12 => r.r12 = v,
        13 => r.r13 = v,
        14 => r.r14 = v,
        15 => r.r15 = v,
        16 => r.r16 = v,
        17 => r.r17 = v,
        18 => r.r18 = v,
        19 => r.r19 = v,
        20 => r.r20 = v,
        21 => r.r21 = v,
        22 => r.r22 = v,
        23 => r.r23 = v,
        24 => r.r24 = v,
        25 => r.r25 = v,
        26 => r.r26 = v,
        27 => r.r27 = v,
        28 => r.r28 = v,
        29 => r.r29 = v,
        30 => r.r30 = v,
        31 => r.r31 = v,
        _ => {}
    }
}

fn x86_seg<'a>(s: &'a SystemRegisters, idx: usize) -> Option<&'a Segment> {
    Some(match idx {
        0 => &s.es,
        1 => &s.cs,
        2 => &s.ss,
        3 => &s.ds,
        4 => &s.fs,
        5 => &s.gs,
        _ => return None,
    })
}
fn x86_seg_mut<'a>(s: &'a mut SystemRegisters, idx: usize) -> Option<&'a mut Segment> {
    Some(match idx {
        0 => &mut s.es,
        1 => &mut s.cs,
        2 => &mut s.ss,
        3 => &mut s.ds,
        4 => &mut s.fs,
        5 => &mut s.gs,
        _ => return None,
    })
}

/// Returns the natural byte width of an x86 register id, or `None` if invalid.
fn x86_size(id: i32) -> Option<usize> {
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    Some(match id {
        X86_RIP | X86_RFLAGS => 8,
        X86_EIP | X86_EFLAGS => 4,
        X86_FLAGS => 2,
        X86_EFER | X86_STAR | X86_LSTAR | X86_CSTAR | X86_FMASK | X86_SYSENTER_CS
        | X86_SYSENTER_ESP | X86_SYSENTER_EIP | X86_FS_BASE | X86_GS_BASE | X86_GDT_BASE
        | X86_IDT_BASE | X86_LDTR_BASE | X86_TR_BASE => 8,
        X86_GDT_LIMIT | X86_IDT_LIMIT | X86_LDTR_LIMIT | X86_TR_LIMIT | X86_LDTR_SEL
        | X86_TR_SEL => match id {
            X86_GDT_LIMIT | X86_IDT_LIMIT | X86_LDTR_LIMIT | X86_TR_LIMIT => 2,
            _ => 2,
        },
        _ => match fam {
            X86_GPR64 if idx < 32 => 8,
            X86_GPR32 if idx < 32 => 4,
            X86_GPR16 if idx < 32 => 2,
            X86_GPR8L if idx < 32 => 1,
            X86_GPR8H if idx < 4 => 1,
            X86_SEG_SEL if idx < 6 => 2,
            X86_SEG_BASE if idx < 6 => 8,
            X86_SEG_LIMIT if idx < 6 => 4,
            X86_CR if idx <= 8 => 8,
            X86_DR if idx <= 7 => 8,
            X86_XMM if idx < 32 => 16,
            X86_YMM if idx < 32 => 32,
            X86_ZMM if idx < 32 => 64,
            X86_K if idx < 8 => 8,
            X86_MM if idx < 8 => 8,
            _ => return None,
        },
    })
}

/// Assembles a ZMM register (up to 64 bytes) from the split storage.
fn x86_zmm_bytes(r: &Registers, idx: usize) -> [u8; 64] {
    let mut b = [0u8; 64];
    if idx < 16 {
        b[0..8].copy_from_slice(&r.xmm[idx][0].to_le_bytes());
        b[8..16].copy_from_slice(&r.xmm[idx][1].to_le_bytes());
        b[16..24].copy_from_slice(&r.ymm_high[idx][0].to_le_bytes());
        b[24..32].copy_from_slice(&r.ymm_high[idx][1].to_le_bytes());
        for j in 0..4 {
            b[32 + j * 8..40 + j * 8].copy_from_slice(&r.zmm_high[idx][j].to_le_bytes());
        }
    } else {
        let e = idx - 16;
        for j in 0..8 {
            b[j * 8..j * 8 + 8].copy_from_slice(&r.zmm_ext[e][j].to_le_bytes());
        }
    }
    b
}

/// Writes up to 64 bytes back into the split ZMM storage (the rest preserved
/// only insofar as the supplied width covers it; callers pass full-width).
fn x86_zmm_set(r: &mut Registers, idx: usize, bytes: &[u8], width: usize) {
    let mut b = x86_zmm_bytes(r, idx);
    b[..width].copy_from_slice(&bytes[..width]);
    let rd = |s: &[u8]| {
        let mut t = [0u8; 8];
        t.copy_from_slice(s);
        u64::from_le_bytes(t)
    };
    if idx < 16 {
        r.xmm[idx][0] = rd(&b[0..8]);
        r.xmm[idx][1] = rd(&b[8..16]);
        r.ymm_high[idx][0] = rd(&b[16..24]);
        r.ymm_high[idx][1] = rd(&b[24..32]);
        for j in 0..4 {
            r.zmm_high[idx][j] = rd(&b[32 + j * 8..40 + j * 8]);
        }
    } else {
        let e = idx - 16;
        for j in 0..8 {
            r.zmm_ext[e][j] = rd(&b[j * 8..j * 8 + 8]);
        }
    }
}

fn x86_read(st: &CpuState, id: i32, out: &mut [u8]) -> Option<usize> {
    let (r, s) = match st {
        CpuState::X86_64(x) => (&x.regs, &x.sregs),
        _ => return None,
    };
    let size = x86_size(id)?;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        X86_RIP => put_uint(out, r.rip, 8),
        X86_EIP => put_uint(out, r.rip & 0xFFFF_FFFF, 4),
        X86_RFLAGS => put_uint(out, r.rflags, 8),
        X86_EFLAGS => put_uint(out, r.rflags & 0xFFFF_FFFF, 4),
        X86_FLAGS => put_uint(out, r.rflags & 0xFFFF, 2),
        X86_EFER => put_uint(out, s.efer, 8),
        X86_STAR => put_uint(out, s.star, 8),
        X86_LSTAR => put_uint(out, s.lstar, 8),
        X86_CSTAR => put_uint(out, s.cstar, 8),
        X86_FMASK => put_uint(out, s.fmask, 8),
        X86_SYSENTER_CS => put_uint(out, s.sysenter_cs, 8),
        X86_SYSENTER_ESP => put_uint(out, s.sysenter_esp, 8),
        X86_SYSENTER_EIP => put_uint(out, s.sysenter_eip, 8),
        X86_FS_BASE => put_uint(out, s.fs.base, 8),
        X86_GS_BASE => put_uint(out, s.gs.base, 8),
        X86_GDT_BASE => put_uint(out, s.gdt.base, 8),
        X86_GDT_LIMIT => put_uint(out, s.gdt.limit as u64, 2),
        X86_IDT_BASE => put_uint(out, s.idt.base, 8),
        X86_IDT_LIMIT => put_uint(out, s.idt.limit as u64, 2),
        X86_LDTR_SEL => put_uint(out, s.ldt.selector as u64, 2),
        X86_LDTR_BASE => put_uint(out, s.ldt.base, 8),
        X86_LDTR_LIMIT => put_uint(out, s.ldt.limit as u64, 2),
        X86_TR_SEL => put_uint(out, s.tr.selector as u64, 2),
        X86_TR_BASE => put_uint(out, s.tr.base, 8),
        X86_TR_LIMIT => put_uint(out, s.tr.limit as u64, 2),
        _ => match fam {
            X86_GPR64 => put_uint(out, x86_gpr_get(r, idx), 8),
            X86_GPR32 => put_uint(out, x86_gpr_get(r, idx) & 0xFFFF_FFFF, 4),
            X86_GPR16 => put_uint(out, x86_gpr_get(r, idx) & 0xFFFF, 2),
            X86_GPR8L => put_uint(out, x86_gpr_get(r, idx) & 0xFF, 1),
            X86_GPR8H => put_uint(out, (x86_gpr_get(r, idx) >> 8) & 0xFF, 1),
            X86_SEG_SEL => put_uint(out, x86_seg(s, idx)?.selector as u64, 2),
            X86_SEG_BASE => put_uint(out, x86_seg(s, idx)?.base, 8),
            X86_SEG_LIMIT => put_uint(out, x86_seg(s, idx)?.limit as u64, 4),
            X86_CR => put_uint(
                out,
                match idx {
                    0 => s.cr0,
                    2 => s.cr2,
                    3 => s.cr3,
                    4 => s.cr4,
                    8 => s.cr8,
                    _ => return None,
                },
                8,
            ),
            X86_DR => put_uint(
                out,
                match idx {
                    0 => s.dr0,
                    1 => s.dr1,
                    2 => s.dr2,
                    3 => s.dr3,
                    6 => s.dr6,
                    7 => s.dr7,
                    _ => return None,
                },
                8,
            ),
            X86_XMM => out[..16].copy_from_slice(&x86_zmm_bytes(r, idx)[..16]),
            X86_YMM => out[..32].copy_from_slice(&x86_zmm_bytes(r, idx)[..32]),
            X86_ZMM => out[..64].copy_from_slice(&x86_zmm_bytes(r, idx)[..64]),
            X86_K => put_uint(out, r.k.get(idx).copied().unwrap_or(0), 8),
            X86_MM => put_uint(out, r.mm.get(idx).copied().unwrap_or(0), 8),
            _ => return None,
        },
    }
    Some(size)
}

fn x86_write(st: &mut CpuState, id: i32, inp: &[u8]) -> Option<usize> {
    let size = x86_size(id)?;
    let (r, s) = match st {
        CpuState::X86_64(x) => (&mut x.regs, &mut x.sregs),
        _ => return None,
    };
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    let v = if size <= 8 { get_uint(inp, size) } else { 0 };
    match id {
        X86_RIP => r.rip = v,
        X86_EIP => r.rip = v & 0xFFFF_FFFF,
        X86_RFLAGS => r.rflags = v,
        X86_EFLAGS => r.rflags = (r.rflags & !0xFFFF_FFFF) | (v & 0xFFFF_FFFF),
        X86_FLAGS => r.rflags = (r.rflags & !0xFFFF) | (v & 0xFFFF),
        X86_EFER => s.efer = v,
        X86_STAR => s.star = v,
        X86_LSTAR => s.lstar = v,
        X86_CSTAR => s.cstar = v,
        X86_FMASK => s.fmask = v,
        X86_SYSENTER_CS => s.sysenter_cs = v,
        X86_SYSENTER_ESP => s.sysenter_esp = v,
        X86_SYSENTER_EIP => s.sysenter_eip = v,
        X86_FS_BASE => s.fs.base = v,
        X86_GS_BASE => s.gs.base = v,
        X86_GDT_BASE => s.gdt.base = v,
        X86_GDT_LIMIT => s.gdt.limit = v as u16,
        X86_IDT_BASE => s.idt.base = v,
        X86_IDT_LIMIT => s.idt.limit = v as u16,
        X86_LDTR_SEL => s.ldt.selector = v as u16,
        X86_LDTR_BASE => s.ldt.base = v,
        X86_LDTR_LIMIT => s.ldt.limit = v as u32,
        X86_TR_SEL => s.tr.selector = v as u16,
        X86_TR_BASE => s.tr.base = v,
        X86_TR_LIMIT => s.tr.limit = v as u32,
        _ => match fam {
            X86_GPR64 => x86_gpr_set(r, idx, v),
            X86_GPR32 => x86_gpr_set(r, idx, v & 0xFFFF_FFFF), // zero-extend
            X86_GPR16 => {
                let cur = x86_gpr_get(r, idx);
                x86_gpr_set(r, idx, (cur & !0xFFFF) | (v & 0xFFFF));
            }
            X86_GPR8L => {
                let cur = x86_gpr_get(r, idx);
                x86_gpr_set(r, idx, (cur & !0xFF) | (v & 0xFF));
            }
            X86_GPR8H => {
                let cur = x86_gpr_get(r, idx);
                x86_gpr_set(r, idx, (cur & !0xFF00) | ((v & 0xFF) << 8));
            }
            X86_SEG_SEL => x86_seg_mut(s, idx)?.selector = v as u16,
            X86_SEG_BASE => x86_seg_mut(s, idx)?.base = v,
            X86_SEG_LIMIT => x86_seg_mut(s, idx)?.limit = v as u32,
            X86_CR => match idx {
                0 => s.cr0 = v,
                2 => s.cr2 = v,
                3 => s.cr3 = v,
                4 => s.cr4 = v,
                8 => s.cr8 = v,
                _ => return None,
            },
            X86_DR => match idx {
                0 => s.dr0 = v,
                1 => s.dr1 = v,
                2 => s.dr2 = v,
                3 => s.dr3 = v,
                6 => s.dr6 = v,
                7 => s.dr7 = v,
                _ => return None,
            },
            X86_XMM => x86_zmm_set(r, idx, inp, 16),
            X86_YMM => x86_zmm_set(r, idx, inp, 32),
            X86_ZMM => x86_zmm_set(r, idx, inp, 64),
            X86_K => {
                if idx < 8 {
                    r.k[idx] = v;
                } else {
                    return None;
                }
            }
            X86_MM => {
                if idx < 8 {
                    r.mm[idx] = v;
                } else {
                    return None;
                }
            }
            _ => return None,
        },
    }
    Some(size)
}

// ===========================================================================
// AArch64 / AArch32 / Cortex-M / RISC-V / Hexagon
// ===========================================================================

const REG_GP: i32 = 0x0100; // primary GP file (X, R, x)
const REG_VEC: i32 = 0x0200; // primary vector/FP file (V, S, f)
const REG_SEC: i32 = 0x0300; // secondary file (Hexagon control registers)
const HEX_PRED: i32 = 0x0400; // Hexagon scalar predicates P0..P3
const HEX_QPRED: i32 = 0x0500; // Hexagon vector predicates Q0..Q3

// Scalar ids shared by naming convention (per-arch validity differs).
const SC_SP: i32 = 0x0010;
const SC_PC: i32 = 0x0011;
const SC_PSTATE: i32 = 0x0012; // arm64 PSTATE / arm32 CPSR / cortexm XPSR
const SC_LR: i32 = 0x0013;
const SC_SPSR: i32 = 0x0014;
const SC_FPCR: i32 = 0x0020;
const SC_FPSR: i32 = 0x0021;
const SC_FPSCR: i32 = 0x0022;
const SC_FCSR: i32 = 0x0023;
// Cortex-M specials.
const CM_MSP: i32 = 0x0030;
const CM_PSP: i32 = 0x0031;
const CM_CONTROL: i32 = 0x0032;
const CM_PRIMASK: i32 = 0x0033;
const CM_FAULTMASK: i32 = 0x0034;
const CM_BASEPRI: i32 = 0x0035;

fn arm64_size(id: i32) -> Option<usize> {
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    Some(match id {
        SC_SP | SC_PC | SC_PSTATE => 8,
        SC_FPCR | SC_FPSR => 4,
        _ => match fam {
            REG_GP if idx < 31 => 8,
            REG_VEC if idx < 32 => 16,
            _ => return None,
        },
    })
}

fn arm64_read(st: &CpuState, id: i32, out: &mut [u8]) -> Option<usize> {
    let x = st.as_aarch64()?;
    let r = &x.regs;
    let size = arm64_size(id)?;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_SP => put_uint(out, r.sp, 8),
        SC_PC => put_uint(out, r.pc, 8),
        SC_PSTATE => put_uint(out, r.pstate, 8),
        SC_FPCR => put_uint(out, r.fpcr as u64, 4),
        SC_FPSR => put_uint(out, r.fpsr as u64, 4),
        _ => match fam {
            REG_GP => put_uint(out, r.x[idx], 8),
            REG_VEC => {
                out[0..8].copy_from_slice(&r.v[idx][0].to_le_bytes());
                out[8..16].copy_from_slice(&r.v[idx][1].to_le_bytes());
            }
            _ => return None,
        },
    }
    Some(size)
}

fn arm64_write(st: &mut CpuState, id: i32, inp: &[u8]) -> Option<usize> {
    let size = arm64_size(id)?;
    let x = match st {
        CpuState::Aarch64(x) => x,
        _ => return None,
    };
    let r = &mut x.regs;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_SP => r.sp = get_uint(inp, 8),
        SC_PC => r.pc = get_uint(inp, 8),
        SC_PSTATE => r.pstate = get_uint(inp, 8),
        SC_FPCR => r.fpcr = get_uint(inp, 4) as u32,
        SC_FPSR => r.fpsr = get_uint(inp, 4) as u32,
        _ => match fam {
            REG_GP => r.x[idx] = get_uint(inp, 8),
            REG_VEC => {
                let mut lo = [0u8; 8];
                let mut hi = [0u8; 8];
                lo.copy_from_slice(&inp[0..8]);
                hi.copy_from_slice(&inp[8..16]);
                r.v[idx][0] = u64::from_le_bytes(lo);
                r.v[idx][1] = u64::from_le_bytes(hi);
            }
            _ => return None,
        },
    }
    Some(size)
}

fn arm32_size(id: i32) -> Option<usize> {
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    Some(match id {
        SC_SP | SC_LR | SC_PC | SC_PSTATE | SC_SPSR | SC_FPSCR => 4,
        _ => match fam {
            REG_GP if idx < 13 => 4,
            REG_VEC if idx < 32 => 4,
            _ => return None,
        },
    })
}

fn arm32_read(st: &CpuState, id: i32, out: &mut [u8]) -> Option<usize> {
    let x = st.as_aarch32()?;
    let r = &x.regs;
    let size = arm32_size(id)?;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_SP => put_uint(out, r.sp as u64, 4),
        SC_LR => put_uint(out, r.lr as u64, 4),
        SC_PC => put_uint(out, r.pc as u64, 4),
        SC_PSTATE => put_uint(out, r.cpsr as u64, 4),
        SC_SPSR => put_uint(out, r.spsr as u64, 4),
        SC_FPSCR => put_uint(out, r.fpscr as u64, 4),
        _ => match fam {
            REG_GP => put_uint(out, r.r[idx] as u64, 4),
            REG_VEC => put_uint(out, r.s[idx] as u64, 4),
            _ => return None,
        },
    }
    Some(size)
}

fn arm32_write(st: &mut CpuState, id: i32, inp: &[u8]) -> Option<usize> {
    let size = arm32_size(id)?;
    let x = match st {
        CpuState::Aarch32(x) => x,
        _ => return None,
    };
    let r = &mut x.regs;
    let v = get_uint(inp, 4) as u32;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_SP => r.sp = v,
        SC_LR => r.lr = v,
        SC_PC => r.pc = v,
        SC_PSTATE => r.cpsr = v,
        SC_SPSR => r.spsr = v,
        SC_FPSCR => r.fpscr = v,
        _ => match fam {
            REG_GP => r.r[idx] = v,
            REG_VEC => r.s[idx] = v,
            _ => return None,
        },
    }
    Some(size)
}

fn cortexm_size(id: i32) -> Option<usize> {
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    Some(match id {
        SC_LR | SC_PC | SC_PSTATE | SC_FPSCR | CM_MSP | CM_PSP | CM_CONTROL | CM_PRIMASK
        | CM_FAULTMASK | CM_BASEPRI => 4,
        _ => match fam {
            REG_GP if idx < 13 => 4,
            REG_VEC if idx < 32 => 4,
            _ => return None,
        },
    })
}

fn cortexm_read(st: &CpuState, id: i32, out: &mut [u8]) -> Option<usize> {
    let x = st.as_cortex_m()?;
    let r = &x.regs;
    let size = cortexm_size(id)?;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_LR => put_uint(out, r.lr as u64, 4),
        SC_PC => put_uint(out, r.pc as u64, 4),
        SC_PSTATE => put_uint(out, r.xpsr as u64, 4),
        SC_FPSCR => put_uint(out, r.fpscr as u64, 4),
        CM_MSP => put_uint(out, r.msp as u64, 4),
        CM_PSP => put_uint(out, r.psp as u64, 4),
        CM_CONTROL => put_uint(out, r.control as u64, 4),
        CM_PRIMASK => put_uint(out, r.primask as u64, 4),
        CM_FAULTMASK => put_uint(out, r.faultmask as u64, 4),
        CM_BASEPRI => put_uint(out, r.basepri as u64, 4),
        _ => match fam {
            REG_GP => put_uint(out, r.r[idx] as u64, 4),
            REG_VEC => put_uint(out, r.s[idx] as u64, 4),
            _ => return None,
        },
    }
    Some(size)
}

fn cortexm_write(st: &mut CpuState, id: i32, inp: &[u8]) -> Option<usize> {
    let size = cortexm_size(id)?;
    let x = match st {
        CpuState::CortexM(x) => x,
        _ => return None,
    };
    let r = &mut x.regs;
    let v = get_uint(inp, 4) as u32;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_LR => r.lr = v,
        SC_PC => r.pc = v,
        SC_PSTATE => r.xpsr = v,
        SC_FPSCR => r.fpscr = v,
        CM_MSP => r.msp = v,
        CM_PSP => r.psp = v,
        CM_CONTROL => r.control = v,
        CM_PRIMASK => r.primask = v,
        CM_FAULTMASK => r.faultmask = v,
        CM_BASEPRI => r.basepri = v,
        _ => match fam {
            REG_GP => r.r[idx] = v,
            REG_VEC => r.s[idx] = v,
            _ => return None,
        },
    }
    Some(size)
}

fn riscv_size(id: i32) -> Option<usize> {
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    Some(match id {
        SC_PC => 8,
        SC_FCSR => 4,
        _ => match fam {
            REG_GP if idx < 32 => 8,
            REG_VEC if idx < 32 => 8,
            _ => return None,
        },
    })
}

fn riscv_read(st: &CpuState, id: i32, out: &mut [u8]) -> Option<usize> {
    let x = st.as_riscv()?;
    let r = &x.regs;
    let size = riscv_size(id)?;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_PC => put_uint(out, r.pc, 8),
        SC_FCSR => put_uint(out, r.fcsr as u64, 4),
        _ => match fam {
            REG_GP => put_uint(out, r.x[idx], 8),
            REG_VEC => put_uint(out, r.f[idx], 8),
            _ => return None,
        },
    }
    Some(size)
}

fn riscv_write(st: &mut CpuState, id: i32, inp: &[u8]) -> Option<usize> {
    let size = riscv_size(id)?;
    let x = match st {
        CpuState::RiscV(x) => x,
        _ => return None,
    };
    let r = &mut x.regs;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match id {
        SC_PC => r.pc = get_uint(inp, 8),
        SC_FCSR => r.fcsr = get_uint(inp, 4) as u32,
        _ => match fam {
            // x0 is hardwired zero; ignore writes to keep architectural truth.
            REG_GP => {
                if idx != 0 {
                    r.x[idx] = get_uint(inp, 8);
                }
            }
            REG_VEC => r.f[idx] = get_uint(inp, 8),
            _ => return None,
        },
    }
    Some(size)
}

fn hexagon_size(id: i32) -> Option<usize> {
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    Some(match fam {
        REG_GP if idx < 32 => 4,
        REG_VEC if idx < 32 => 128,
        REG_SEC if idx < 32 => 4, // control regs (== HEX_PRED base; distinguished below)
        HEX_PRED if idx < 4 => 1,
        HEX_QPRED if idx < 4 => 16,
        _ => return None,
    })
}

fn hexagon_read(st: &CpuState, id: i32, out: &mut [u8]) -> Option<usize> {
    let r = match st {
        CpuState::Hexagon(h) => &h.regs,
        _ => return None,
    };
    let size = hexagon_size(id)?;
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    match fam {
        REG_GP => put_uint(out, r.r[idx] as u64, 4),
        REG_VEC => {
            for j in 0..32 {
                out[j * 4..j * 4 + 4].copy_from_slice(&r.v[idx][j].to_le_bytes());
            }
        }
        REG_SEC => put_uint(out, r.control(idx) as u64, 4),
        HEX_PRED => out[0] = r.p[idx],
        HEX_QPRED => {
            for j in 0..4 {
                out[j * 4..j * 4 + 4].copy_from_slice(&r.q[idx][j].to_le_bytes());
            }
        }
        _ => return None,
    }
    Some(size)
}

fn hexagon_write(st: &mut CpuState, id: i32, inp: &[u8]) -> Option<usize> {
    let size = hexagon_size(id)?;
    let r = match st {
        CpuState::Hexagon(h) => &mut h.regs,
        _ => return None,
    };
    let fam = id & !0xFF;
    let idx = (id & 0xFF) as usize;
    let rd = |s: &[u8]| {
        let mut t = [0u8; 4];
        t.copy_from_slice(s);
        u32::from_le_bytes(t)
    };
    match fam {
        REG_GP => r.r[idx] = get_uint(inp, 4) as u32,
        REG_VEC => {
            for j in 0..32 {
                r.v[idx][j] = rd(&inp[j * 4..j * 4 + 4]);
            }
        }
        REG_SEC => r.set_control(idx, get_uint(inp, 4) as u32),
        HEX_PRED => r.set_predicate(idx, inp[0]),
        HEX_QPRED => {
            for j in 0..4 {
                r.q[idx][j] = rd(&inp[j * 4..j * 4 + 4]);
            }
        }
        _ => return None,
    }
    Some(size)
}

// --- dispatch --------------------------------------------------------------

/// Sets the program counter field of a [`CpuState`] in place. Used by the run
/// loop to honour `emu_start(begin=...)`.
pub(crate) fn set_state_pc(st: &mut CpuState, pc: u64) {
    match st {
        CpuState::X86_64(x) => x.regs.rip = pc,
        CpuState::Aarch64(x) => x.regs.pc = pc,
        CpuState::Aarch32(x) => x.regs.pc = pc as u32,
        CpuState::CortexM(x) => x.regs.pc = pc as u32,
        CpuState::RiscV(x) => x.regs.pc = pc,
        CpuState::Hexagon(x) => x.regs.set_pc(pc as u32),
    }
}

fn reg_size_for(arch: RaxArch, id: i32) -> Option<usize> {
    match arch {
        RaxArch::X86 => x86_size(id),
        RaxArch::Arm64 => arm64_size(id),
        RaxArch::Arm => arm32_size(id),
        RaxArch::CortexM => cortexm_size(id),
        RaxArch::Riscv64 => riscv_size(id),
        RaxArch::Hexagon => hexagon_size(id),
    }
}

// ===========================================================================
// FFI
// ===========================================================================

/// Returns the byte width of register `regid` for `arch`, or 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn rax_reg_size(arch: c_int, regid: c_int) -> usize {
    crate::guard_val(0, || match RaxArch::from_i32(arch) {
        Some(a) => reg_size_for(a, regid).unwrap_or(0),
        None => 0,
    })
}

/// Reads register `regid` into `value` (caller buffer of at least
/// `rax_reg_size(arch, regid)` bytes). The number of bytes written is stored in
/// `*out_size` if non-NULL.
#[unsafe(no_mangle)]
pub extern "C" fn rax_reg_read(
    engine: *mut Engine,
    regid: c_int,
    value: *mut u8,
    out_size: *mut usize,
) -> RaxStatus {
    guard(|| {
        let e = match unsafe { engine_mut(engine) } {
            Some(e) => e,
            None => return RaxStatus::Handle,
        };
        e.clear_err();
        if value.is_null() {
            return e.fail(RaxStatus::Arg, "null value buffer");
        }
        let st = match e.vcpu.get_state() {
            Ok(s) => s,
            Err(err) => return e.fail_engine(&err),
        };
        let mut tmp = [0u8; MAX_REG_BYTES];
        let res = match e.arch {
            RaxArch::X86 => x86_read(&st, regid, &mut tmp),
            RaxArch::Arm64 => arm64_read(&st, regid, &mut tmp),
            RaxArch::Arm => arm32_read(&st, regid, &mut tmp),
            RaxArch::CortexM => cortexm_read(&st, regid, &mut tmp),
            RaxArch::Riscv64 => riscv_read(&st, regid, &mut tmp),
            RaxArch::Hexagon => hexagon_read(&st, regid, &mut tmp),
        };
        match res {
            Some(n) => {
                unsafe {
                    std::ptr::copy_nonoverlapping(tmp.as_ptr(), value, n);
                    if !out_size.is_null() {
                        *out_size = n;
                    }
                }
                RaxStatus::Ok
            }
            None => e.fail(RaxStatus::Reg, "invalid register id for architecture"),
        }
    })
}

/// Writes register `regid` from `value` (caller buffer of at least
/// `rax_reg_size(arch, regid)` bytes).
#[unsafe(no_mangle)]
pub extern "C" fn rax_reg_write(engine: *mut Engine, regid: c_int, value: *const u8) -> RaxStatus {
    guard(|| {
        let e = match unsafe { engine_mut(engine) } {
            Some(e) => e,
            None => return RaxStatus::Handle,
        };
        e.clear_err();
        if value.is_null() {
            return e.fail(RaxStatus::Arg, "null value buffer");
        }
        let size = match reg_size_for(e.arch, regid) {
            Some(s) => s,
            None => return e.fail(RaxStatus::Reg, "invalid register id for architecture"),
        };
        let inp = unsafe { std::slice::from_raw_parts(value, size) };
        let mut st = match e.vcpu.get_state() {
            Ok(s) => s,
            Err(err) => return e.fail_engine(&err),
        };
        let res = match e.arch {
            RaxArch::X86 => x86_write(&mut st, regid, inp),
            RaxArch::Arm64 => arm64_write(&mut st, regid, inp),
            RaxArch::Arm => arm32_write(&mut st, regid, inp),
            RaxArch::CortexM => cortexm_write(&mut st, regid, inp),
            RaxArch::Riscv64 => riscv_write(&mut st, regid, inp),
            RaxArch::Hexagon => hexagon_write(&mut st, regid, inp),
        };
        if res.is_none() {
            return e.fail(RaxStatus::Reg, "invalid register id for architecture");
        }
        match e.vcpu.set_state(&st) {
            Ok(()) => RaxStatus::Ok,
            Err(err) => e.fail_engine(&err),
        }
    })
}

/// Convenience: reads an integer register (width <= 8) as a `uint64_t`.
#[unsafe(no_mangle)]
pub extern "C" fn rax_reg_read_u64(
    engine: *mut Engine,
    regid: c_int,
    value: *mut u64,
) -> RaxStatus {
    guard(|| {
        let e = match unsafe { engine_ref(engine) } {
            Some(_) => {}
            None => return RaxStatus::Handle,
        };
        let _ = e;
        if value.is_null() {
            return RaxStatus::Arg;
        }
        let mut buf = [0u8; 8];
        let mut n: usize = 0;
        let st = rax_reg_read(engine, regid, buf.as_mut_ptr(), &mut n);
        if st != RaxStatus::Ok {
            return st;
        }
        if n > 8 {
            return RaxStatus::Arg;
        }
        unsafe {
            *value = u64::from_le_bytes(buf);
        }
        RaxStatus::Ok
    })
}

/// Convenience: writes an integer register (width <= 8) from a `uint64_t`.
#[unsafe(no_mangle)]
pub extern "C" fn rax_reg_write_u64(engine: *mut Engine, regid: c_int, value: u64) -> RaxStatus {
    guard(|| {
        let arch = match unsafe { engine_ref(engine) } {
            Some(e) => e.arch,
            None => return RaxStatus::Handle,
        };
        let size = match reg_size_for(arch, regid) {
            Some(s) if s <= 8 => s,
            Some(_) => return RaxStatus::Arg,
            None => return RaxStatus::Reg,
        };
        let mut buf = [0u8; 8];
        buf[..size].copy_from_slice(&value.to_le_bytes()[..size]);
        rax_reg_write(engine, regid, buf.as_ptr())
    })
}
