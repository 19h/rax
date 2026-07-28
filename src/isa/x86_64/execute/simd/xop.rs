//! AMD XOP packed rotate and signed-direction shift execution.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute::system::is_canonical_48;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VcpuExit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XopPackedBitKind {
    Rotate,
    LogicalShift,
    ArithmeticShift,
}

#[inline]
fn xmm_bytes(vcpu: &X86_64Vcpu, register: u8) -> [u8; 16] {
    let value = vcpu.regs.xmm[usize::from(register)];
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&value[0].to_le_bytes());
    bytes[8..].copy_from_slice(&value[1].to_le_bytes());
    bytes
}

#[inline]
fn write_xmm_zero_upper(vcpu: &mut X86_64Vcpu, register: u8, bytes: [u8; 16]) {
    let index = usize::from(register);
    vcpu.regs.xmm[index] = [
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..].try_into().unwrap()),
    ];
    vcpu.regs.ymm_high[index] = [0; 2];
    vcpu.regs.zmm_high[index] = [0; 4];
}

#[inline]
fn vector_bytes(vcpu: &X86_64Vcpu, register: u8, width: usize) -> [u8; 32] {
    debug_assert!(matches!(width, 16 | 32));
    let index = usize::from(register);
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&vcpu.regs.xmm[index][0].to_le_bytes());
    bytes[8..16].copy_from_slice(&vcpu.regs.xmm[index][1].to_le_bytes());
    if width == 32 {
        bytes[16..24].copy_from_slice(&vcpu.regs.ymm_high[index][0].to_le_bytes());
        bytes[24..32].copy_from_slice(&vcpu.regs.ymm_high[index][1].to_le_bytes());
    }
    bytes
}

#[inline]
fn write_vector_zero_upper(vcpu: &mut X86_64Vcpu, register: u8, width: usize, bytes: [u8; 32]) {
    debug_assert!(matches!(width, 16 | 32));
    let index = usize::from(register);
    vcpu.regs.xmm[index] = [
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    ];
    vcpu.regs.ymm_high[index] = if width == 32 {
        [
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        ]
    } else {
        [0; 2]
    };
    vcpu.regs.zmm_high[index] = [0; 4];
}

#[inline]
fn transform_element(value: u64, bits: u32, count: i8, kind: XopPackedBitKind) -> u64 {
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1_u64 << bits) - 1
    };
    let amount = u32::from(count.unsigned_abs()) & (bits - 1);
    match (kind, count.is_negative()) {
        (XopPackedBitKind::Rotate, false) => {
            if bits == 64 {
                value.rotate_left(amount)
            } else {
                ((value << amount) | (value >> ((bits - amount) & (bits - 1)))) & mask
            }
        }
        (XopPackedBitKind::Rotate, true) => {
            if bits == 64 {
                value.rotate_right(amount)
            } else {
                ((value >> amount) | (value << ((bits - amount) & (bits - 1)))) & mask
            }
        }
        (XopPackedBitKind::LogicalShift, false) | (XopPackedBitKind::ArithmeticShift, false) => {
            (value << amount) & mask
        }
        (XopPackedBitKind::LogicalShift, true) => value >> amount,
        (XopPackedBitKind::ArithmeticShift, true) => {
            let signed = if bits == 64 {
                value as i64
            } else {
                ((value << (64 - bits)) as i64) >> (64 - bits)
            };
            ((signed >> amount) as u64) & mask
        }
    }
}

fn transform_vector(
    source: [u8; 16],
    counts: Option<[u8; 16]>,
    fixed_count: Option<u8>,
    element_bytes: usize,
    kind: XopPackedBitKind,
) -> [u8; 16] {
    let mut result = [0_u8; 16];
    let bits = (element_bytes * 8) as u32;
    for offset in (0..16).step_by(element_bytes) {
        let mut element = [0_u8; 8];
        element[..element_bytes].copy_from_slice(&source[offset..offset + element_bytes]);
        let value = u64::from_le_bytes(element);
        let count =
            fixed_count.unwrap_or_else(|| counts.expect("variable XOP count vector")[offset]) as i8;
        let transformed = transform_element(value, bits, count, kind).to_le_bytes();
        result[offset..offset + element_bytes].copy_from_slice(&transformed[..element_bytes]);
    }
    result
}

#[inline]
fn alignment_check_enabled(vcpu: &X86_64Vcpu) -> bool {
    const CR0_AM: u64 = 1 << 18;
    vcpu.sregs.cr0 & CR0_AM != 0
        && vcpu.regs.rflags & flags::bits::AC != 0
        && vcpu.sregs.cs.selector & 3 == 3
}

/// Execute VPROT[B/W/D/Q], VPSHL[B/W/D/Q], or VPSHA[B/W/D/Q].
///
/// Encoding and dynamic XOP/#NM validation have already completed. Memory
/// address validation remains here so #SS/#GP and enabled #AC are raised before
/// the single complete 16-byte read and before any destination state commits.
pub(in crate::isa::x86_64) fn execute_xop_packed_bit(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
    vvvv: u8,
    w: bool,
    fixed_count: bool,
) -> Result<Option<VcpuExit>> {
    let (kind, element_bytes) = match opcode {
        0x90 | 0xC0 => (XopPackedBitKind::Rotate, 1),
        0x91 | 0xC1 => (XopPackedBitKind::Rotate, 2),
        0x92 | 0xC2 => (XopPackedBitKind::Rotate, 4),
        0x93 | 0xC3 => (XopPackedBitKind::Rotate, 8),
        0x94 => (XopPackedBitKind::LogicalShift, 1),
        0x95 => (XopPackedBitKind::LogicalShift, 2),
        0x96 => (XopPackedBitKind::LogicalShift, 4),
        0x97 => (XopPackedBitKind::LogicalShift, 8),
        0x98 => (XopPackedBitKind::ArithmeticShift, 1),
        0x99 => (XopPackedBitKind::ArithmeticShift, 2),
        0x9A => (XopPackedBitKind::ArithmeticShift, 4),
        0x9B => (XopPackedBitKind::ArithmeticShift, 8),
        _ => return vcpu.inject_undefined_instruction(),
    };

    let modrm_offset = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let destination = ((modrm >> 3) & 7) | ctx.any_rex_r();
    let rm = (modrm & 7) | ctx.any_rex_b();
    let is_memory = modrm >> 6 != 3;

    let memory_operand = if is_memory {
        let (address, extra, stack_segment) =
            vcpu.decode_modrm_addr_with_stack_segment(ctx, modrm_offset)?;
        ctx.cursor = modrm_offset + 1 + extra;
        Some((address, stack_segment))
    } else {
        None
    };
    let fixed_count = if fixed_count {
        Some(ctx.consume_u8()?)
    } else {
        None
    };

    let rm_value = if let Some((address, stack_segment)) = memory_operand {
        if vcpu.sregs.cs.l {
            let canonical = address
                .checked_add(15)
                .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last));
            if !canonical {
                vcpu.inject_exception(if stack_segment { 12 } else { 13 }, Some(0))?;
                return Ok(None);
            }
        }
        if address & 15 != 0 && alignment_check_enabled(vcpu) {
            vcpu.inject_exception(17, Some(0))?;
            return Ok(None);
        }

        let bytes = vcpu.read_bytes(address, 16)?;
        bytes.try_into().expect("complete 16-byte XOP memory read")
    } else {
        xmm_bytes(vcpu, rm)
    };

    let (source, counts) = if fixed_count.is_some() {
        (rm_value, None)
    } else if w {
        (xmm_bytes(vcpu, vvvv), Some(rm_value))
    } else {
        (rm_value, Some(xmm_bytes(vcpu, vvvv)))
    };
    let result = transform_vector(source, counts, fixed_count, element_bytes, kind);
    write_xmm_zero_upper(vcpu, destination, result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Execute AMD XOP VPCMOV.
///
/// Encoding and dynamic XOP/#NM validation have already completed. The
/// immediate source-register selector is decoded before the data read; memory
/// canonicality and enabled #AC are validated before the complete 16-byte or
/// 32-byte read and before any destination state commits.
pub(in crate::isa::x86_64) fn execute_xop_vpcmov(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    vvvv: u8,
    w: bool,
    l: u8,
) -> Result<Option<VcpuExit>> {
    let width = if l == 0 { 16 } else { 32 };
    let modrm_offset = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let destination = ((modrm >> 3) & 7) | ctx.any_rex_r();
    let rm = (modrm & 7) | ctx.any_rex_b();
    let is_memory = modrm >> 6 != 3;

    let memory_operand = if is_memory {
        let (address, extra, stack_segment) =
            vcpu.decode_modrm_addr_with_stack_segment(ctx, modrm_offset)?;
        ctx.cursor = modrm_offset + 1 + extra;
        Some((address, stack_segment))
    } else {
        None
    };
    let immediate = ctx.consume_u8()?;
    // In protected/compatibility 32-bit mode the high selector bit is ignored,
    // matching the architectural restriction to XMM/YMM0-7.
    let selected_register = (immediate >> 4) & if vcpu.sregs.cs.l { 0x0F } else { 0x07 };

    let rm_value = if let Some((address, stack_segment)) = memory_operand {
        if vcpu.sregs.cs.l {
            let canonical = address
                .checked_add(width as u64 - 1)
                .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last));
            if !canonical {
                vcpu.inject_exception(if stack_segment { 12 } else { 13 }, Some(0))?;
                return Ok(None);
            }
        }
        if address & 15 != 0 && alignment_check_enabled(vcpu) {
            vcpu.inject_exception(17, Some(0))?;
            return Ok(None);
        }

        let loaded = vcpu.read_bytes(address, width)?;
        let mut bytes = [0_u8; 32];
        bytes[..width].copy_from_slice(&loaded);
        bytes
    } else {
        vector_bytes(vcpu, rm, width)
    };

    let source1 = vector_bytes(vcpu, vvvv, width);
    let selected = vector_bytes(vcpu, selected_register, width);
    let (source2, mask) = if w {
        (selected, rm_value)
    } else {
        (rm_value, selected)
    };
    let mut result = [0_u8; 32];
    for byte in 0..width {
        result[byte] = (source1[byte] & mask[byte]) | (source2[byte] & !mask[byte]);
    }
    write_vector_zero_upper(vcpu, destination, width, result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[inline]
fn vpcom_lane(value: &[u8], offset: usize, element_bytes: usize) -> u64 {
    let mut lane = [0_u8; 8];
    lane[..element_bytes].copy_from_slice(&value[offset..offset + element_bytes]);
    u64::from_le_bytes(lane)
}

#[inline]
fn vpcom_signed(value: u64, bits: u32) -> i64 {
    if bits == 64 {
        value as i64
    } else {
        let shift = 64 - bits;
        ((value << shift) as i64) >> shift
    }
}

/// Execute AMD XOP VPCOMB/W/D/Q and VPCOMUB/UW/UD/UQ.
///
/// Encoding and dynamic XOP/#NM validation have already completed. The
/// immediate follows ModR/M addressing, so `rip_relative_offset` accounts for
/// it before address calculation. A memory source is validated as one aligned
/// 16-byte access before any destination state commits.
pub(in crate::isa::x86_64) fn execute_xop_vpcom(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
    vvvv: u8,
) -> Result<Option<VcpuExit>> {
    let (element_bytes, signed) = match opcode {
        0xCC => (1, true),
        0xCD => (2, true),
        0xCE => (4, true),
        0xCF => (8, true),
        0xEC => (1, false),
        0xED => (2, false),
        0xEE => (4, false),
        0xEF => (8, false),
        _ => return vcpu.inject_undefined_instruction(),
    };

    let modrm_offset = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let destination = ((modrm >> 3) & 7) | ctx.any_rex_r();
    let rm = (modrm & 7) | ctx.any_rex_b();
    let memory_operand = if modrm >> 6 != 3 {
        let (address, extra, stack_segment) =
            vcpu.decode_modrm_addr_with_stack_segment(ctx, modrm_offset)?;
        ctx.cursor = modrm_offset + 1 + extra;
        Some((address, stack_segment))
    } else {
        None
    };
    let predicate = ctx.consume_u8()? & 7;

    let source2 = if let Some((address, stack_segment)) = memory_operand {
        if vcpu.sregs.cs.l {
            let canonical = address
                .checked_add(15)
                .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last));
            if !canonical {
                vcpu.inject_exception(if stack_segment { 12 } else { 13 }, Some(0))?;
                return Ok(None);
            }
        }
        if address & 15 != 0 && alignment_check_enabled(vcpu) {
            vcpu.inject_exception(17, Some(0))?;
            return Ok(None);
        }
        let bytes = vcpu.read_bytes(address, 16)?;
        bytes
            .try_into()
            .expect("complete 16-byte VPCOM memory read")
    } else {
        xmm_bytes(vcpu, rm)
    };
    let source1 = xmm_bytes(vcpu, vvvv);
    let bits = (element_bytes * 8) as u32;
    let mut result = [0_u8; 16];
    for offset in (0..16).step_by(element_bytes) {
        let left = vpcom_lane(&source1, offset, element_bytes);
        let right = vpcom_lane(&source2, offset, element_bytes);
        let condition = match predicate {
            0 if signed => vpcom_signed(left, bits) < vpcom_signed(right, bits),
            1 if signed => vpcom_signed(left, bits) <= vpcom_signed(right, bits),
            2 if signed => vpcom_signed(left, bits) > vpcom_signed(right, bits),
            3 if signed => vpcom_signed(left, bits) >= vpcom_signed(right, bits),
            0 => left < right,
            1 => left <= right,
            2 => left > right,
            3 => left >= right,
            4 => left == right,
            5 => left != right,
            6 => false,
            7 => true,
            _ => unreachable!("VPCOM predicate is masked to three bits"),
        };
        result[offset..offset + element_bytes].fill(if condition { u8::MAX } else { 0 });
    }

    write_xmm_zero_upper(vcpu, destination, result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{XopPackedBitKind, transform_element, transform_vector};

    #[test]
    fn signed_counts_reduce_modulo_element_width_without_overflow() {
        assert_eq!(
            transform_element(0x81, 8, 1, XopPackedBitKind::Rotate),
            0x03
        );
        assert_eq!(
            transform_element(0x81, 8, -1, XopPackedBitKind::Rotate),
            0xC0
        );
        assert_eq!(
            transform_element(0x81, 8, i8::MIN, XopPackedBitKind::Rotate),
            0x81
        );
        assert_eq!(
            transform_element(0x80, 8, -7, XopPackedBitKind::ArithmeticShift),
            0xFF
        );
        assert_eq!(
            transform_element(0x80, 8, -7, XopPackedBitKind::LogicalShift),
            0x01
        );
    }

    #[test]
    fn vector_counts_use_each_elements_low_byte() {
        let source = [
            0x01, 0x80, 0x34, 0x12, 0x78, 0x56, 0xBC, 0x9A, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45,
            0x23, 0x01,
        ];
        let mut counts = [0_u8; 16];
        counts[0] = 1;
        counts[2] = (-1_i8) as u8;
        counts[4] = 17;
        counts[6] = (-17_i8) as u8;
        let result = transform_vector(
            source,
            Some(counts),
            None,
            2,
            XopPackedBitKind::LogicalShift,
        );
        assert_eq!(
            &result[..8],
            &[0x02, 0x00, 0x1A, 0x09, 0xF0, 0xAC, 0x5E, 0x4D]
        );
    }
}
