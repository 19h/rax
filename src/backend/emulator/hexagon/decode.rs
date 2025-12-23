#[derive(Clone, Copy, Debug)]
pub enum MemWidth {
    Byte,
    Half,
    Word,
    Double,
}

#[derive(Clone, Copy, Debug)]
pub enum MemSign {
    Signed,
    Unsigned,
}

#[derive(Clone, Copy, Debug)]
pub enum AddrMode {
    Offset { base: u8, offset: i32 },
    PostIncImm { base: u8, offset: i32 },
    GpOffset { offset: i32 },
    Abs { addr: u32 },
}

#[derive(Clone, Copy, Debug)]
pub enum ShiftKind {
    Lsl,
    Lsr,
    Asr,
}

#[derive(Clone, Copy, Debug)]
pub enum CmpKind {
    Eq,
    Gt,
    Gtu,
    Ne,
    Lte,
    Lteu,
}

#[derive(Clone, Copy, Debug)]
pub enum DecodedInsn {
    ImmExt { value: u32 },
    Add { dst: u8, src1: u8, src2: u8 },
    Sub { dst: u8, src1: u8, src2: u8 },
    And { dst: u8, src1: u8, src2: u8 },
    Or { dst: u8, src1: u8, src2: u8 },
    Xor { dst: u8, src1: u8, src2: u8 },
    AddImm { dst: u8, src: u8, imm: i32 },
    Mov { dst: u8, src: u8 },
    MovImm { dst: u8, imm: i32 },
    Load {
        dst: u8,
        addr: AddrMode,
        width: MemWidth,
        sign: MemSign,
    },
    Store {
        src: u8,
        addr: AddrMode,
        width: MemWidth,
    },
    Jump { offset: i32 },
    JumpCond {
        offset: i32,
        pred: u8,
        sense: bool,
        pred_new: bool,
    },
    JumpReg { src: u8 },
    JumpRegCond {
        src: u8,
        pred: u8,
        sense: bool,
        pred_new: bool,
    },
    Call { offset: i32 },
    CallReg { src: u8 },
    Cmp {
        pred: u8,
        src1: u8,
        src2: u8,
        kind: CmpKind,
    },
    CmpImm {
        pred: u8,
        src: u8,
        imm: i32,
        kind: CmpKind,
        unsigned: bool,
    },
    Mul { dst: u8, src1: u8, src2: u8 },
    ShiftImm {
        dst: u8,
        src: u8,
        kind: ShiftKind,
        amount: u8,
    },
    ShiftReg {
        dst: u8,
        src: u8,
        amt: u8,
        kind: ShiftKind,
    },
    TfrCrR { dst: u8, src: u8 },
    TfrRrCr { dst: u8, src: u8 },
    LoopStartReg {
        loop_id: u8,
        start_offset: i32,
        count_reg: u8,
    },
    LoopStartImm {
        loop_id: u8,
        start_offset: i32,
        count: u32,
    },
    Trap0,
    Unknown(u32),
}

pub struct DecodedWord {
    pub insn: DecodedInsn,
    pub used_ext: bool,
}

fn bits(word: u32, hi: u8, lo: u8) -> u32 {
    (word >> lo) & ((1u32 << (hi - lo + 1)) - 1)
}

fn gather_bits(word: u32, positions: &[u8]) -> u32 {
    let mut value = 0u32;
    for &bit in positions {
        value <<= 1;
        value |= (word >> bit) & 1;
    }
    value
}

fn sign_extend(value: u32, bits: u8) -> i32 {
    let shift = 32u8.saturating_sub(bits);
    ((value << shift) as i32) >> shift
}

fn apply_immext(imm: u32, immext: u32) -> u32 {
    let ext = immext & 0x03ff_ffff;
    (ext << 6) | (imm & 0x3f)
}

fn decode_simm(word: u32, positions: &[u8], bits: u8, immext: Option<u32>) -> (i32, bool) {
    let imm = gather_bits(word, positions);
    if let Some(ext) = immext {
        return (apply_immext(imm, ext) as i32, true);
    }
    (sign_extend(imm, bits), false)
}

fn decode_uimm(word: u32, positions: &[u8], immext: Option<u32>) -> (u32, bool) {
    let imm = gather_bits(word, positions);
    if let Some(ext) = immext {
        return (apply_immext(imm, ext), true);
    }
    (imm, false)
}

fn decode_ld_width(op: u32) -> Option<(MemWidth, MemSign)> {
    match op {
        0b1000 => Some((MemWidth::Byte, MemSign::Signed)),
        0b1001 => Some((MemWidth::Byte, MemSign::Unsigned)),
        0b1010 => Some((MemWidth::Half, MemSign::Signed)),
        0b1011 => Some((MemWidth::Half, MemSign::Unsigned)),
        0b1100 => Some((MemWidth::Word, MemSign::Unsigned)),
        0b1110 => Some((MemWidth::Double, MemSign::Unsigned)),
        _ => None,
    }
}

fn decode_st_width(op: u32) -> Option<MemWidth> {
    match op {
        0b1000 => Some(MemWidth::Byte),
        0b1010 => Some(MemWidth::Half),
        0b1100 => Some(MemWidth::Word),
        0b1110 => Some(MemWidth::Double),
        _ => None,
    }
}

fn width_shift(width: MemWidth) -> u8 {
    match width {
        MemWidth::Byte => 0,
        MemWidth::Half => 1,
        MemWidth::Word => 2,
        MemWidth::Double => 3,
    }
}

pub fn decode(word: u32, immext: Option<u32>) -> DecodedWord {
    let iclass = bits(word, 31, 28);

    if iclass == 0x0 {
        let high = bits(word, 27, 16);
        let low = bits(word, 13, 0);
        let value = (high << 14) | low;
        return DecodedWord {
            insn: DecodedInsn::ImmExt { value },
            used_ext: false,
        };
    }

    if iclass == 0xf {
        let maj4 = bits(word, 27, 24);
        let min3 = bits(word, 23, 21);
        let s = bits(word, 20, 16) as u8;
        let t = bits(word, 12, 8) as u8;
        let d = bits(word, 4, 0) as u8;

        if maj4 == 0b0010 {
            let dst_hi = bits(word, 4, 2);
            if dst_hi == 0 {
                let pred = bits(word, 1, 0) as u8;
                let kind = match min3 & 0b011 {
                    0b00 => CmpKind::Eq,
                    0b10 => CmpKind::Gt,
                    0b11 => CmpKind::Gtu,
                    _ => {
                        return DecodedWord {
                            insn: DecodedInsn::Unknown(word),
                            used_ext: false,
                        }
                    }
                };
                return DecodedWord {
                    insn: DecodedInsn::Cmp {
                        pred,
                        src1: s,
                        src2: t,
                        kind,
                    },
                    used_ext: false,
                };
            }
        }

        let insn = match (maj4, min3) {
            (0b0011, 0b000) => DecodedInsn::Add {
                dst: d,
                src1: s,
                src2: t,
            },
            (0b0011, 0b001) => DecodedInsn::Sub {
                dst: d,
                src1: s,
                src2: t,
            },
            (0b0001, 0b000) => DecodedInsn::And {
                dst: d,
                src1: s,
                src2: t,
            },
            (0b0001, 0b001) => DecodedInsn::Or {
                dst: d,
                src1: s,
                src2: t,
            },
            (0b0001, 0b011) => DecodedInsn::Xor {
                dst: d,
                src1: s,
                src2: t,
            },
            _ => DecodedInsn::Unknown(word),
        };

        return DecodedWord {
            insn,
            used_ext: false,
        };
    }

    if iclass == 0xb {
        let s = bits(word, 20, 16) as u8;
        let d = bits(word, 4, 0) as u8;
        let imm_positions = [
            27, 26, 25, 24, 23, 22, 21, 13, 12, 11, 10, 9, 8, 7, 6, 5,
        ];
        let (imm, used) = decode_simm(word, &imm_positions, 16, immext);
        return DecodedWord {
            insn: DecodedInsn::AddImm { dst: d, src: s, imm },
            used_ext: used,
        };
    }

    if iclass == 0x7 {
        let maj4 = bits(word, 27, 24);
        let min3 = bits(word, 23, 21);
        let s = bits(word, 20, 16) as u8;
        let d = bits(word, 4, 0) as u8;
        let smod = bits(word, 13, 13);

        if maj4 == 0b0101 {
            let dst_hi = bits(word, 4, 2);
            if dst_hi == 0 {
                let pred = bits(word, 1, 0) as u8;
                let op2 = bits(word, 23, 22);
                if op2 == 0b00 {
                    let imm_positions = [21, 13, 12, 11, 10, 9, 8, 7, 6, 5];
                    let (imm, used) = decode_simm(word, &imm_positions, 10, immext);
                    return DecodedWord {
                        insn: DecodedInsn::CmpImm {
                            pred,
                            src: s,
                            imm,
                            kind: CmpKind::Eq,
                            unsigned: false,
                        },
                        used_ext: used,
                    };
                }
                if op2 == 0b01 {
                    let imm_positions = [21, 13, 12, 11, 10, 9, 8, 7, 6, 5];
                    let (imm, used) = decode_simm(word, &imm_positions, 10, immext);
                    return DecodedWord {
                        insn: DecodedInsn::CmpImm {
                            pred,
                            src: s,
                            imm,
                            kind: CmpKind::Gt,
                            unsigned: false,
                        },
                        used_ext: used,
                    };
                }
                if op2 == 0b10 && bits(word, 21, 21) == 0 {
                    let imm_positions = [13, 12, 11, 10, 9, 8, 7, 6, 5];
                    let (imm, used) = decode_uimm(word, &imm_positions, immext);
                    return DecodedWord {
                        insn: DecodedInsn::CmpImm {
                            pred,
                            src: s,
                            imm: imm as i32,
                            kind: CmpKind::Gtu,
                            unsigned: true,
                        },
                        used_ext: used,
                    };
                }
            }
        }

        if maj4 == 0b0000 && min3 == 0b011 && smod == 0 {
            return DecodedWord {
                insn: DecodedInsn::Mov { dst: d, src: s },
                used_ext: false,
            };
        }

        if maj4 == 0b1000 {
            let imm_positions = [
                23, 22, 20, 19, 18, 17, 16, 13, 12, 11, 10, 9, 8, 7, 6, 5,
            ];
            let (imm, used) = decode_simm(word, &imm_positions, 16, immext);
            return DecodedWord {
                insn: DecodedInsn::MovImm { dst: d, imm },
                used_ext: used,
            };
        }
    }

    if iclass == 0x9 {
        let op = bits(word, 24, 21);
        if bits(word, 27, 27) == 0 {
            if let Some((width, sign)) = decode_ld_width(op) {
                let base = bits(word, 20, 16) as u8;
                let dst = bits(word, 4, 0) as u8;
                let imm_positions = [26, 25, 13, 12, 11, 10, 9, 8, 7, 6, 5];
                let (imm, used) = decode_simm(word, &imm_positions, 11, immext);
                let offset = imm.wrapping_shl(width_shift(width) as u32);
                return DecodedWord {
                    insn: DecodedInsn::Load {
                        dst,
                        addr: AddrMode::Offset { base, offset },
                        width,
                        sign,
                    },
                    used_ext: used,
                };
            }
        }
        if bits(word, 27, 25) == 0b101 && bits(word, 13, 12) == 0 {
            if let Some((width, sign)) = decode_ld_width(op) {
                let base = bits(word, 20, 16) as u8;
                let dst = bits(word, 4, 0) as u8;
                let imm_positions = [8, 7, 6, 5];
                let (imm, used) = decode_simm(word, &imm_positions, 4, immext);
                let offset = imm.wrapping_shl(width_shift(width) as u32);
                return DecodedWord {
                    insn: DecodedInsn::Load {
                        dst,
                        addr: AddrMode::PostIncImm { base, offset },
                        width,
                        sign,
                    },
                    used_ext: used,
                };
            }
        }
    }

    if iclass == 0xa {
        let op = bits(word, 24, 21);
        if bits(word, 27, 27) == 0 {
            if let Some(width) = decode_st_width(op) {
                let base = bits(word, 20, 16) as u8;
                let src = bits(word, 12, 8) as u8;
                let imm_positions = [26, 25, 13, 7, 6, 5, 4, 3, 2, 1, 0];
                let (imm, used) = decode_simm(word, &imm_positions, 11, immext);
                let offset = imm.wrapping_shl(width_shift(width) as u32);
                return DecodedWord {
                    insn: DecodedInsn::Store {
                        src,
                        addr: AddrMode::Offset { base, offset },
                        width,
                    },
                    used_ext: used,
                };
            }
        }
        if bits(word, 27, 25) == 0b101 && bits(word, 13, 13) == 0 {
            if let Some(width) = decode_st_width(op) {
                let base = bits(word, 20, 16) as u8;
                let src = bits(word, 12, 8) as u8;
                let imm_positions = [6, 5, 4, 3];
                let (imm, used) = decode_simm(word, &imm_positions, 4, immext);
                let offset = imm.wrapping_shl(width_shift(width) as u32);
                return DecodedWord {
                    insn: DecodedInsn::Store {
                        src,
                        addr: AddrMode::PostIncImm { base, offset },
                        width,
                    },
                    used_ext: used,
                };
            }
        }
    }

    if iclass == 0x4 && bits(word, 27, 27) == 1 {
        let is_load = bits(word, 24, 24) == 1;
        let op = bits(word, 23, 21);
        if is_load {
            if let Some((width, sign)) = decode_ld_width(op) {
                let dst = bits(word, 4, 0) as u8;
                let imm_positions = [
                    26, 25, 20, 19, 18, 17, 16, 13, 12, 11, 10, 9, 8, 7, 6, 5,
                ];
                let (imm, used) = decode_uimm(word, &imm_positions, immext);
                let offset = (imm << width_shift(width)) as i32;
                return DecodedWord {
                    insn: DecodedInsn::Load {
                        dst,
                        addr: AddrMode::GpOffset { offset },
                        width,
                        sign,
                    },
                    used_ext: used,
                };
            }
        } else if let Some(width) = decode_st_width(op) {
            let src = bits(word, 12, 8) as u8;
            let imm_positions = [
                26, 25, 20, 19, 18, 17, 16, 13, 7, 6, 5, 4, 3, 2, 1, 0,
            ];
            let (imm, used) = decode_uimm(word, &imm_positions, immext);
            let offset = (imm << width_shift(width)) as i32;
            return DecodedWord {
                insn: DecodedInsn::Store {
                    src,
                    addr: AddrMode::GpOffset { offset },
                    width,
                },
                used_ext: used,
            };
        }
    }

    if iclass == 0x5 {
        let major = bits(word, 27, 24);
        if major == 0b1000 || major == 0b1001 {
            let imm_positions = [
                24, 23, 22, 21, 20, 19, 18, 17, 16, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3,
                2, 1,
            ];
            let (imm, used) = decode_simm(word, &imm_positions, 22, immext);
            return DecodedWord {
                insn: DecodedInsn::Jump { offset: imm << 2 },
                used_ext: used,
            };
        }
        if major == 0b1010 || major == 0b1011 {
            let imm_positions = [
                24, 23, 22, 21, 20, 19, 18, 17, 16, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3,
                2, 1,
            ];
            let (imm, used) = decode_simm(word, &imm_positions, 22, immext);
            return DecodedWord {
                insn: DecodedInsn::Call { offset: imm << 2 },
                used_ext: used,
            };
        }
        if major == 0b1100 {
            let pred = bits(word, 9, 8) as u8;
            let pred_new = bits(word, 11, 11) == 1;
            let sense = bits(word, 21, 21) == 0;
            let imm_positions = [23, 22, 20, 19, 18, 17, 16, 13, 7, 6, 5, 4, 3, 2, 1];
            let (imm, used) = decode_simm(word, &imm_positions, 15, immext);
            return DecodedWord {
                insn: DecodedInsn::JumpCond {
                    offset: imm << 2,
                    pred,
                    sense,
                    pred_new,
                },
                used_ext: used,
            };
        }
        if major == 0b0010 && bits(word, 23, 21) == 0b100 {
            let src = bits(word, 20, 16) as u8;
            return DecodedWord {
                insn: DecodedInsn::JumpReg { src },
                used_ext: false,
            };
        }
        if major == 0b0011 {
            let sub = bits(word, 23, 21);
            if sub == 0b010 || sub == 0b011 {
                let src = bits(word, 20, 16) as u8;
                let pred = bits(word, 9, 8) as u8;
                let pred_new = bits(word, 11, 11) == 1;
                let sense = sub == 0b010;
                return DecodedWord {
                    insn: DecodedInsn::JumpRegCond {
                        src,
                        pred,
                        sense,
                        pred_new,
                    },
                    used_ext: false,
                };
            }
        }
        if major == 0b0000 && bits(word, 23, 21) == 0b101 {
            let src = bits(word, 20, 16) as u8;
            return DecodedWord {
                insn: DecodedInsn::CallReg { src },
                used_ext: false,
            };
        }
        if major == 0b0100 && bits(word, 23, 22) == 0b00 {
            return DecodedWord {
                insn: DecodedInsn::Trap0,
                used_ext: false,
            };
        }
    }

    if iclass == 0x6 {
        let maj4 = bits(word, 27, 24);
        let min3 = bits(word, 23, 21);
        if maj4 == 0b1010 && min3 == 0b000 {
            let src = bits(word, 20, 16) as u8;
            let dst = bits(word, 4, 0) as u8;
            return DecodedWord {
                insn: DecodedInsn::TfrCrR { dst, src },
                used_ext: false,
            };
        }
        if maj4 == 0b0010 && min3 == 0b001 {
            let src = bits(word, 20, 16) as u8;
            let dst = bits(word, 4, 0) as u8;
            return DecodedWord {
                insn: DecodedInsn::TfrRrCr { dst, src },
                used_ext: false,
            };
        }
        if maj4 == 0b0000 && (min3 == 0b000 || min3 == 0b001) {
            let loop_id = if min3 == 0b000 { 0 } else { 1 };
            let count_reg = bits(word, 20, 16) as u8;
            let imm_positions = [12, 11, 10, 9, 8, 4, 3];
            let (imm, used) = decode_simm(word, &imm_positions, 7, immext);
            return DecodedWord {
                insn: DecodedInsn::LoopStartReg {
                    loop_id,
                    start_offset: imm << 2,
                    count_reg,
                },
                used_ext: used,
            };
        }
        if maj4 == 0b1001 && (min3 == 0b000 || min3 == 0b001) {
            let loop_id = if min3 == 0b000 { 0 } else { 1 };
            let imm_positions = [12, 11, 10, 9, 8, 4, 3];
            let (imm, used) = decode_simm(word, &imm_positions, 7, immext);
            let count_positions = [20, 19, 18, 17, 16, 7, 6, 5, 2, 1];
            let (count, _) = decode_uimm(word, &count_positions, None);
            return DecodedWord {
                insn: DecodedInsn::LoopStartImm {
                    loop_id,
                    start_offset: imm << 2,
                    count,
                },
                used_ext: used,
            };
        }
    }

    if iclass == 0x8 {
        let maj4 = bits(word, 27, 24);
        let min3 = bits(word, 23, 21);
        if maj4 == 0b1100 && min3 == 0b000 && bits(word, 13, 13) == 0 {
            let src = bits(word, 20, 16) as u8;
            let dst = bits(word, 4, 0) as u8;
            let amount = bits(word, 12, 8) as u8;
            let vmin3 = bits(word, 7, 5) & 0b011;
            let kind = match vmin3 {
                0b00 => ShiftKind::Asr,
                0b01 => ShiftKind::Lsr,
                0b10 => ShiftKind::Lsl,
                _ => {
                    return DecodedWord {
                        insn: DecodedInsn::Unknown(word),
                        used_ext: false,
                    }
                }
            };
            return DecodedWord {
                insn: DecodedInsn::ShiftImm {
                    dst,
                    src,
                    kind,
                    amount,
                },
                used_ext: false,
            };
        }
    }

    if iclass == 0xc {
        let maj4 = bits(word, 27, 24);
        let min3 = bits(word, 23, 21);
        if maj4 == 0b0110 && (min3 & 0b110) == 0b010 {
            let src = bits(word, 20, 16) as u8;
            let amt = bits(word, 12, 8) as u8;
            let dst = bits(word, 4, 0) as u8;
            let vmin3 = bits(word, 7, 5);
            let kind = match vmin3 & 0b110 {
                0b000 => ShiftKind::Asr,
                0b010 => ShiftKind::Lsr,
                0b100 | 0b110 => ShiftKind::Lsl,
                _ => {
                    return DecodedWord {
                        insn: DecodedInsn::Unknown(word),
                        used_ext: false,
                    }
                }
            };
            return DecodedWord {
                insn: DecodedInsn::ShiftReg {
                    dst,
                    src,
                    amt,
                    kind,
                },
                used_ext: false,
            };
        }
    }

    if iclass == 0xe {
        let regtype = bits(word, 27, 24);
        if regtype == 0b1101
            && bits(word, 23, 21) == 0
            && bits(word, 13, 13) == 0
            && bits(word, 7, 5) == 0
        {
            let src1 = bits(word, 20, 16) as u8;
            let src2 = bits(word, 12, 8) as u8;
            let dst = bits(word, 4, 0) as u8;
            return DecodedWord {
                insn: DecodedInsn::Mul { dst, src1, src2 },
                used_ext: false,
            };
        }
    }

    DecodedWord {
        insn: DecodedInsn::Unknown(word),
        used_ext: false,
    }
}
