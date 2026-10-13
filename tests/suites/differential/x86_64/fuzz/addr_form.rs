//! Memory-operand encodings shared by the generators that aim an operand at a
//! chosen address.

use super::{CODE_ADDR, Rng, modrm, set_reg};
use rax::vm::vcpu::Registers;

/// Base register (ModR/M.rm or SIB.base) of every form that has one: rbx.
pub(super) const BASE: u8 = 3;
/// Index register of every SIB form that has one: rsi.
pub(super) const INDEX: u8 = 6;

#[derive(Clone, Copy)]
pub(super) enum AddrForm {
    Base,       // mod=00 rm=base          [rbx]
    BaseDisp8,  // mod=01 rm=base          [rbx+d8]
    BaseDisp32, // mod=10 rm=base          [rbx+d32]
    Sib,        // mod=00 rm=4 SIB         [rbx + rsi*s]
    SibDisp8,   // mod=01 rm=4 SIB + d8    [rbx + rsi*s + d8]
    SibDisp32,  // mod=10 rm=4 SIB + d32   [rbx + rsi*s + d32]
    SibNoIndex, // mod=00 rm=4 SIB idx=4   [rbx]            (no-index encoding)
    SibNoBase,  // mod=00 rm=4 SIB base=5  [rsi*s + disp32] (no base)
    RipRel,     // mod=00 rm=5             [rip + disp32]
}

pub(super) const ADDR_FORMS: [AddrForm; 9] = [
    AddrForm::Base,
    AddrForm::BaseDisp8,
    AddrForm::BaseDisp32,
    AddrForm::Sib,
    AddrForm::SibDisp8,
    AddrForm::SibDisp32,
    AddrForm::SibNoIndex,
    AddrForm::SibNoBase,
    AddrForm::RipRel,
];

/// Appends the ModR/M byte (with `reg` in ModR/M.reg) and any SIB byte and
/// displacement of a `form` operand whose offset is `target`, and sets
/// rbx/rsi in `r` to reach it. `scale_bits` and `idxv` are the SIB scale and
/// index value; displacements are drawn from `rng`.
///
/// `imm_len` is the length of the immediate that the caller appends after
/// the operand: a RIP-relative displacement is relative to the end of the
/// whole instruction (Intel SDM Vol. 2A section 2.2.1.6).
#[allow(clippy::too_many_arguments)]
pub(super) fn encode_ea_operand(
    rng: &mut Rng,
    form: AddrForm,
    reg: u8,
    target: u64,
    scale_bits: u8,
    idxv: u64,
    imm_len: usize,
    code: &mut Vec<u8>,
    r: &mut Registers,
) {
    let scale = 1u64 << scale_bits;
    match form {
        AddrForm::Base => {
            code.push(modrm(0b00, reg, BASE));
            set_reg(r, BASE, target);
        }
        AddrForm::BaseDisp8 => {
            let d8 = (rng.next_u32() as i8) as i64;
            code.push(modrm(0b01, reg, BASE));
            set_reg(r, BASE, target.wrapping_sub(d8 as u64));
            code.push(d8 as u8);
        }
        AddrForm::BaseDisp32 => {
            let d32 = rng.next_u32() as i32 as i64;
            code.push(modrm(0b10, reg, BASE));
            set_reg(r, BASE, target.wrapping_sub(d32 as u64));
            code.extend_from_slice(&(d32 as i32).to_le_bytes());
        }
        AddrForm::Sib => {
            code.push(modrm(0b00, reg, 4));
            code.push((scale_bits << 6) | (INDEX << 3) | BASE);
            set_reg(r, INDEX, idxv);
            set_reg(r, BASE, target.wrapping_sub(idxv.wrapping_mul(scale)));
        }
        AddrForm::SibDisp8 => {
            let d8 = (rng.next_u32() as i8) as i64;
            code.push(modrm(0b01, reg, 4));
            code.push((scale_bits << 6) | (INDEX << 3) | BASE);
            set_reg(r, INDEX, idxv);
            set_reg(
                r,
                BASE,
                target
                    .wrapping_sub(idxv.wrapping_mul(scale))
                    .wrapping_sub(d8 as u64),
            );
            code.push(d8 as u8);
        }
        AddrForm::SibDisp32 => {
            let d32 = rng.next_u32() as i32 as i64;
            code.push(modrm(0b10, reg, 4));
            code.push((scale_bits << 6) | (INDEX << 3) | BASE);
            set_reg(r, INDEX, idxv);
            set_reg(
                r,
                BASE,
                target
                    .wrapping_sub(idxv.wrapping_mul(scale))
                    .wrapping_sub(d32 as u64),
            );
            code.extend_from_slice(&(d32 as i32).to_le_bytes());
        }
        AddrForm::SibNoIndex => {
            // index=4 in SIB encodes "no index". EA = base.
            code.push(modrm(0b00, reg, 4));
            code.push((scale_bits << 6) | (4 << 3) | BASE);
            set_reg(r, BASE, target);
        }
        AddrForm::SibNoBase => {
            // mod=00, SIB base=5 => no base register, disp32 follows.
            // EA = index*scale + disp32.
            code.push(modrm(0b00, reg, 4));
            code.push((scale_bits << 6) | (INDEX << 3) | 5);
            set_reg(r, INDEX, idxv);
            let disp = target.wrapping_sub(idxv.wrapping_mul(scale)) as i64 as i32;
            code.extend_from_slice(&disp.to_le_bytes());
        }
        AddrForm::RipRel => {
            // mod=00, rm=5 => [rip + disp32], rip = address of NEXT insn.
            code.push(modrm(0b00, reg, 5));
            let rip_after = CODE_ADDR + code.len() as u64 + 4 + imm_len as u64;
            let disp = target.wrapping_sub(rip_after) as i64 as i32;
            code.extend_from_slice(&disp.to_le_bytes());
        }
    }
}
