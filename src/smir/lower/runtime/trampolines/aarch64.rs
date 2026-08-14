//! trampolines::aarch64 tests

use super::*;
use crate::smir::lower::runtime::*;

/// Convert the four x86 status flags representable by AArch64 PSTATE into
/// architectural NZCV bit positions. PF/AF and every control flag remain in the
/// x86 state object and are deliberately not encoded here.
pub fn x86_rflags_to_aarch64_nzcv(rflags: u64) -> u64 {
    const X86_CF: u64 = 1 << 0;
    const X86_ZF: u64 = 1 << 6;
    const X86_SF: u64 = 1 << 7;
    const X86_OF: u64 = 1 << 11;
    const A64_N: u64 = 1 << 31;
    const A64_Z: u64 = 1 << 30;
    const A64_C: u64 = 1 << 29;
    const A64_V: u64 = 1 << 28;

    (u64::from(rflags & X86_SF != 0) * A64_N)
        | (u64::from(rflags & X86_ZF != 0) * A64_Z)
        | (u64::from(rflags & X86_CF != 0) * A64_C)
        | (u64::from(rflags & X86_OF != 0) * A64_V)
}
/// Merge architectural NZCV back into an x86 RFLAGS snapshot. Exactly
/// CF/ZF/SF/OF are replaced; PF/AF, control flags, reserved bits, and all other
/// state are preserved from `prior_rflags`.
pub fn merge_aarch64_nzcv_into_x86_rflags(prior_rflags: u64, nzcv: u64) -> u64 {
    const X86_CF: u64 = 1 << 0;
    const X86_ZF: u64 = 1 << 6;
    const X86_SF: u64 = 1 << 7;
    const X86_OF: u64 = 1 << 11;
    const X86_NZCV: u64 = X86_CF | X86_ZF | X86_SF | X86_OF;
    const A64_N: u64 = 1 << 31;
    const A64_Z: u64 = 1 << 30;
    const A64_C: u64 = 1 << 29;
    const A64_V: u64 = 1 << 28;

    (prior_rflags & !X86_NZCV)
        | (u64::from(nzcv & A64_C != 0) * X86_CF)
        | (u64::from(nzcv & A64_Z != 0) * X86_ZF)
        | (u64::from(nzcv & A64_N != 0) * X86_SF)
        | (u64::from(nzcv & A64_V != 0) * X86_OF)
}
/// Decide whether x86-lifted SMIR can execute through the AArch64 identity-map
/// trampoline without changing architectural meaning. This is intentionally a
/// separate gate from [`is_aarch64_native_clobber_safe_excluding`], which models
/// an AArch64 guest and therefore has different register and flag semantics.
///
/// The initial production bridge is register-only and maps legacy x86 GPRs
/// RAX..R15 to X0..X15. It admits only operations already in the scalar JIT
/// whitelist (plus validated BMI/ADX scalar forms), rejects virtual writes and
/// non-legacy register operands, and applies an x86-specific flag-liveness pass:
///
/// - PF/AF have no NZCV representation, so a definition is allowed only when it
///   is dead before any use or native exit; parity consumers always bail.
/// - NZV and carry-producing operations use the canonical CF→C mapping.
/// - AArch64 subtraction exposes no-borrow in C, the inverse of x86 CF. A live
///   CF definition by SUB/CMP/NEG therefore bails. SBB is admitted because its
///   x86-register lowering explicitly inverts C before and after SBC. Generic
///   CF-based unsigned conditions still bail pending equivalent normalization.
pub fn is_x86_aarch64_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    let flag_live_in = x86_flag_live_in(func, excluded);
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .all(|block| {
            let flags_live_out = x86_block_flag_live_out(block, excluded, &flag_live_in);
            x86_aarch64_block_is_clobber_safe(block, flags_live_out)
        })
}
pub(crate) fn x86_aarch64_block_flags_are_representable(
    block: &crate::smir::ir::SmirBlock,
    mut live: crate::smir::ir::flags::FlagSet,
) -> bool {
    use crate::smir::ir::flags::FlagSet;
    use crate::smir::ir::ops::{OpKind, X86AdxKind};

    let unavailable = FlagSet::PF.union(FlagSet::AF);
    for op in block.ops.iter().rev() {
        let uses = x86_flag_uses(&op.kind);
        if !uses.intersection(unavailable).is_empty() {
            return false;
        }

        // Canonical bridge state stores x86 CF directly in NZCV.C. ADC and the
        // rotate/ADX carry chains consume that representation directly. The
        // x86-register SBB lowering normalizes CF around SBC. Unsigned condition
        // evaluation still expects AArch64's no-borrow convention and therefore
        // cannot consume canonical x86 CF without an equivalent normalization.
        if !uses.intersection(FlagSet::CF).is_empty()
            && !matches!(
                op.kind,
                OpKind::Adc { .. }
                    | OpKind::Sbb { .. }
                    | OpKind::Rcl { .. }
                    | OpKind::Rcr { .. }
                    | OpKind::X86Adx {
                        kind: X86AdxKind::Adcx,
                        ..
                    }
                    | OpKind::CmcCF
            )
        {
            return false;
        }

        let defs = x86_flag_defs(&op.kind);
        if !defs.intersection(unavailable).intersection(live).is_empty() {
            return false;
        }
        if !defs.intersection(FlagSet::CF).intersection(live).is_empty()
            && matches!(
                op.kind,
                OpKind::Sub { .. } | OpKind::Neg { .. } | OpKind::Cmp { .. }
            )
        {
            return false;
        }

        live = live.difference(defs).union(uses);
    }
    true
}
pub(crate) fn x86_aarch64_block_is_clobber_safe(
    block: &crate::smir::ir::SmirBlock,
    flags_live_out: crate::smir::ir::flags::FlagSet,
) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VReg;

    if !x86_aarch64_block_flags_are_representable(block, flags_live_out) {
        return false;
    }

    let folded_branch_cond = matches!(
        (&block.terminator, block.ops.last().map(|op| &op.kind)),
        (
            Terminator::CondBranch { cond, .. },
            Some(OpKind::TestCondition { dst, .. })
        ) if cond == dst
    );

    let n = block.ops.len();
    for (index, op) in block.ops.iter().enumerate() {
        if index + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    // The lowerer folds this virtual condition result directly
                    // into B.cond, so it does not consume a mapped host GPR.
                    continue;
                }
            }
        }

        if !x86_aarch64_scalar_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86RequireApx)
            && !crate::smir::lower::x86_64::x86_require_apx_shape_valid(op)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86RequireTbm)
            && !crate::smir::lower::x86_64::x86_require_tbm_shape_valid(op)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86Tbm { .. }) && op.x86_hint.is_some() {
            return false;
        }
        // AH/CH/DH/BH require x86 byte-lane extraction. The generic AArch64
        // register map sees only the parent GPR and cannot infer that lane from
        // the encoding hint, so retain interpreter fallback for these forms.
        if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
            return false;
        }
        // `/6` SAL carries an x86-only deterministic undefined-AF policy.
        // The AArch64 flag bridge cannot represent that distinction.
        if matches!(op.x86_hint, Some(X86OpHint::ShiftGroup6)) {
            return false;
        }
        if matches!(op.x86_hint, Some(X86OpHint::Mulx)) && !x86_mulx_arch_shape_valid(op) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::MulU {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            } | OpKind::MulS {
                dst_hi: Some(_),
                width: crate::smir::ir::types::OpWidth::W16,
                ..
            }
        ) && !x86_word_full_mul_shape_valid(&op.kind, false)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bsf { .. } | OpKind::Bsr { .. })
            && !x86_bit_scan_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Cwd { .. }) && !x86_cwd_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Rcl { .. } | OpKind::Rcr { .. })
            && !x86_carry_rotate_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::AndNot { .. }
                | OpKind::Bextr { .. }
                | OpKind::Bzhi { .. }
                | OpKind::X86Bls { .. }
                | OpKind::X86Tbm { .. }
                | OpKind::Pdep { .. }
                | OpKind::Pext { .. }
        ) && !x86_bmi_shape_valid(&op.kind)
            && !x86_aarch64_tbm_bextr_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::X86Adx { .. }) && !x86_adx_shape_valid(&op.kind) {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::Bt { .. } | OpKind::Bts { .. } | OpKind::Btr { .. } | OpKind::Btc { .. }
        ) && !x86_aarch64_bit_test_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(
            op.kind,
            OpKind::Clz { .. }
                | OpKind::Ctz { .. }
                | OpKind::Popcnt { .. }
                | OpKind::X86Count { .. }
        ) && !x86_count_shape_valid(&op.kind)
        {
            return false;
        }
        if matches!(op.kind, OpKind::Bswap { .. }) && !x86_bswap_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::Xchg { .. }) && !x86_xchg_shape_valid(&op.kind) {
            return false;
        }
        if matches!(op.kind, OpKind::X86NddDoubleShift { .. })
            && !x86_ndd_double_shift_shape_valid(&op.kind)
        {
            return false;
        }

        let source_is_representable = |source: &VReg| {
            x86_aarch64_legacy_gpr(source)
                || matches!(
                    &op.kind,
                    OpKind::Bextr {
                        control: VReg::Imm(control),
                        ..
                    } if source == &VReg::Imm(*control)
                )
        };
        if op
            .kind
            .dests()
            .iter()
            .any(|dst| !x86_aarch64_legacy_gpr(dst))
            || op
                .kind
                .source_vregs()
                .iter()
                .any(|source| !source_is_representable(source))
        {
            return false;
        }
    }

    // Terminator operands bypass `OpKind::{dests,source_vregs}`. Validate them
    // explicitly so an APX/virtual condition or switch index cannot read an
    // un-marshalled host X16+ register. The trailing TestCondition exception is
    // safe because the lowerer folds it directly into B.cond and never reads
    // its virtual destination.
    match &block.terminator {
        Terminator::Branch { .. } => true,
        Terminator::CondBranch { cond, .. } => {
            folded_branch_cond || matches!(cond, VReg::Imm(_)) || x86_aarch64_legacy_gpr(cond)
        }
        Terminator::Switch { index, .. } => {
            matches!(index, VReg::Imm(_)) || x86_aarch64_legacy_gpr(index)
        }
        Terminator::Return { values } => values.is_empty(),
        _ => false,
    }
}
pub(crate) fn x86_aarch64_legacy_gpr(vreg: &crate::smir::ir::types::VReg) -> bool {
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    matches!(
        vreg,
        VReg::Arch(ArchReg::X86(
            X86Reg::Rax
                | X86Reg::Rcx
                | X86Reg::Rdx
                | X86Reg::Rbx
                | X86Reg::Rsp
                | X86Reg::Rbp
                | X86Reg::Rsi
                | X86Reg::Rdi
                | X86Reg::R8
                | X86Reg::R9
                | X86Reg::R10
                | X86Reg::R11
                | X86Reg::R12
                | X86Reg::R13
                | X86Reg::R14
                | X86Reg::R15
        ))
    )
}

/// Validate the TBM subset whose AArch64 identity mapping can use X4/X5 for
/// guest RSP/RBP. The shared x86-host BMI validator deliberately excludes
/// those two registers because they are state-backed on an x86-64 host.
fn x86_aarch64_tbm_bextr_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, VReg};

    let tbm_flags = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    let bextr_flags = FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF);

    match op {
        OpKind::X86Tbm {
            dst,
            src,
            width: OpWidth::W32 | OpWidth::W64,
            flags,
            ..
        } => {
            x86_aarch64_legacy_gpr(dst)
                && x86_aarch64_legacy_gpr(src)
                && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(tbm_flags))
        }
        OpKind::Bextr {
            dst,
            src,
            control,
            width: OpWidth::W32 | OpWidth::W64,
            flags,
        } => {
            x86_aarch64_legacy_gpr(dst)
                && x86_aarch64_legacy_gpr(src)
                && (x86_aarch64_legacy_gpr(control) || matches!(control, VReg::Imm(_)))
                && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(bextr_flags))
        }
        _ => false,
    }
}

/// Architecture-specific scalar whitelist for the x86 VCPU identity bridge.
///
/// AArch64 W-register writes zero-extend. That is exact for x86 32-bit GPR
/// destinations, but not for 8/16-bit destinations, which preserve the upper
/// bits. Keep every destination-producing operation at W32/W64 unless its
/// lowering has a separately validated x86 partial-write implementation. This
/// explicit match also makes future additions to the shared x86-host whitelist
/// fail closed until their AArch64-host shape is reviewed.
pub(crate) fn x86_aarch64_scalar_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{FenceKind, OpWidth, SrcOperand};

    let full_gpr_write = |width: &OpWidth| matches!(width, OpWidth::W32 | OpWidth::W64);
    let scalar_read_width = |width: &OpWidth| {
        matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        )
    };

    match op {
        OpKind::Add { dst, width, .. }
        | OpKind::Sub { dst, width, .. }
        | OpKind::Adc { dst, width, .. }
        | OpKind::Sbb { dst, width, .. }
        | OpKind::Neg { dst, width, .. }
        | OpKind::Inc { dst, width, .. }
        | OpKind::Dec { dst, width, .. }
        | OpKind::And { dst, width, .. }
        | OpKind::Or { dst, width, .. }
        | OpKind::Xor { dst, width, .. }
        | OpKind::Shl { dst, width, .. }
        | OpKind::Shr { dst, width, .. }
        | OpKind::Sar { dst, width, .. }
        | OpKind::Rol { dst, width, .. }
        | OpKind::Ror { dst, width, .. }
        | OpKind::Rcl { dst, width, .. }
        | OpKind::Rcr { dst, width, .. } => {
            full_gpr_write(width)
                || (x86_aarch64_legacy_gpr(dst) && matches!(width, OpWidth::W8 | OpWidth::W16))
        }
        OpKind::X86NddDoubleShift {
            dst,
            amount,
            width,
            flags,
            ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && (!flags.updates_any()
                        || matches!(amount, SrcOperand::Imm(value) if (*value as u64 & 0x1f) <= 16)))
        }
        OpKind::Shld {
            dst,
            amount,
            width,
            flags,
            ..
        }
        | OpKind::Shrd {
            dst,
            amount,
            width,
            flags,
            ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && (!flags.updates_any()
                        || matches!(amount, SrcOperand::Imm(value) | SrcOperand::Imm64(value) if (*value as u64 & 0x1f) <= 16)))
        }
        OpKind::MulS {
            dst_lo,
            dst_hi,
            width,
            ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst_lo)
                    && dst_hi.as_ref().is_none_or(x86_aarch64_legacy_gpr))
        }
        OpKind::MulU {
            dst_lo,
            dst_hi: Some(dst_hi),
            width: OpWidth::W16,
            ..
        } => x86_aarch64_legacy_gpr(dst_lo) && x86_aarch64_legacy_gpr(dst_hi),
        OpKind::Bsf {
            dst, src, width, ..
        }
        | OpKind::Bsr {
            dst, src, width, ..
        }
        | OpKind::Clz { dst, src, width }
        | OpKind::Ctz { dst, src, width }
        | OpKind::Popcnt { dst, src, width } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && x86_aarch64_legacy_gpr(src))
        }
        OpKind::X86Count {
            dst, src, width, ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && x86_aarch64_legacy_gpr(src))
        }
        OpKind::Crc32C {
            dst,
            crc,
            data,
            data_width,
        } => {
            dst == crc
                && x86_aarch64_legacy_gpr(dst)
                && x86_aarch64_legacy_gpr(data)
                && matches!(
                    data_width,
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
                )
        }
        OpKind::AndNot { width, .. }
        | OpKind::MulU { width, .. }
        | OpKind::Bextr { width, .. }
        | OpKind::Bzhi { width, .. }
        | OpKind::X86Bls { width, .. }
        | OpKind::X86Tbm { width, .. }
        | OpKind::X86Adx { width, .. }
        | OpKind::Pdep { width, .. }
        | OpKind::Pext { width, .. }
        | OpKind::Bswap { width, .. } => full_gpr_write(width),
        OpKind::Mov { dst, width, .. } => {
            full_gpr_write(width)
                || (x86_aarch64_legacy_gpr(dst) && matches!(width, OpWidth::W8 | OpWidth::W16))
        }
        // Register SETcc is architecturally byte-sized. Legacy high-byte
        // destinations lift through virtual merge temporaries and are rejected
        // by the register/hint checks below; this arm admits low-byte forms.
        OpKind::SetCC { dst, width, .. } => {
            x86_aarch64_legacy_gpr(dst) && matches!(width, OpWidth::W8)
        }
        OpKind::CMove {
            dst, src, width, ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W16)
                    && x86_aarch64_legacy_gpr(dst)
                    && x86_aarch64_legacy_gpr(src))
        }
        OpKind::Not {
            dst, src, width, ..
        } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W8 | OpWidth::W16)
                    && dst == src
                    && x86_aarch64_legacy_gpr(dst))
        }
        OpKind::Xchg { reg1, reg2, width } => {
            full_gpr_write(width)
                || (matches!(width, OpWidth::W8 | OpWidth::W16)
                    && x86_aarch64_legacy_gpr(reg1)
                    && x86_aarch64_legacy_gpr(reg2))
        }
        OpKind::ZeroExtend {
            dst,
            from_width,
            to_width,
            ..
        }
        | OpKind::SignExtend {
            dst,
            from_width,
            to_width,
            ..
        } => {
            full_gpr_write(to_width)
                || (matches!((from_width, to_width), (OpWidth::W8, OpWidth::W16))
                    && x86_aarch64_legacy_gpr(dst))
        }
        // CWD/CDQ/CQO has dedicated x86 partial-write lowering and native
        // machine regressions for its W8/W16 merge behavior.
        OpKind::Cwd { width, .. } => scalar_read_width(width),
        OpKind::Cmp { width, .. } | OpKind::Test { width, .. } => scalar_read_width(width),
        OpKind::Bt { .. } | OpKind::Bts { .. } | OpKind::Btr { .. } | OpKind::Btc { .. } => {
            x86_aarch64_bit_test_shape_valid(op)
        }
        OpKind::TestCondition { .. }
        | OpKind::Lea { .. }
        | OpKind::SetCF { .. }
        | OpKind::CmcCF
        | OpKind::X86RequireApx
        | OpKind::X86RequireTbm
        | OpKind::Nop => true,
        OpKind::Fence {
            kind: FenceKind::InstructionSerialize,
        } => true,
        _ => false,
    }
}
pub(crate) fn x86_aarch64_bit_test_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{OpWidth, SrcOperand};

    let index_valid = |index: &SrcOperand| {
        matches!(index, SrcOperand::Imm(_) | SrcOperand::Imm64(_))
            || matches!(index, SrcOperand::Reg(reg) if x86_aarch64_legacy_gpr(reg))
    };
    match op {
        OpKind::Bt { src, index, width } => {
            x86_aarch64_legacy_gpr(src)
                && index_valid(index)
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        }
        OpKind::Bts {
            dst,
            src,
            index,
            width,
        }
        | OpKind::Btr {
            dst,
            src,
            index,
            width,
        }
        | OpKind::Btc {
            dst,
            src,
            index,
            width,
        } => {
            dst == src
                && x86_aarch64_legacy_gpr(dst)
                && index_valid(index)
                && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
        }
        _ => false,
    }
}
/// Decide whether AArch32-lifted scalar SMIR can execute through the AArch64
/// identity trampoline without exposing host-only state.
///
/// The default contract is deliberately register-only and AArch32-state
/// specific (A32 or T16/T32 without hidden instruction predication):
/// r0-r14 map to W0-W14, r15 is rejected because architectural PC reads are
/// pipeline-relative and writes are control flow, and every data result is
/// W32.  Flag-discarding comparison temporaries are accepted because the
/// lowerer maps them to WZR; all materialized virtual registers are rejected.
/// Direct internal branches are admitted. Conditional branches accept either
/// an AArch32 r0-r14 zero test (Thumb CBZ/CBNZ) or a final `TestCondition` whose
/// virtual destination is consumed only by the terminator; the AArch64 lowerer
/// respectively emits `CBZ`/`CBNZ` or folds the pair into `B.cond`. A direct
/// guest call is admitted only when its final operation writes the exact A32
/// or Thumb link value to r14; callers must pair this gate with
/// `Aarch64Lowerer::set_guest_call_exits(true)` so the call becomes a native
/// frontier exit. Direct and register BLX calls additionally carry an explicit
/// interworking target; callers must enable
/// `Aarch64Lowerer::set_guest_interworking_call_exits(true)`. BLX LR has an
/// exact W32 virtual snapshot before the r14 link write so the old target is
/// consumed in architectural order. A register-indirect terminator is admitted only for an
/// AArch32 r0-r14 target with no speculative target list; callers must pair it
/// with `Aarch64Lowerer::set_guest_indirect_exits(true)`, which records an
/// interworking dispatcher exit and exports target bit 0 as CPSR.T. CFG targets
/// must exist, phi nodes and locals are rejected, and frontier blocks named in
/// `excluded` must still be present for native-exit lowering. Predicated data
/// instructions, Thumb IT state, SIMD/VFP, and other CPSR fields remain
/// interpreter-only. Use
/// [`is_aarch32_aarch64_native_clobber_safe_excluding_with_mem`] to admit the
/// validated scalar memory-helper shapes.
pub fn is_aarch32_aarch64_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(func, excluded, false)
}
/// Memory-aware form of
/// [`is_aarch32_aarch64_native_clobber_safe_excluding`].
///
/// When `allow_mem` is true, scalar B1/B2/B4 loads/stores and B4 load/store
/// pairs are admitted only when every address component and value register is
/// AArch32 r0-r14. Scalar loads additionally admit a frozen absolute address in
/// the 32-bit guest domain for validated A32/T16/T32 literal forms; absolute
/// stores and pairs remain rejected. Pair destinations must be distinct.
/// Callers must pair this gate with `Aarch64Lowerer::set_mem_helpers(true)` and
/// `Aarch64Lowerer::set_mem_helper_addr_width(OpWidth::W32)`.
pub fn is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ArmReg, Condition, OpWidth, SrcOperand, VReg};
    use crate::smir::ir::{CallTarget, Terminator};

    if !func.locals.is_empty()
        || func.get_block(func.entry).is_none()
        || excluded.keys().any(|id| func.get_block(*id).is_none())
    {
        return false;
    }

    let mut block_ids = std::collections::HashSet::with_capacity(func.blocks.len());
    if func.blocks.iter().any(|block| !block_ids.insert(block.id)) {
        return false;
    }

    let target_exists = |target| func.get_block(target).is_some();
    let gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if *index < 15);
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .all(|block| {
            if !block.phis.is_empty() {
                return false;
            }

            let ordinary_ops_valid = |ops: &[crate::smir::ir::ops::SmirOp]| {
                ops.iter()
                    .all(|op| aarch32_aarch64_native_op_shape_valid(&op.kind, allow_mem))
            };

            match &block.terminator {
                Terminator::Return { values } => {
                    values.is_empty() && ordinary_ops_valid(&block.ops)
                }
                Terminator::Branch { target } => {
                    target_exists(*target) && ordinary_ops_valid(&block.ops)
                }
                Terminator::CondBranch {
                    cond,
                    true_target,
                    false_target,
                } => {
                    if gpr(cond) {
                        return target_exists(*true_target)
                            && target_exists(*false_target)
                            && ordinary_ops_valid(&block.ops);
                    }
                    let Some((test, prefix)) = block.ops.split_last() else {
                        return false;
                    };
                    let OpKind::TestCondition {
                        dst,
                        cond: condition,
                    } = &test.kind
                    else {
                        return false;
                    };
                    matches!(cond, VReg::Virtual(_))
                        && dst == cond
                        && !matches!(condition, Condition::Parity | Condition::NoParity)
                        && target_exists(*true_target)
                        && target_exists(*false_target)
                        && ordinary_ops_valid(prefix)
                }
                Terminator::Call {
                    target: CallTarget::GuestAddr(target),
                    args,
                    continuation,
                } => {
                    let Some(continuation_pc) = func
                        .get_block(*continuation)
                        .map(|continuation| continuation.guest_pc)
                    else {
                        return false;
                    };
                    let Some((link, prefix)) = block.ops.split_last() else {
                        return false;
                    };
                    let OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(link_pc),
                        width: OpWidth::W32,
                    } = &link.kind
                    else {
                        return false;
                    };
                    let arm_link = continuation_pc;
                    let thumb_link = continuation_pc | 1;
                    args.is_empty()
                        && *target <= u64::from(u32::MAX)
                        && *target & 1 == 0
                        && continuation_pc <= u64::from(u32::MAX)
                        && continuation_pc & 1 == 0
                        && *dst == VReg::Arch(ArchReg::Arm(ArmReg::X(14)))
                        && (*link_pc == arm_link as i64 || *link_pc == thumb_link as i64)
                        && ordinary_ops_valid(prefix)
                }
                Terminator::Call {
                    target: CallTarget::GuestAddrInterworking { addr, thumb },
                    args,
                    continuation,
                } => {
                    let Some(continuation_pc) = func
                        .get_block(*continuation)
                        .map(|continuation| continuation.guest_pc)
                    else {
                        return false;
                    };
                    let Some((link, prefix)) = block.ops.split_last() else {
                        return false;
                    };
                    let OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(link_pc),
                        width: OpWidth::W32,
                    } = &link.kind
                    else {
                        return false;
                    };
                    let expected_link = continuation_pc | u64::from(!*thumb);
                    args.is_empty()
                        && *addr <= u64::from(u32::MAX)
                        && if *thumb {
                            *addr & 1 == 0
                        } else {
                            *addr & 3 == 0
                        }
                        && continuation_pc <= u64::from(u32::MAX)
                        && continuation_pc & 1 == 0
                        && *dst == VReg::Arch(ArchReg::Arm(ArmReg::X(14)))
                        && *link_pc == expected_link as i64
                        && ordinary_ops_valid(prefix)
                }
                Terminator::Call {
                    target: CallTarget::IndirectInterworking(target),
                    args,
                    continuation,
                } => {
                    let Some(continuation_pc) = func
                        .get_block(*continuation)
                        .map(|continuation| continuation.guest_pc)
                    else {
                        return false;
                    };
                    if !args.is_empty()
                        || continuation_pc > u64::from(u32::MAX)
                        || continuation_pc & 1 != 0
                    {
                        return false;
                    }
                    let link_valid = |link: &crate::smir::ir::ops::SmirOp| {
                        matches!(
                            &link.kind,
                            OpKind::Mov {
                                dst: VReg::Arch(ArchReg::Arm(ArmReg::X(14))),
                                src: SrcOperand::Imm(link_pc),
                                width: OpWidth::W32,
                            } if *link_pc == continuation_pc as i64
                                || *link_pc == (continuation_pc | 1) as i64
                        )
                    };
                    match target {
                        VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if *index < 14 => {
                            let Some((link, prefix)) = block.ops.split_last() else {
                                return false;
                            };
                            link_valid(link) && ordinary_ops_valid(prefix)
                        }
                        VReg::Virtual(snapshot) => {
                            let [prefix @ .., snapshot_op, link] = block.ops.as_slice() else {
                                return false;
                            };
                            matches!(
                                &snapshot_op.kind,
                                OpKind::Mov {
                                    dst: VReg::Virtual(id),
                                    src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::X(14)))),
                                    width: OpWidth::W32,
                                } if id == snapshot
                            ) && link_valid(link)
                                && ordinary_ops_valid(prefix)
                        }
                        _ => false,
                    }
                }
                Terminator::IndirectBranch {
                    target,
                    possible_targets,
                } => possible_targets.is_empty() && gpr(target) && ordinary_ops_valid(&block.ops),
                Terminator::Switch { .. }
                | Terminator::IndirectBranchMem { .. }
                | Terminator::Call { .. }
                | Terminator::TailCall { .. }
                | Terminator::Trap { .. }
                | Terminator::Unreachable => false,
            }
        })
}
pub(crate) fn aarch32_aarch64_native_op_shape_valid(
    op: &crate::smir::ir::ops::OpKind,
    allow_mem: bool,
) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{
        Address, ArchReg, ArmReg, MemWidth, OpWidth, ShiftOp, SignExtend, SrcOperand, VReg,
    };

    let gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::Arm(ArmReg::X(index))) if *index < 15);
    let source = |src: &SrcOperand| match src {
        SrcOperand::Imm(_) | SrcOperand::Imm64(_) => true,
        SrcOperand::Reg(reg) => gpr(reg),
        SrcOperand::Shifted { reg, shift, amount } => {
            gpr(reg)
                && *amount < 32
                && !matches!(shift, ShiftOp::Rrx)
                && !(*amount == 0 && matches!(shift, ShiftOp::Lsr | ShiftOp::Asr))
        }
        SrcOperand::Extended { .. } => false,
    };
    let arithmetic_dst = |dst: &VReg, flags: &FlagUpdate| {
        gpr(dst) || (matches!(dst, VReg::Virtual(_)) && flags.updates_any())
    };
    let partial_nz = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF));
    let partial_nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
    let nzcv = FlagUpdate::Specific(FlagSet::NZCV);
    let register_address = |addr: &Address| match addr {
        Address::Direct(base) | Address::BaseOffset { base, .. } => gpr(base),
        Address::BaseIndexScale {
            base: Some(base),
            index,
            scale: 1 | 2 | 4 | 8,
            ..
        } => gpr(base) && gpr(index),
        _ => false,
    };
    let load_address = |addr: &Address| {
        register_address(addr)
            || matches!(addr, Address::Absolute(address) if *address <= u64::from(u32::MAX))
    };

    match op {
        OpKind::Nop => true,
        OpKind::Mov {
            dst,
            src,
            width: OpWidth::W32,
        } => gpr(dst) && source(src),
        OpKind::Add {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Sub {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => arithmetic_dst(dst, flags) && gpr(src1) && source(src2),
        OpKind::Adc {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Sbb {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => {
            arithmetic_dst(dst, flags)
                && gpr(src1)
                && match src2 {
                    SrcOperand::Imm(_) | SrcOperand::Imm64(_) => true,
                    SrcOperand::Reg(reg) => gpr(reg),
                    SrcOperand::Shifted { .. } | SrcOperand::Extended { .. } => false,
                }
        }
        OpKind::And {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Or {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Xor {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::AndNot {
            dst,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => {
            (*flags == FlagUpdate::None || *flags == partial_nz)
                && (gpr(dst) || (*flags == partial_nz && matches!(dst, VReg::Virtual(_))))
                && (gpr(src1)
                    || (*flags == partial_nz
                        && matches!(op, OpKind::AndNot { .. })
                        && matches!(src1, VReg::Imm(-1))
                        && matches!(src2, SrcOperand::Reg(_))))
                && source(src2)
        }
        OpKind::Not {
            dst,
            src,
            width: OpWidth::W32,
        }
        | OpKind::Clz {
            dst,
            src,
            width: OpWidth::W32,
        }
        | OpKind::Rbit {
            dst,
            src,
            width: OpWidth::W32,
        }
        | OpKind::Bswap {
            dst,
            src,
            width: OpWidth::W32,
        } => gpr(dst) && gpr(src),
        OpKind::ArmRegShift {
            dst,
            src,
            amount,
            shift,
            width: OpWidth::W32,
            flags,
        } => {
            gpr(dst)
                && gpr(src)
                && matches!(
                    shift,
                    ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
                )
                && (*flags == FlagUpdate::None || *flags == partial_nzc)
                && match amount {
                    SrcOperand::Imm(_) | SrcOperand::Imm64(_) => true,
                    SrcOperand::Reg(reg) => gpr(reg),
                    SrcOperand::Shifted { .. } | SrcOperand::Extended { .. } => false,
                }
        }
        OpKind::ArmDpRegShift {
            kind,
            dst,
            rn,
            rm,
            rs,
            shift,
            flags,
        } => {
            (dst.is_some() == kind.writes_result())
                && dst.as_ref().is_none_or(gpr)
                && (rn.is_some() == kind.uses_rn())
                && rn.as_ref().is_none_or(gpr)
                && gpr(rm)
                && gpr(rs)
                && matches!(
                    shift,
                    ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
                )
                && (*flags == FlagUpdate::None
                    || (kind.is_logical() && *flags == partial_nzc)
                    || (!kind.is_logical() && *flags == nzcv))
        }
        OpKind::Neg {
            dst,
            src,
            width: OpWidth::W32,
            flags,
        } => arithmetic_dst(dst, flags) && gpr(src),
        OpKind::SignExtend {
            dst,
            src,
            from_width: OpWidth::W8 | OpWidth::W16,
            to_width: OpWidth::W32,
        }
        | OpKind::ZeroExtend {
            dst,
            src,
            from_width: OpWidth::W8 | OpWidth::W16,
            to_width: OpWidth::W32,
        } => gpr(dst) && gpr(src),
        OpKind::Shl {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Shr {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Sar {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        }
        | OpKind::Ror {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W32,
            flags,
        } => {
            gpr(dst)
                && gpr(src)
                && ((*flags == FlagUpdate::None && (1..32).contains(amount))
                    || (*flags == partial_nzc
                        && !matches!(op, OpKind::Ror { .. })
                        && (1..=32).contains(amount)))
        }
        OpKind::MulU {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        }
        | OpKind::MulS {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width: OpWidth::W32,
            flags,
        } => {
            ((*flags == FlagUpdate::None && dst_hi.as_ref().is_none_or(gpr))
                || (*flags == partial_nz && dst_hi.is_none()))
                && gpr(dst_lo)
                && gpr(src1)
                && source(src2)
                && (*flags == partial_nz || dst_hi.as_ref() != Some(dst_lo))
        }
        OpKind::MulAdd {
            dst,
            acc,
            src1,
            src2,
            width: OpWidth::W32,
        }
        | OpKind::MulSub {
            dst,
            acc,
            src1,
            src2,
            width: OpWidth::W32,
        } => gpr(dst) && gpr(acc) && gpr(src1) && gpr(src2),
        OpKind::DivU {
            quot,
            rem: None,
            src1,
            src2,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        }
        | OpKind::DivS {
            quot,
            rem: None,
            src1,
            src2,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        } => gpr(quot) && gpr(src1) && source(src2),
        OpKind::Bfx {
            dst,
            src,
            lsb,
            width_bits,
            op_width: OpWidth::W32,
            ..
        } => {
            gpr(dst)
                && gpr(src)
                && *width_bits != 0
                && u16::from(*lsb) + u16::from(*width_bits) <= 32
        }
        OpKind::Bfi {
            dst,
            dst_in,
            src,
            lsb,
            width_bits,
            op_width: OpWidth::W32,
        } => {
            gpr(dst)
                && gpr(dst_in)
                && gpr(src)
                && *width_bits != 0
                && u16::from(*lsb) + u16::from(*width_bits) <= 32
        }
        OpKind::Load {
            dst,
            addr,
            width,
            sign,
        } => {
            allow_mem
                && gpr(dst)
                && load_address(addr)
                && matches!(
                    (width, sign),
                    (
                        MemWidth::B1 | MemWidth::B2,
                        SignExtend::Zero | SignExtend::Sign
                    ) | (MemWidth::B4, SignExtend::Zero)
                )
        }
        OpKind::Store {
            src,
            addr,
            width: MemWidth::B1 | MemWidth::B2 | MemWidth::B4,
        } => allow_mem && gpr(src) && register_address(addr),
        OpKind::LoadPair {
            dst1,
            dst2,
            addr,
            width: MemWidth::B4,
        } => allow_mem && dst1 != dst2 && gpr(dst1) && gpr(dst2) && register_address(addr),
        OpKind::StorePair {
            src1,
            src2,
            addr,
            width: MemWidth::B4,
        } => allow_mem && gpr(src1) && gpr(src2) && register_address(addr),
        _ => false,
    }
}
/// AArch64 analogue of [`is_native_clobber_safe_excluding`]: decide whether the
/// EXECUTED (non-exit) blocks of `func` are safe to run through the identity-map
/// AArch64 entry trampoline (`rax_a64_enter_native`). `excluded` holds the
/// native-exit (frontier) blocks, whose bodies never execute natively.
///
/// The identity map (guest `Xn` ⇒ host `Xn`) leaves every host GPR holding live
/// guest state, and the trampoline reserves host X18 (platform), X28 (state
/// pointer), X30 (link), and SP (host stack). So a block is unsafe if it:
///   1. uses a non-JIT-safe op (touches memory / has side effects / is
///      unvalidated) — except register-destination `Load`/`Store` when
///      `allow_mem` (they lower to MMU helper call-outs), and except `DivU`/
///      `DivS` which are clean on AArch64 (the shared [`OpKind::is_jit_safe`]
///      excludes them only to model x86's `#DE`);
///   2. writes a `VReg::Virtual` temporary (would alias a guest GPR); or
///   3. reads or writes guest X18/X28/X30/SP — a read is tolerated only as a
///      memory operand under `allow_mem` (the helper reads the frozen value
///      from the state struct, not the live host register).
/// A trailing `TestCondition` feeding the block's `CondBranch` is exempt (the
/// lowerer folds it into a `B.cond` and never materializes its dst).
pub fn is_aarch64_native_clobber_safe_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
    allow_mem: bool,
) -> bool {
    let blocks = func.blocks.iter().filter(|b| !excluded.contains_key(&b.id));
    let mut uses_fp_trampoline = false;
    let mut uses_mem_helper = false;
    for block in blocks {
        if !aarch64_block_is_clobber_safe(block, allow_mem) {
            return false;
        }
        for op in &block.ops {
            uses_fp_trampoline |= aarch64_op_needs_fp_trampoline(&op.kind);
            uses_mem_helper |= allow_mem && aarch64_mem_helper_op(&op.kind);
        }
    }
    // The FP trampoline keeps guest V0-V31/FPCR/FPSR live in host SIMD/FP
    // state for the whole region, while extern memory helpers may clobber the
    // AAPCS64 caller-saved subset. Keep those paths separate.
    !(uses_fp_trampoline && uses_mem_helper)
}
pub(crate) fn aarch64_mem_helper_op(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::Load { .. } | OpKind::Store { .. } | OpKind::VLoad { .. } | OpKind::VStore { .. }
    )
}
pub(crate) fn aarch64_fp_trampoline_vreg(vreg: &crate::smir::ir::types::VReg) -> bool {
    use crate::smir::ir::types::{ArchReg, ArmReg, VReg};

    matches!(
        vreg,
        VReg::Arch(ArchReg::Arm(ArmReg::V(_) | ArmReg::Fpcr | ArmReg::Fpsr))
    )
}
pub(crate) fn aarch64_fp_sysreg(reg: u32) -> bool {
    const SYSREG_FPCR: u32 = (3 << 14) | (3 << 11) | (4 << 7) | (4 << 3);
    const SYSREG_FPSR: u32 = SYSREG_FPCR | 1;

    matches!(reg, SYSREG_FPCR | SYSREG_FPSR)
}
pub(crate) fn aarch64_op_needs_fp_trampoline(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    let touches_raw_fp_sysreg = match op {
        OpKind::ReadSysReg { reg, .. } | OpKind::WriteSysReg { reg, .. } => aarch64_fp_sysreg(*reg),
        _ => false,
    };

    touches_raw_fp_sysreg
        || op.dests().iter().any(aarch64_fp_trampoline_vreg)
        || op.source_vregs().iter().any(aarch64_fp_trampoline_vreg)
}
pub(crate) fn aarch64_block_is_clobber_safe(
    block: &crate::smir::ir::SmirBlock,
    allow_mem: bool,
) -> bool {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ArmReg, VReg};

    // Host BRK/UDF would signal the emulator rather than deliver the guest
    // exception. Excluded frontier blocks are filtered by the caller, so every
    // trap that reaches this predicate must remain interpreter-only.
    if matches!(
        block.terminator,
        Terminator::Trap { .. } | Terminator::Unreachable
    ) {
        return false;
    }

    // Reserved host registers under the identity-map trampoline. A guest write to
    // any of these clobbers host platform/state/link/stack; a guest read returns
    // the host (not guest) value. X28 holds the live state pointer; X18 is the
    // macOS platform register; X30 is the trampoline link; SP is the host stack
    // (guest SP is never loaded).
    let touches_reserved = |v: &VReg| {
        matches!(
            v,
            VReg::Arch(ArchReg::Arm(ArmReg::X(18)))
                | VReg::Arch(ArchReg::Arm(ArmReg::X(28)))
                | VReg::Arch(ArchReg::Arm(ArmReg::X(30)))
                | VReg::Arch(ArchReg::Arm(ArmReg::Sp))
        )
    };

    let n = block.ops.len();
    for (i, op) in block.ops.iter().enumerate() {
        if i + 1 == n {
            if let (Terminator::CondBranch { cond, .. }, OpKind::TestCondition { dst, .. }) =
                (&block.terminator, &op.kind)
            {
                if dst == cond {
                    continue;
                }
            }
        }
        // These operations access x86-specific architectural state. The
        // AArch64 guest-state ABI has no corresponding fields or lowerers, so
        // their generic JIT-safety classification must not admit them here.
        if matches!(
            op.kind,
            OpKind::SetAC { .. }
                | OpKind::X86RequireApx
                | OpKind::X86RequireSse4a
                | OpKind::X86RequireTbm
                | OpKind::X86RequireXop
                | OpKind::X86CheckAlignmentAc { .. }
                | OpKind::X86XopPackedBit { .. }
                | OpKind::X86Sse4aBitfield { .. }
                | OpKind::X86Cli { .. }
                | OpKind::X86Sti { .. }
                | OpKind::X86Clts
                | OpKind::X86Msr(..)
                | OpKind::X86ReadControl { .. }
                | OpKind::X86Smsw(..)
                | OpKind::X86SystemSelectorStore(..)
                | OpKind::X86SystemSelectorLoad(..)
                | OpKind::X86SelectorVerify(..)
                | OpKind::X86SelectorQuery(..)
                | OpKind::X86FarJump(..)
                | OpKind::X86FarCall(..)
                | OpKind::X86FarReturn(..)
                | OpKind::X86FastSystemTransfer(..)
                | OpKind::X86Lmsw(..)
                | OpKind::X86DescriptorTableStore(..)
                | OpKind::X86DescriptorTableLoad(..)
                | OpKind::X86Invlpg(..)
                | OpKind::X86Invpcid(..)
                | OpKind::X86WriteControl { .. }
                | OpKind::X86ReadDebug { .. }
                | OpKind::X86WriteDebug { .. }
                | OpKind::X86X87Control { .. }
        ) {
            return false;
        }
        let mem_ok = allow_mem && aarch64_mem_helper_op(&op.kind);
        // AArch64-clean register-only ops that the x86-tuned `is_jit_safe`
        // whitelist omits: UDIV/SDIV never trap on AArch64 (no x86 `#DE`), and
        // CLZ/RBIT/REV(Bswap)/bitfield insert+extract are pure ALU ops the
        // native lowerer emits correctly (validated by the differential harness
        // in tests/suites/smir/lower/aarch64_native.rs). Admitting them lets the emulator JIT
        // real scalar loops that use them instead of deopting.
        let a64_ok = matches!(
            op.kind,
            OpKind::DivU { .. }
                | OpKind::DivS { .. }
                | OpKind::Clz { .. }
                | OpKind::Rbit { .. }
                | OpKind::Bswap { .. }
                | OpKind::Bfx { .. }
                | OpKind::Bfi { .. }
                // IEEE-exact / correctly-rounded scalar FP: lower to the native
                // f-ops and match the interpreter under default rounding (run via
                // the FP trampoline which marshals V0-V31 + FPCR/FPSR). The
                // directed-rounding/convert/min-max/fmov forms are deliberately
                // excluded (the lowerer has documented rounding/fusion deviations).
                | OpKind::FAdd { .. }
                | OpKind::FSub { .. }
                | OpKind::FMul { .. }
                | OpKind::FDiv { .. }
                | OpKind::FSqrt { .. }
                | OpKind::FAbs { .. }
                | OpKind::FNeg { .. }
                // NEON three-same vector arithmetic/logic the lowerer emits
                // natively (run via the V-register FP trampoline). Element-type/
                // arrangement forms the lowerer can't handle bail at lower time.
                | OpKind::VAdd { .. }
                | OpKind::VSub { .. }
                | OpKind::VMul { .. }
                | OpKind::VDiv { .. }
                | OpKind::VUnary { .. }
                | OpKind::VReduce { .. }
                | OpKind::VFMinMaxNm { .. }
                | OpKind::VPermute2 { .. }
                | OpKind::VTableLookup { .. }
                | OpKind::VMax { .. }
                | OpKind::VMin { .. }
                | OpKind::VAnd { .. }
                | OpKind::VOr { .. }
                | OpKind::VXor { .. }
                | OpKind::VFma { .. }
        );
        if !op.is_jit_safe() && !a64_ok && !mem_ok {
            return false;
        }
        if op
            .kind
            .dests()
            .iter()
            .any(|d| matches!(d, VReg::Virtual(_)))
        {
            return false;
        }
        if op.kind.dests().iter().any(touches_reserved) {
            return false;
        }
        if !mem_ok && op.kind.source_vregs().iter().any(touches_reserved) {
            return false;
        }
    }
    true
}
