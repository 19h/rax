//! Duplex subinstruction decoding and V68 Slot 0/Slot 1 class selection.

use super::*;
use crate::isa::hexagon::opcode::EncClass;

fn subreg(code: u8) -> u8 {
    if code < 8 { code } else { code + 8 }
}

fn subreg_pair(code: u8) -> u8 {
    let code = code & 0x7;
    if code < 4 {
        code * 2
    } else {
        16 + (code - 4) * 2
    }
}

fn decode_subinsn(sub: u16, class: EncClass, isa: HexagonIsa) -> Option<DecodedSub> {
    let decoded = opcode::decode_sub(sub, class)?;
    let opcode = decoded.opcode;
    let insn = match opcode {
        Opcode::SL1_loadri_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            let offset = imm << 2;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset { base, offset },
                width: MemWidth::Word,
                sign: MemSign::Unsigned,
                pred: None,
            }
        }
        Opcode::SL1_loadrub_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset { base, offset: imm },
                width: MemWidth::Byte,
                sign: MemSign::Unsigned,
                pred: None,
            }
        }
        Opcode::SS1_storew_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let src = subreg(field_u8(&decoded, b't')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Store {
                src,
                addr: AddrMode::Offset {
                    base,
                    offset: imm << 2,
                },
                width: MemWidth::Word,
                pred: None,
                src_new: false,
                high_half: false,
            }
        }
        Opcode::SS1_storeb_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let src = subreg(field_u8(&decoded, b't')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Store {
                src,
                addr: AddrMode::Offset { base, offset: imm },
                width: MemWidth::Byte,
                pred: None,
                src_new: false,
                high_half: false,
            }
        }
        Opcode::SL2_loadrh_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset {
                    base,
                    offset: imm << 1,
                },
                width: MemWidth::Half,
                sign: MemSign::Signed,
                pred: None,
            }
        }
        Opcode::SL2_loadruh_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset {
                    base,
                    offset: imm << 1,
                },
                width: MemWidth::Half,
                sign: MemSign::Unsigned,
                pred: None,
            }
        }
        Opcode::SL2_loadrb_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset { base, offset: imm },
                width: MemWidth::Byte,
                sign: MemSign::Signed,
                pred: None,
            }
        }
        Opcode::SL2_loadri_sp => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset {
                    base: 29,
                    offset: imm << 2,
                },
                width: MemWidth::Word,
                sign: MemSign::Unsigned,
                pred: None,
            }
        }
        Opcode::SL2_loadrd_sp => {
            let dst = subreg_pair(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Load {
                dst,
                addr: AddrMode::Offset {
                    base: 29,
                    offset: imm << 3,
                },
                width: MemWidth::Double,
                sign: MemSign::Unsigned,
                pred: None,
            }
        }
        Opcode::SL2_deallocframe => DecodedInsn::DeallocFrame {
            base: 30,
            dst: None,
            update_lr_fp: true,
        },
        Opcode::SL2_return => DecodedInsn::DeallocReturn {
            base: 30,
            dst: None,
            pred: None,
            update_lr_fp: true,
        },
        Opcode::SL2_return_t => DecodedInsn::DeallocReturn {
            base: 30,
            dst: None,
            pred: Some(pred_cond(0, true, false)),
            update_lr_fp: true,
        },
        Opcode::SL2_return_f => DecodedInsn::DeallocReturn {
            base: 30,
            dst: None,
            pred: Some(pred_cond(0, false, false)),
            update_lr_fp: true,
        },
        Opcode::SL2_return_tnew => DecodedInsn::DeallocReturn {
            base: 30,
            dst: None,
            pred: Some(pred_cond(0, true, true)),
            update_lr_fp: true,
        },
        Opcode::SL2_return_fnew => DecodedInsn::DeallocReturn {
            base: 30,
            dst: None,
            pred: Some(pred_cond(0, false, true)),
            update_lr_fp: true,
        },
        Opcode::SL2_jumpr31 => DecodedInsn::JumpReg { src: 31 },
        Opcode::SL2_jumpr31_t => DecodedInsn::JumpRegCond {
            src: 31,
            pred: 0,
            sense: true,
            pred_new: false,
        },
        Opcode::SL2_jumpr31_f => DecodedInsn::JumpRegCond {
            src: 31,
            pred: 0,
            sense: false,
            pred_new: false,
        },
        Opcode::SL2_jumpr31_tnew => DecodedInsn::JumpRegCond {
            src: 31,
            pred: 0,
            sense: true,
            pred_new: true,
        },
        Opcode::SL2_jumpr31_fnew => DecodedInsn::JumpRegCond {
            src: 31,
            pred: 0,
            sense: false,
            pred_new: true,
        },
        Opcode::SS2_allocframe => {
            let imm = decode_field_uimm(&decoded, b'i', None)?.0;
            DecodedInsn::AllocFrame {
                base: 29,
                size: imm << 3,
            }
        }
        Opcode::SS2_storeh_io => {
            let base = subreg(field_u8(&decoded, b's')?);
            let src = subreg(field_u8(&decoded, b't')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Store {
                src,
                addr: AddrMode::Offset {
                    base,
                    offset: imm << 1,
                },
                width: MemWidth::Half,
                pred: None,
                src_new: false,
                high_half: false,
            }
        }
        Opcode::SS2_storew_sp => {
            let src = subreg(field_u8(&decoded, b't')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::Store {
                src,
                addr: AddrMode::Offset {
                    base: 29,
                    offset: imm << 2,
                },
                width: MemWidth::Word,
                pred: None,
                src_new: false,
                high_half: false,
            }
        }
        Opcode::SS2_stored_sp => {
            let src = subreg_pair(field_u8(&decoded, b't')?);
            let imm = decode_simm_val(decode_field_uimm(&decoded, b'i', None)?.0, 6, None).0;
            DecodedInsn::Store {
                src,
                addr: AddrMode::Offset {
                    base: 29,
                    offset: imm << 3,
                },
                width: MemWidth::Double,
                pred: None,
                src_new: false,
                high_half: false,
            }
        }
        Opcode::SS2_storewi0 => {
            let base = subreg(field_u8(&decoded, b's')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::StoreImm {
                value: 0,
                addr: AddrMode::Offset {
                    base,
                    offset: imm << 2,
                },
                width: MemWidth::Word,
                pred: None,
            }
        }
        Opcode::SS2_storewi1 => {
            let base = subreg(field_u8(&decoded, b's')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::StoreImm {
                value: 1,
                addr: AddrMode::Offset {
                    base,
                    offset: imm << 2,
                },
                width: MemWidth::Word,
                pred: None,
            }
        }
        Opcode::SS2_storebi0 => {
            let base = subreg(field_u8(&decoded, b's')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::StoreImm {
                value: 0,
                addr: AddrMode::Offset { base, offset: imm },
                width: MemWidth::Byte,
                pred: None,
            }
        }
        Opcode::SS2_storebi1 => {
            let base = subreg(field_u8(&decoded, b's')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::StoreImm {
                value: 1,
                addr: AddrMode::Offset { base, offset: imm },
                width: MemWidth::Byte,
                pred: None,
            }
        }
        Opcode::SA1_addi => {
            let rx = subreg(field_u8(&decoded, b'x')?);
            let imm = decode_simm_val(decode_field_uimm(&decoded, b'i', None)?.0, 7, None).0;
            DecodedInsn::AddImm {
                dst: rx,
                src: rx,
                imm,
            }
        }
        Opcode::SA1_addsp => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::AddImm {
                dst,
                src: 29,
                imm: imm << 2,
            }
        }
        Opcode::SA1_seti => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::MovImm { dst, imm }
        }
        Opcode::SA1_setin1 => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            DecodedInsn::MovImm { dst, imm: -1 }
        }
        Opcode::SA1_tfr => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Mov { dst, src }
        }
        Opcode::SA1_inc => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::AddImm { dst, src, imm: 1 }
        }
        Opcode::SA1_dec => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::AddImm { dst, src, imm: -1 }
        }
        Opcode::SA1_and1 => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::AndImm { dst, src, imm: 1 }
        }
        Opcode::SA1_addrx => {
            // Rx = add(Rx, Rs)  (read-modify GPR; fields x=Rx, s=Rs).
            let rx = subreg(field_u8(&decoded, b'x')?);
            let rs = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Add {
                dst: rx,
                src1: rx,
                src2: rs,
            }
        }
        Opcode::SA1_sxtb => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Extend {
                dst,
                src,
                kind: ExtendKind::Sxt8,
            }
        }
        Opcode::SA1_sxth => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Extend {
                dst,
                src,
                kind: ExtendKind::Sxt16,
            }
        }
        Opcode::SA1_zxtb => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Extend {
                dst,
                src,
                kind: ExtendKind::Zxt8,
            }
        }
        Opcode::SA1_zxth => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Extend {
                dst,
                src,
                kind: ExtendKind::Zxt16,
            }
        }
        Opcode::SA1_cmpeqi => {
            let src = subreg(field_u8(&decoded, b's')?);
            let imm = decode_field_uimm(&decoded, b'i', None)?.0 as i32;
            DecodedInsn::CmpImm {
                pred: 0,
                src,
                imm,
                kind: CmpKind::Eq,
                unsigned: false,
            }
        }
        Opcode::SA1_clrt => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            DecodedInsn::ClearCond {
                dst,
                pred: pred_cond(0, true, false),
            }
        }
        Opcode::SA1_clrf => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            DecodedInsn::ClearCond {
                dst,
                pred: pred_cond(0, false, false),
            }
        }
        Opcode::SA1_clrtnew => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            DecodedInsn::ClearCond {
                dst,
                pred: pred_cond(0, true, true),
            }
        }
        Opcode::SA1_clrfnew => {
            let dst = subreg(field_u8(&decoded, b'd')?);
            DecodedInsn::ClearCond {
                dst,
                pred: pred_cond(0, false, true),
            }
        }
        Opcode::SA1_combinezr => {
            let dst = subreg_pair(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Combine {
                dst,
                high: CombineOperand::Imm(0),
                low: CombineOperand::Reg(src),
            }
        }
        Opcode::SA1_combinerz => {
            let dst = subreg_pair(field_u8(&decoded, b'd')?);
            let src = subreg(field_u8(&decoded, b's')?);
            DecodedInsn::Combine {
                dst,
                high: CombineOperand::Reg(src),
                low: CombineOperand::Imm(0),
            }
        }
        Opcode::SA1_combine0i
        | Opcode::SA1_combine1i
        | Opcode::SA1_combine2i
        | Opcode::SA1_combine3i => {
            let dst = subreg_pair(field_u8(&decoded, b'd')?);
            let low = decode_field_uimm(&decoded, b'i', None)?.0;
            let high = match decoded.opcode {
                Opcode::SA1_combine0i => 0,
                Opcode::SA1_combine1i => 1,
                Opcode::SA1_combine2i => 2,
                _ => 3,
            };
            DecodedInsn::Combine {
                dst,
                high: CombineOperand::Imm(high),
                low: CombineOperand::Imm(low),
            }
        }
        _ => DecodedInsn::Unknown(sub as u32),
    };

    if insn_uses_dotnew(&insn) && !isa_supports_dotnew(isa) {
        return None;
    }

    Some(DecodedSub {
        insn,
        opcode: Some(opcode),
    })
}

fn duplex_iclass(word: u32) -> u8 {
    let low = ((word >> 13) & 0x1) as u8;
    let high = ((word >> 29) & 0x7) as u8;
    (high << 1) | low
}

pub fn decode_duplex(word: u32, isa: HexagonIsa) -> Option<(DecodedSub, DecodedSub)> {
    if !isa_supports_duplex(isa) {
        return None;
    }
    let iclass = duplex_iclass(word);
    let slot0 = (word & 0x1fff) as u16;
    let slot1 = ((word >> 16) & 0x1fff) as u16;

    // V68 table 10-5 lists low Slot 0 first, then high Slot 1.
    let (class0, class1) = match iclass {
        0x0 => (EncClass::SubinsnL1, EncClass::SubinsnL1),
        0x1 => (EncClass::SubinsnL2, EncClass::SubinsnL1),
        0x2 => (EncClass::SubinsnL2, EncClass::SubinsnL2),
        0x3 => (EncClass::SubinsnA, EncClass::SubinsnA),
        0x4 => (EncClass::SubinsnL1, EncClass::SubinsnA),
        0x5 => (EncClass::SubinsnL2, EncClass::SubinsnA),
        0x6 => (EncClass::SubinsnS1, EncClass::SubinsnA),
        0x7 => (EncClass::SubinsnS2, EncClass::SubinsnA),
        0x8 => (EncClass::SubinsnS1, EncClass::SubinsnL1),
        0x9 => (EncClass::SubinsnS1, EncClass::SubinsnL2),
        0xa => (EncClass::SubinsnS1, EncClass::SubinsnS1),
        0xb => (EncClass::SubinsnS2, EncClass::SubinsnS1),
        0xc => (EncClass::SubinsnS2, EncClass::SubinsnL1),
        0xd => (EncClass::SubinsnS2, EncClass::SubinsnL2),
        0xe => (EncClass::SubinsnS2, EncClass::SubinsnS2),
        _ => return None,
    };

    let insn1 = decode_subinsn(slot1, class1, isa)?;
    let insn0 = decode_subinsn(slot0, class0, isa)?;
    Some((insn1, insn0))
}
