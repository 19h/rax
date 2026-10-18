//! Legacy 128-bit bit-vector operations. No floating-point arithmetic or
//! VEX/EVEX upper-lane semantics are projected into this register bank.
use super::*;

fn vector(r: VReg) -> Checked<RaxSmirOperand> {
    let operand = reg(r)?;
    if operand.kind == 1 || operand.kind == 4 {
        Ok(operand)
    } else {
        Err(())
    }
}

pub(super) fn legacy_encoding(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .copied()
        .find(|&b| {
            !matches!(
                b,
                0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 | 0x66 | 0x67 | 0xf0 | 0xf2 | 0xf3 | 0x40
                    ..=0x4f
            )
        })
        .is_some_and(|b| !matches!(b, 0xc4 | 0xc5 | 0x62 | 0x8f))
}

pub(super) fn operation(op: &SmirOp) -> Checked<Option<RaxSmirOp>> {
    let mut out = RaxSmirOp {
        struct_size: size_of::<RaxSmirOp>() as u32,
        abi_version: RAX_SMIR_ABI_VERSION,
        width_bits: 128,
        guest_pc: op.guest_pc,
        ..Default::default()
    };
    match &op.kind {
        OpKind::VMov { dst, src, width } if *width == VecWidth::V128 => {
            out.opcode = RAX_SMIR_VMOV;
            out.dst = vector(*dst)?;
            out.a = vector(*src)?;
        }
        OpKind::VLoad {
            dst,
            addr: a,
            width,
        } if *width == VecWidth::V128 => {
            out.opcode = RAX_SMIR_VLOAD;
            out.dst = vector(*dst)?;
            out.address = addr(a, op.guest_pc)?;
        }
        OpKind::VStore {
            src,
            addr: a,
            width,
        } if *width == VecWidth::V128 => {
            out.opcode = RAX_SMIR_VSTORE;
            out.a = vector(*src)?;
            out.address = addr(a, op.guest_pc)?;
        }
        OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } if *width == VecWidth::V128 => {
            out.opcode = RAX_SMIR_VXOR;
            out.dst = vector(*dst)?;
            out.a = vector(*src1)?;
            out.b = vector(*src2)?;
        }
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem,
        } => {
            out.opcode = RAX_SMIR_VINSERT;
            out.dst = vector(*dst)?;
            out.a = vector(*vec)?;
            out.b = reg(*scalar)?;
            out.source_width_bits = elem.bytes() * 8;
            out.auxiliary = u32::from(*lane);
            if out.source_width_bits > 64
                || (out.auxiliary + 1) * out.source_width_bits > 128
                || out.b.kind == 4
            {
                return Err(());
            }
        }
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign,
        } => {
            out.opcode = RAX_SMIR_VEXTRACT;
            out.width_bits = 64;
            out.dst = reg(*dst)?;
            out.a = vector(*vec)?;
            out.source_width_bits = elem.bytes() * 8;
            out.auxiliary = u32::from(*lane);
            out.condition = (*sign == SignExtend::Sign) as u32;
            if out.source_width_bits > 64
                || (out.auxiliary + 1) * out.source_width_bits > 128
                || out.dst.kind == 4
            {
                return Err(());
            }
        }
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } => {
            out.opcode = RAX_SMIR_VBROADCAST;
            out.dst = vector(*dst)?;
            out.a = reg(*scalar)?;
            out.source_width_bits = elem.bytes() * 8;
            out.auxiliary = u32::from(*lanes);
            if out.dst.kind != 1
                || out.a.kind == 4
                || out.source_width_bits > 64
                || out.auxiliary * out.source_width_bits > 128
            {
                return Err(());
            }
        }
        OpKind::X86MovdQ {
            dst,
            src,
            width,
            zero_upper,
        } => {
            if *zero_upper || !matches!(width, OpWidth::W32 | OpWidth::W64) {
                return Err(());
            }
            out.opcode = RAX_SMIR_X86_MOVDQ;
            out.width_bits = width.bits();
            out.dst = reg(*dst)?;
            out.a = reg(*src)?;
            if (out.dst.kind == 4) == (out.a.kind == 4) {
                return Err(());
            }
        }
        OpKind::X86CheckAlignment { addr: a, alignment } => {
            if !alignment.is_power_of_two() {
                return Err(());
            }
            out.opcode = RAX_SMIR_CHECK_ALIGNMENT;
            out.width_bits = 64;
            out.address = addr(a, op.guest_pc)?;
            out.auxiliary = u32::from(*alignment);
        }
        _ => return Ok(None),
    }
    Ok(Some(out))
}
