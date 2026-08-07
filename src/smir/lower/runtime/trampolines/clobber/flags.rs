//! Flag-clobber classification separated from the identity-map block gate.

pub(crate) fn x86_native_op_would_clobber_preserved_flags(
    op: &crate::smir::ir::ops::OpKind,
) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::Adc {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Sbb {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Shld {
            flags: FlagUpdate::None,
            ..
        } | OpKind::Shrd {
            flags: FlagUpdate::None,
            ..
        }
    )
}
