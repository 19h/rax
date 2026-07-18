//! scalar::hex tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::interpret::tests::*;

    /// Pins the load-bearing `:lib` fma behaviour: ties-AWAY rounding of a
    /// subnormal result (native `f32::mul_add` rounds ties-to-even and DIVERGES
    /// here), the spurious-overflow back-off to max-finite, and the inf-minus-inf
    /// flush to +0. Values derived from the reference sem (`sf_fma_lib`).
    #[test]
    fn smir_hex_sf_fma_lib_matches_sem() {
        // 2^-149 * 0.5 + 0 = 2^-150, exactly halfway between 0 and the smallest
        // subnormal 2^-149. ties-to-even -> 0x0; ties-away (`:lib`) -> 0x1.
        assert_eq!(
            hex_sf_fma_lib(0x0000_0001, 0x3f00_0000, 0x0000_0000, false),
            0x0000_0001
        );
        // Sanity: the native ties-to-even path would give 0 here.
        assert_eq!(
            f32::from_bits(0x0000_0001).mul_add(0.5, 0.0).to_bits(),
            0x0000_0000
        );

        // Spurious overflow (no infinite input): FLT_MAX * 4 + 0 -> +inf, which
        // is backed off to max-finite by a bit decrement (0x7f800000 - 1).
        let big = 0x7f7f_ffff; // FLT_MAX
        let four = 0x4080_0000; // 4.0
        assert_eq!(hex_sf_fma_lib(big, four, 0, false), 0x7f7f_ffff);

        // inf - inf -> flushed to +0 for the fms form: prod=+inf, c=+inf.
        // sffms computes c - prod; with prod=+inf and c=+inf this is the
        // inf-minus-inf case -> +0.0.
        assert_eq!(
            hex_sf_fma_lib(0x7f80_0000, 0x3f80_0000, 0x7f80_0000, true),
            0
        );

        // Plain finite case matches a single-rounded fma (no fixup fires).
        assert_eq!(
            hex_sf_fma_lib(
                0x4000_0000, /*2*/
                0x4040_0000, /*3*/
                0x3f80_0000, /*1*/
                false
            ),
            (7.0f32).to_bits()
        );
    }
