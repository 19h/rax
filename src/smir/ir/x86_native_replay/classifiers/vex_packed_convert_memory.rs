//! AVX VEX packed-conversion memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Semantic family encoded by one classic VEX packed conversion whose source
/// is memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexPackedConvertMemoryKind {
    FpPrecision {
        from: VecElementType,
        to: VecElementType,
    },
    IntToFp {
        fp_elem: VecElementType,
    },
    FpToInt {
        fp_elem: VecElementType,
        truncate: bool,
    },
}

/// One complete VEX packed-conversion memory encoding rewritten to consume a
/// precise-helper result from a borrowed low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexPackedConvertMemoryEncoding {
    pub(crate) kind: X86VexPackedConvertMemoryKind,
    pub(crate) destination: u8,
    pub(crate) scratch: u8,
    pub(crate) source_width: VecWidth,
    pub(crate) destination_width: VecWidth,
    pub(crate) operation_width: VecWidth,
    pub(crate) memory_size: u32,
    pub(crate) w: bool,
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86VexPackedConvertMemoryEncoding {
    pub(crate) fn lanes(self) -> u8 {
        match self.kind {
            X86VexPackedConvertMemoryKind::FpPrecision { from, to } => {
                (self.operation_width.bytes() / from.bytes().max(to.bytes())) as u8
            }
            X86VexPackedConvertMemoryKind::IntToFp { fp_elem }
            | X86VexPackedConvertMemoryKind::FpToInt { fp_elem, .. } => {
                (self.operation_width.bytes() / fp_elem.bytes().max(VecElementType::I32.bytes()))
                    as u8
            }
        }
    }

    pub(crate) fn transfer_width(self) -> VecWidth {
        if self.source_width == VecWidth::V256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        }
    }
}

impl X86InstructionBytes {
    /// Validate and rewrite one defined AVX VEX packed-conversion memory
    /// source.
    ///
    /// The admitted family is:
    ///
    /// - `VCVTPS2PD` and `VCVTPD2PS`;
    /// - `VCVTDQ2PS` and `VCVTDQ2PD`;
    /// - `VCVTPS2DQ`, `VCVTTPS2DQ`, `VCVTPD2DQ`, and `VCVTTPD2DQ`.
    ///
    /// Every form uses map 0F, reserves `VEX.vvvv=1111b`, specifies WIG, and
    /// defines both VEX.128 and VEX.256 memory sources. Complete
    /// ModR/M/SIB/displacement validation is delegated to the shared VEX
    /// parser. Segment and address-size prefixes are accepted because the
    /// lowerer evaluates the guest address before replay.
    pub(crate) fn vex_packed_convert_memory_encoding(
        &self,
    ) -> Option<X86VexPackedConvertMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.source1 != 0 {
            return None;
        }

        let kind = match (fields.opcode, fields.pp) {
            (0x5A, 0) => X86VexPackedConvertMemoryKind::FpPrecision {
                from: VecElementType::F32,
                to: VecElementType::F64,
            },
            (0x5A, 1) => X86VexPackedConvertMemoryKind::FpPrecision {
                from: VecElementType::F64,
                to: VecElementType::F32,
            },
            (0x5B, 0) => X86VexPackedConvertMemoryKind::IntToFp {
                fp_elem: VecElementType::F32,
            },
            (0xE6, 2) => X86VexPackedConvertMemoryKind::IntToFp {
                fp_elem: VecElementType::F64,
            },
            (0x5B, 1) => X86VexPackedConvertMemoryKind::FpToInt {
                fp_elem: VecElementType::F32,
                truncate: false,
            },
            (0x5B, 2) => X86VexPackedConvertMemoryKind::FpToInt {
                fp_elem: VecElementType::F32,
                truncate: true,
            },
            (0xE6, 3) => X86VexPackedConvertMemoryKind::FpToInt {
                fp_elem: VecElementType::F64,
                truncate: false,
            },
            (0xE6, 1) => X86VexPackedConvertMemoryKind::FpToInt {
                fp_elem: VecElementType::F64,
                truncate: true,
            },
            _ => return None,
        };

        let operation_width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let (source_width, destination_width) = match kind {
            X86VexPackedConvertMemoryKind::FpPrecision {
                from: VecElementType::F32,
                to: VecElementType::F64,
            }
            | X86VexPackedConvertMemoryKind::IntToFp {
                fp_elem: VecElementType::F64,
            } => (
                if fields.width_256 {
                    VecWidth::V128
                } else {
                    VecWidth::V64
                },
                operation_width,
            ),
            X86VexPackedConvertMemoryKind::FpPrecision {
                from: VecElementType::F64,
                to: VecElementType::F32,
            }
            | X86VexPackedConvertMemoryKind::FpToInt {
                fp_elem: VecElementType::F64,
                ..
            } => (operation_width, VecWidth::V128),
            X86VexPackedConvertMemoryKind::IntToFp {
                fp_elem: VecElementType::F32,
            }
            | X86VexPackedConvertMemoryKind::FpToInt {
                fp_elem: VecElementType::F32,
                ..
            } => (operation_width, operation_width),
            _ => return None,
        };

        let scratch = (0..8u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one VEX destination leaves seven low scratch registers");
        let register_instruction = self.vex_memory_with_register_source(scratch)?;

        Some(X86VexPackedConvertMemoryEncoding {
            kind,
            destination: fields.destination,
            scratch,
            source_width,
            destination_width,
            operation_width,
            memory_size: source_width.bytes(),
            w: fields.w,
            pp: fields.pp,
            opcode: fields.opcode,
            register_instruction,
        })
    }
}
