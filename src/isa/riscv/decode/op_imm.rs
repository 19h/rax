//! OP-IMM and OP-IMM-32 decoding.

use super::*;

pub(super) fn decode_op_imm(w: u32, rv64: bool, isa: &Isa) -> Insn {
    match funct3(w) {
        0 => with_imm(Op::Addi, w, imm_i(w)),
        2 => with_imm(Op::Slti, w, imm_i(w)),
        3 => with_imm(Op::Sltiu, w, imm_i(w)),
        4 => with_imm(Op::Xori, w, imm_i(w)),
        6 if isa.zicbop && rd(w) == 0 => {
            let op = match rs2(w) {
                0 => Op::PrefetchI,
                1 => Op::PrefetchR,
                3 => Op::PrefetchW,
                _ => Op::Ori,
            };
            if matches!(op, Op::Ori) {
                with_imm(op, w, imm_i(w))
            } else {
                with_imm(op, w, imm_i(w) & !0x1f)
            }
        }
        6 => with_imm(Op::Ori, w, imm_i(w)),
        7 => with_imm(Op::Andi, w, imm_i(w)),
        1 => decode_shift_left_imm(w, rv64, isa),
        5 => decode_shift_right_imm(w, rv64, isa),
        _ => Insn::illegal(w, 4),
    }
}

// OP-IMM funct3==1 (SLLI and Zbb/Zbs left-shift-immediate overlays).
fn decode_shift_left_imm(w: u32, rv64: bool, isa: &Isa) -> Insn {
    if !rv64 && w & (1 << 25) != 0 {
        return Insn::illegal(w, 4);
    }

    let funct6 = (w >> 26) & 0x3f;
    let funct7 = funct7(w);
    let shamt = ((w >> 20) & if rv64 { 0x3f } else { 0x1f }) as i64;
    let rs2f = rs2(w);
    // SHA / SM3 message-schedule transforms (funct7 = 0b0001000).
    if funct7 == 0b0001000 {
        match rs2f {
            0b00000 if isa.zknh => return base(Op::Sha256Sum0, w),
            0b00001 if isa.zknh => return base(Op::Sha256Sum1, w),
            0b00010 if isa.zknh => return base(Op::Sha256Sig0, w),
            0b00011 if isa.zknh => return base(Op::Sha256Sig1, w),
            0b00100 if rv64 && isa.zknh => return base(Op::Sha512Sum0, w),
            0b00101 if rv64 && isa.zknh => return base(Op::Sha512Sum1, w),
            0b00110 if rv64 && isa.zknh => return base(Op::Sha512Sig0, w),
            0b00111 if rv64 && isa.zknh => return base(Op::Sha512Sig1, w),
            0b01000 if isa.zksh => return base(Op::Sm3p0, w),
            0b01001 if isa.zksh => return base(Op::Sm3p1, w),
            _ => {}
        }
    }
    // AES-64 decrypt InvMixColumns / key-schedule step 1 (funct7 = 0b0011000).
    if rv64 && funct7 == 0b0011000 {
        if rs2f == 0 && isa.zknd {
            return base(Op::Aes64im, w);
        }
        if rs2f & 0b10000 != 0 && (isa.zkne || isa.zknd) {
            // aes64ks1i: rnum in rs2[3:0], must be <= 0xA.
            if (rs2f & 0xf) <= 0xA {
                return base(Op::Aes64ks1i, w);
            }
        }
    }
    // CLZ/CTZ/CPOP/SEXT.B/SEXT.H share funct7=0b0110000.
    if isa.zbb && funct7 == 0b0110000 {
        let op = match rs2f {
            0b00000 => Op::Clz,
            0b00001 => Op::Ctz,
            0b00010 => Op::Cpop,
            0b00100 => Op::SextB,
            0b00101 => Op::SextH,
            _ => return Insn::illegal(w, 4),
        };
        return base(op, w);
    }
    // Zbkb zip: RV32-only, funct7=0b0000100, shamt/rs2 field = 15.
    if isa.zbkb && !rv64 && funct7 == 0b0000100 && rs2f == 0b01111 {
        return base(Op::Zip, w);
    }
    if isa.zbs {
        match funct6 {
            0b010010 => return with_imm(Op::Bclri, w, shamt),
            0b011010 => return with_imm(Op::Binvi, w, shamt),
            0b001010 => return with_imm(Op::Bseti, w, shamt),
            _ => {}
        }
    }
    // SLLI: funct6 must be zero (RV64) / funct7 zero (RV32).
    if (rv64 && funct6 == 0) || (!rv64 && funct7 == 0) {
        return with_imm(Op::Slli, w, shamt);
    }
    Insn::illegal(w, 4)
}

// OP-IMM funct3==5 (SRLI/SRAI and Zbb/Zbs right-shift-immediate overlays).
fn decode_shift_right_imm(w: u32, rv64: bool, isa: &Isa) -> Insn {
    if !rv64 && w & (1 << 25) != 0 {
        return Insn::illegal(w, 4);
    }

    let funct6 = (w >> 26) & 0x3f;
    let funct7 = funct7(w);
    let rs2f = rs2(w);
    let shamt = ((w >> 20) & if rv64 { 0x3f } else { 0x1f }) as i64;
    if isa.zbb {
        // ORC.B: funct7=0b0010100, rs2=0b00111.
        if funct7 == 0b0010100 && rs2f == 0b00111 {
            return base(Op::Orcb, w);
        }
        // REV8: RV64 funct12=0b011010111000, RV32 funct12=0b011010011000.
        let funct12 = (w >> 20) & 0xfff;
        if (rv64 && funct12 == 0b0110_1011_1000) || (!rv64 && funct12 == 0b0110_1001_1000) {
            return base(Op::Rev8, w);
        }
    }
    // Zbkb brev8: funct7=0b0110100, rs2=0b00111, funct3=5.
    if isa.zbkb && funct7 == 0b0110100 && rs2f == 0b00111 {
        return base(Op::Brev8, w);
    }
    // Zbkb unzip: RV32-only, funct7=0b0000100, shamt/rs2 field = 15.
    if isa.zbkb && !rv64 && funct7 == 0b0000100 && rs2f == 0b01111 {
        return base(Op::Unzip, w);
    }
    if isa.zbb && funct6 == 0b011000 {
        return with_imm(Op::Rori, w, shamt);
    }
    if isa.zbs && funct6 == 0b010010 {
        return with_imm(Op::Bexti, w, shamt);
    }
    match funct6 {
        0b000000 => with_imm(Op::Srli, w, shamt),
        0b010000 => with_imm(Op::Srai, w, shamt),
        _ if !rv64 && funct7 == 0b0000000 => with_imm(Op::Srli, w, shamt),
        _ if !rv64 && funct7 == 0b0100000 => with_imm(Op::Srai, w, shamt),
        _ => Insn::illegal(w, 4),
    }
}

// OP-IMM-32 (RV64 word immediate ops + Zba/Zbb word overlays).
pub(super) fn decode_op_imm32(w: u32, isa: &Isa) -> Insn {
    let funct7 = funct7(w);
    let funct6 = (w >> 26) & 0x3f;
    let rs2f = rs2(w);
    let shamt5 = ((w >> 20) & 0x1f) as i64;
    let shamt6 = ((w >> 20) & 0x3f) as i64;
    match funct3(w) {
        0 => with_imm(Op::Addiw, w, imm_i(w)),
        1 => {
            if isa.zba && funct6 == 0b000010 {
                return with_imm(Op::SlliUw, w, shamt6);
            }
            if isa.zbb && funct7 == 0b0110000 {
                let op = match rs2f {
                    0b00000 => Op::Clzw,
                    0b00001 => Op::Ctzw,
                    0b00010 => Op::Cpopw,
                    _ => return Insn::illegal(w, 4),
                };
                return base(op, w);
            }
            if funct7 == 0 {
                return with_imm(Op::Slliw, w, shamt5);
            }
            Insn::illegal(w, 4)
        }
        5 => {
            if isa.zbb && funct7 == 0b0110000 {
                return with_imm(Op::Roriw, w, shamt5);
            }
            match funct7 {
                0b0000000 => with_imm(Op::Srliw, w, shamt5),
                0b0100000 => with_imm(Op::Sraiw, w, shamt5),
                _ => Insn::illegal(w, 4),
            }
        }
        _ => Insn::illegal(w, 4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shift_imm(funct6: u32, shamt: u32, funct3: u32) -> u32 {
        (funct6 << 26) | (shamt << 20) | (2 << 15) | (funct3 << 12) | (1 << 7) | 0x13
    }

    #[test]
    fn rv32_rejects_shamt_bit_five_for_every_shift_immediate_family() {
        let isa = Isa::rv64gc();
        let forms = [
            (Op::Slli, 0b000000, 0b001),
            (Op::Bseti, 0b001010, 0b001),
            (Op::Bclri, 0b010010, 0b001),
            (Op::Binvi, 0b011010, 0b001),
            (Op::Srli, 0b000000, 0b101),
            (Op::Srai, 0b010000, 0b101),
            (Op::Rori, 0b011000, 0b101),
            (Op::Bexti, 0b010010, 0b101),
        ];

        for (expected, funct6, funct3) in forms {
            let reserved_rv32 = shift_imm(funct6, 0b10_0000, funct3);
            assert_eq!(
                decode(reserved_rv32, Xlen::Rv32, &isa).op,
                Op::Illegal,
                "RV32 accepted bit 25 for {expected:?}"
            );

            let legal_rv32 = shift_imm(funct6, 0b01_1111, funct3);
            let decoded_rv32 = decode(legal_rv32, Xlen::Rv32, &isa);
            assert_eq!(decoded_rv32.op, expected);
            assert_eq!(decoded_rv32.imm, 31);

            let legal_rv64 = decode(reserved_rv32, Xlen::Rv64, &isa);
            assert_eq!(legal_rv64.op, expected);
            assert_eq!(legal_rv64.imm, 32);
        }
    }
}
