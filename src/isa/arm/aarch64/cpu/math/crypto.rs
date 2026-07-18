//! math::crypto tests

use super::*;
use crate::isa::arm::aarch64::cpu::*;

// ---- SHA-1 / SHA-256 primitives (FIPS-180, per ARM ASL) ----

/// Extract 32-bit element `e` from a 128-bit vector.
#[inline]
pub(crate) fn sha_elem(v: u128, e: u32) -> u32 {
    (v >> (e * 32)) as u32
}
/// Insert 32-bit element `e` into a 128-bit vector.
#[inline]
pub(crate) fn sha_set_elem(v: &mut u128, e: u32, x: u32) {
    let sh = e * 32;
    *v = (*v & !(0xFFFF_FFFFu128 << sh)) | ((x as u128) << sh);
}
/// SHAchoose: ((y EOR z) AND x) EOR z
#[inline]
pub(crate) fn sha_choose(x: u32, y: u32, z: u32) -> u32 {
    ((y ^ z) & x) ^ z
}
/// SHAmajority: (x AND y) OR ((x OR y) AND z)
#[inline]
pub(crate) fn sha_majority(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | ((x | y) & z)
}
/// SHAparity: x EOR y EOR z
#[inline]
pub(crate) fn sha_parity(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}
/// SHA256 compression hash update (4 rounds). `part1` selects which 128-bit
/// half (X for SHA256H, Y for SHA256H2) is returned, per the ASL SHA256hash.
pub(crate) fn sha256_hash(x_in: u128, y_in: u128, w: u128, part1: bool) -> u128 {
    let mut x = x_in;
    let mut y = y_in;
    for e in 0..4 {
        let chs = sha_choose(sha_elem(y, 0), sha_elem(y, 1), sha_elem(y, 2));
        let maj = sha_majority(sha_elem(x, 0), sha_elem(x, 1), sha_elem(x, 2));
        // SIGMA1(Y<31:0>) = ROR(y0,6) ^ ROR(y0,11) ^ ROR(y0,25)
        let y0 = sha_elem(y, 0);
        let sigma1 = y0.rotate_right(6) ^ y0.rotate_right(11) ^ y0.rotate_right(25);
        let t = sha_elem(y, 3)
            .wrapping_add(sigma1)
            .wrapping_add(chs)
            .wrapping_add(sha_elem(w, e));
        // X<127:96> = t + X<127:96>
        let x3 = t.wrapping_add(sha_elem(x, 3));
        sha_set_elem(&mut x, 3, x3);
        // SIGMA0(X<31:0>) = ROR(x0,2) ^ ROR(x0,13) ^ ROR(x0,22)
        let x0 = sha_elem(x, 0);
        let sigma0 = x0.rotate_right(2) ^ x0.rotate_right(13) ^ x0.rotate_right(22);
        // Y<127:96> = t + SIGMA0(X<31:0>) + maj
        sha_set_elem(&mut y, 3, t.wrapping_add(sigma0).wrapping_add(maj));
        // <Y, X> = ROL(Y : X, 32) over the 256-bit concatenation (Y high, X low).
        let carry = (y >> 96) as u32; // Y<127:96>
        let new_y = (y << 32) | (x >> 96);
        let new_x = (x << 32) | (carry as u128);
        x = new_x;
        y = new_y;
    }
    if part1 { x } else { y }
}
/// SHA1 hash update (4 rounds) for SHA1C/SHA1P/SHA1M. `f` is the round
/// function (choose / parity / majority). Returns the new X (V[d]).
pub(crate) fn sha1_hash(x_in: u128, y_in: u32, w: u128, f: fn(u32, u32, u32) -> u32) -> u128 {
    let mut x = x_in;
    let mut y = y_in;
    for e in 0..4 {
        let t = f(sha_elem(x, 1), sha_elem(x, 2), sha_elem(x, 3));
        y = y
            .wrapping_add(sha_elem(x, 0).rotate_left(5))
            .wrapping_add(t)
            .wrapping_add(sha_elem(w, e));
        // X<63:32> = ROL(X<63:32>, 30)
        let x1 = sha_elem(x, 1).rotate_left(30);
        sha_set_elem(&mut x, 1, x1);
        // <Y, X> = ROL(Y : X, 32): Y is 32 bits, X is 128 bits (160-bit rotate).
        let new_y = sha_elem(x, 3); // X<127:96>
        let new_x = ((x & ((1u128 << 96) - 1)) << 32) | (y as u128); // X<95:0> : Y
        y = new_y;
        x = new_x;
    }
    x
}
/// Apply the SM4 S-box to each of the four bytes of a 32-bit word.
pub(crate) fn sm4_sub(x: u32) -> u32 {
    let b = x.to_le_bytes();
    u32::from_le_bytes([
        SM4_SBOX[b[0] as usize],
        SM4_SBOX[b[1] as usize],
        SM4_SBOX[b[2] as usize],
        SM4_SBOX[b[3] as usize],
    ])
}
/// One SM4 round transform (4 sub-rounds). `key_or_const` supplies the four
/// 32-bit round inputs (round keys for SM4E, constants for SM4EKEY). `enc`
/// selects the encryption linear transform (ROL 2/10/18/24) vs the key
/// expansion transform (ROL 13/23).
pub(crate) fn sm4_rounds(mut rr: u128, key_or_const: u128, enc: bool) -> u128 {
    for index in 0..4 {
        let k = (key_or_const >> (index * 32)) as u32;
        let mut intval = (rr >> 96) as u32 ^ (rr >> 64) as u32 ^ (rr >> 32) as u32 ^ k;
        intval = sm4_sub(intval);
        intval = if enc {
            intval
                ^ intval.rotate_left(2)
                ^ intval.rotate_left(10)
                ^ intval.rotate_left(18)
                ^ intval.rotate_left(24)
        } else {
            intval ^ intval.rotate_left(13) ^ intval.rotate_left(23)
        };
        intval ^= rr as u32; // EOR roundresult<31:0>
        rr = (rr >> 32) | ((intval as u128) << 96);
    }
    rr
}
/// SM3 TT1/TT2 round transforms. `sel`: 0=TT1A, 1=TT1B, 2=TT2A, 3=TT2B.
/// `i` is the immediate lane index selecting the word of Vm.
pub(crate) fn sm3_tt(vd: u128, vn: u128, vm: u128, i: u32, sel: u32) -> u128 {
    let word = |v: u128, k: u32| (v >> (32 * k)) as u32;
    let d0 = word(vd, 0);
    let d1 = word(vd, 1);
    let d2 = word(vd, 2);
    let d3 = word(vd, 3);
    let wj = word(vm, i);
    let vn3 = word(vn, 3);
    let (tt, rot, mix) = match sel {
        0b00 => {
            // SM3TT1A
            let ss2 = vn3 ^ d3.rotate_left(12);
            let tt1 = d1 ^ (d3 ^ d2);
            (
                tt1.wrapping_add(d0).wrapping_add(ss2).wrapping_add(wj),
                9u32,
                false,
            )
        }
        0b01 => {
            // SM3TT1B (majority)
            let ss2 = vn3 ^ d3.rotate_left(12);
            let tt1 = (d3 & d1) | (d3 & d2) | (d1 & d2);
            (
                tt1.wrapping_add(d0).wrapping_add(ss2).wrapping_add(wj),
                9,
                false,
            )
        }
        0b10 => {
            // SM3TT2A
            let tt2 = d1 ^ (d3 ^ d2);
            (
                tt2.wrapping_add(d0).wrapping_add(vn3).wrapping_add(wj),
                19,
                true,
            )
        }
        _ => {
            // SM3TT2B
            let tt2 = (d3 & d2) | ((!d3) & d1);
            (
                tt2.wrapping_add(d0).wrapping_add(vn3).wrapping_add(wj),
                19,
                true,
            )
        }
    };
    let r0 = d1;
    let r1 = d2.rotate_left(rot);
    let r2 = d3;
    let r3 = if mix {
        tt ^ tt.rotate_left(9) ^ tt.rotate_left(17)
    } else {
        tt
    };
    (r0 as u128) | ((r1 as u128) << 32) | ((r2 as u128) << 64) | ((r3 as u128) << 96)
}
/// SM3PARTW1 message expansion.
pub(crate) fn sm3_partw1(vd: u128, vn: u128, vm: u128) -> u128 {
    let word = |v: u128, k: u32| (v >> (32 * k)) as u32;
    let vdn = vd ^ vn;
    let mut w = [0u32; 4];
    w[0] = word(vdn, 0) ^ word(vm, 1).rotate_left(15);
    w[1] = word(vdn, 1) ^ word(vm, 2).rotate_left(15);
    w[2] = word(vdn, 2) ^ word(vm, 3).rotate_left(15);
    for i in 0..4 {
        if i == 3 {
            w[3] = word(vdn, 3) ^ w[0].rotate_left(15);
        }
        w[i] = w[i] ^ w[i].rotate_left(15) ^ w[i].rotate_left(23);
    }
    (w[0] as u128) | ((w[1] as u128) << 32) | ((w[2] as u128) << 64) | ((w[3] as u128) << 96)
}
/// SM3PARTW2 message expansion.
pub(crate) fn sm3_partw2(vd: u128, vn: u128, vm: u128) -> u128 {
    let word = |v: u128, k: u32| (v >> (32 * k)) as u32;
    let mut tmp = [0u32; 4];
    for k in 0..4 {
        tmp[k as usize] = word(vn, k) ^ word(vm, k).rotate_left(7);
    }
    let mut r = [0u32; 4];
    for k in 0..4 {
        r[k] = word(vd, k as u32) ^ tmp[k];
    }
    let mut tmp2 = tmp[0].rotate_left(15);
    tmp2 = tmp2 ^ tmp2.rotate_left(15) ^ tmp2.rotate_left(23);
    r[3] ^= tmp2;
    (r[0] as u128) | ((r[1] as u128) << 32) | ((r[2] as u128) << 64) | ((r[3] as u128) << 96)
}
/// GF(2^8) multiply with the AES reduction polynomial (0x11b).
#[inline]
pub(crate) fn aes_gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}
#[inline]
pub(crate) fn aes_sub_bytes(state: u128, inverse: bool) -> u128 {
    let table = if inverse { &AES_INV_SBOX } else { &AES_SBOX };
    let mut b = state.to_le_bytes();
    for x in b.iter_mut() {
        *x = table[*x as usize];
    }
    u128::from_le_bytes(b)
}
/// AES ShiftRows on the column-major 16-byte state (or InvShiftRows).
#[inline]
pub(crate) fn aes_shift_rows(state: u128, inverse: bool) -> u128 {
    let s = state.to_le_bytes();
    let mut out = [0u8; 16];
    for r in 0..4usize {
        for c in 0..4usize {
            let src_c = if inverse {
                (c + 4 - r) % 4
            } else {
                (c + r) % 4
            };
            out[c * 4 + r] = s[src_c * 4 + r];
        }
    }
    u128::from_le_bytes(out)
}
/// AES MixColumns (or InvMixColumns) on the column-major 16-byte state.
#[inline]
pub(crate) fn aes_mix_columns(state: u128, inverse: bool) -> u128 {
    let s = state.to_le_bytes();
    let mut out = [0u8; 16];
    for c in 0..4usize {
        let a = [s[c * 4], s[c * 4 + 1], s[c * 4 + 2], s[c * 4 + 3]];
        let col = if inverse {
            [
                aes_gmul(a[0], 14) ^ aes_gmul(a[1], 11) ^ aes_gmul(a[2], 13) ^ aes_gmul(a[3], 9),
                aes_gmul(a[0], 9) ^ aes_gmul(a[1], 14) ^ aes_gmul(a[2], 11) ^ aes_gmul(a[3], 13),
                aes_gmul(a[0], 13) ^ aes_gmul(a[1], 9) ^ aes_gmul(a[2], 14) ^ aes_gmul(a[3], 11),
                aes_gmul(a[0], 11) ^ aes_gmul(a[1], 13) ^ aes_gmul(a[2], 9) ^ aes_gmul(a[3], 14),
            ]
        } else {
            [
                aes_gmul(a[0], 2) ^ aes_gmul(a[1], 3) ^ a[2] ^ a[3],
                a[0] ^ aes_gmul(a[1], 2) ^ aes_gmul(a[2], 3) ^ a[3],
                a[0] ^ a[1] ^ aes_gmul(a[2], 2) ^ aes_gmul(a[3], 3),
                aes_gmul(a[0], 3) ^ a[1] ^ a[2] ^ aes_gmul(a[3], 2),
            ]
        };
        out[c * 4..c * 4 + 4].copy_from_slice(&col);
    }
    u128::from_le_bytes(out)
}
