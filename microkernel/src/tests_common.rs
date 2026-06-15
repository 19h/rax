//! Architecture-neutral test surface.
//!
//! Everything here is pure integer/fixed-point work, so the expected values are
//! identical on x86_64, AArch64 and ARMv6 — the same anchors (computed offline)
//! validate all three. Most checks are also self-validating (cross-method
//! agreement or algebraic invariants) so they would catch a regression even if
//! an anchor were wrong.

use crate::fixed::{Fixed, Vec3};
use crate::harness::Harness;
use crate::mem::allocator;
use crate::nbody::{Body, NBodySimulation};

/// Fixed input buffer the hashing checks run over.
const DATA: &[u8] = b"RAX-microkernel/v2";

pub fn run(h: &mut Harness) {
    arithmetic(h);
    bitops(h);
    wide_muldiv(h);
    hashing(h);
    prng(h);
    sorting(h);
    memops(h);
    number_theory(h);
    control_flow(h);
    fixed_point(h);
    nbody(h);
    allocator_test(h);
}

// ---------------------------------------------------------------------------
fn arithmetic(h: &mut Harness) {
    h.group("arithmetic");

    // Width-correct wrapping across types.
    h.eq_u64("u8_wrap", (200u8.wrapping_add(100)) as u64, 44);
    h.eq_u64("u16_wrap", (60000u16.wrapping_add(10000)) as u64, 4464);
    h.eq_u64("u32_wrap", (0xFFFF_FFFFu32.wrapping_add(2)) as u64, 1);
    h.eq_u64("u64_wrap", u64::MAX.wrapping_add(3), 2);
    h.eq_i64("i64_min_wrap", i64::MIN.wrapping_sub(1), i64::MAX);

    // Signed/unsigned division & remainder.
    h.eq_i64("sdiv_round", (-7i64) / 2, -3);
    h.eq_i64("srem_sign", (-7i64) % 3, -1);
    h.eq_u64("udiv", 1_000_000_007u64 / 9973, 100_270);
    h.eq_u64("umod", 1_000_000_007u64 % 9973, 7297);

    // Checked / saturating / overflowing.
    h.check("checked_overflow", u32::MAX.checked_add(1).is_none());
    h.eq_u64("saturating", 250u8.saturating_add(50) as u64, 255);
    let (v, o) = 200u8.overflowing_mul(2);
    h.check("overflowing", v == 144 && o);

    // Mixed sign-extension.
    h.eq_u64("sext_i8", ((-2i8) as i64) as u64, 0xFFFF_FFFF_FFFF_FFFE);
    h.eq_u64("zext_u8", 0xFEu8 as u64, 0xFE);

    // A long fused expression (exercises the codegen scheduler).
    let mut acc = 0u64;
    for i in 1u64..=100 {
        acc = acc.wrapping_add(i.wrapping_mul(i)).rotate_left(1);
    }
    h.eq_u64("fused_loop", acc, fused_loop_ref());
}

fn fused_loop_ref() -> u64 {
    let mut acc = 0u64;
    for i in 1u64..=100 {
        acc = acc.wrapping_add(i.wrapping_mul(i)).rotate_left(1);
    }
    acc
}

// ---------------------------------------------------------------------------
fn bitops(h: &mut Harness) {
    h.group("bitops");
    let v = 0x0F0F_1234_ABCD_8000u64;
    h.eq_u64("count_ones", v.count_ones() as u64, 24);
    h.eq_u64("leading_zeros", v.leading_zeros() as u64, 4);
    h.eq_u64("trailing_zeros", v.trailing_zeros() as u64, 15);
    h.eq_u64("reverse_bits", v.reverse_bits(), 0x0001_B3D5_2C48_F0F0);
    h.eq_u64("swap_bytes", v.swap_bytes(), 0x0080_CDAB_3412_0F0F);
    h.eq_u64("rotate_left", v.rotate_left(12), (v << 12) | (v >> 52));
    h.eq_u64("rotate_right", v.rotate_right(20), (v >> 20) | (v << 44));

    // popcount of every byte 0..256 sums to 1024.
    let mut pc = 0u64;
    for b in 0u32..256 {
        pc += b.count_ones() as u64;
    }
    h.eq_u64("popcount_sum", pc, 1024);

    // Variable shifts.
    h.eq_u64("shl_var", 1u64 << 40, 0x0100_0000_0000);
    h.eq_u64("ashr_var", ((-1024i64) >> 4) as u64, (-64i64) as u64);

    // isolate-lowest-set-bit identity.
    let x = 0xABCD_0000u64;
    h.eq_u64("lowest_set", x & x.wrapping_neg(), 0x0001_0000);
}

// ---------------------------------------------------------------------------
fn wide_muldiv(h: &mut Harness) {
    h.group("wide_muldiv");
    let a = 0xDEAD_BEEF_CAFE_BABEu64;
    let b = 0x0123_4567_89AB_CDEFu64;
    let prod = (a as u128).wrapping_mul(b as u128);
    h.eq_u64("mul128_lo", prod as u64, 0x7EB6_89F4_EA44_7D62);
    h.eq_u64("mul128_hi", (prod >> 64) as u64, 0x00FD_5BDE_EEB2_A01D);

    // 128-bit division exercises __udivti3 / __umodti3. Reconstruct q*d+r==n
    // for two divisors so the result is self-validating.
    let big = 0x1234_5678_9ABC_DEF0_1122_3344_5566_7788u128;
    h.check("div128_pow2m1", {
        let d = 0xFFFF_FFFFu128;
        big / d * d + big % d == big
    });
    h.check("div128_prime", {
        let d = 1_000_000_007u128;
        big / d * d + big % d == big
    });

    // 64-bit division by non-power-of-two on every iteration.
    let mut x = 0xFFFF_FFFF_FFFF_FFFFu64;
    for d in 3u64..50 {
        x = x / d + x % d;
    }
    h.eq_u64("div_chain", x, div_chain_ref());
}

fn div_chain_ref() -> u64 {
    let mut x = 0xFFFF_FFFF_FFFF_FFFFu64;
    for d in 3u64..50 {
        x = x / d + x % d;
    }
    x
}

// ---------------------------------------------------------------------------
fn hashing(h: &mut Harness) {
    h.group("hashing");
    h.eq_u32("crc32", crc32(DATA), 0x251A_386E);
    h.eq_u64("fnv1a64", fnv1a64(DATA), 0x01BA_DCA5_4A86_CCE5);
    h.eq_u32("adler32", adler32(DATA), 0x3D66_068B);
}

fn crc32(buf: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in buf {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn fnv1a64(buf: &[u8]) -> u64 {
    let mut hsh = 0xcbf2_9ce4_8422_2325u64;
    for &b in buf {
        hsh ^= b as u64;
        hsh = hsh.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hsh
}

fn adler32(buf: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &c in buf {
        a = (a + c as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

// ---------------------------------------------------------------------------
fn prng(h: &mut Harness) {
    h.group("prng");
    // LCG (PCG multiplier), 1000 iterations from seed 1.
    let mut x = 1u64;
    for _ in 0..1000 {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
    h.eq_u64("lcg1000", x, 0xF517_FF66_DF0C_BEA9);

    // xorshift64, xor-accumulate 1000 outputs.
    let mut s = 0x0139_408d_cbbf_7a44u64;
    let mut acc = 0u64;
    for _ in 0..1000 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        acc ^= s;
    }
    h.eq_u64("xorshift_acc", acc, 0x3DCB_4629_97C4_5BAF);
    h.eq_u64("xorshift_final", s, 0x12EA_EAD2_6597_5125);
}

// ---------------------------------------------------------------------------
fn sorting(h: &mut Harness) {
    h.group("sorting");
    // Build a pseudo-random array.
    let mut base = [0u32; 64];
    let mut s = 0x2545_F491u32;
    for slot in base.iter_mut() {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        *slot = s % 10_000;
    }
    let sum0: u64 = base.iter().map(|&v| v as u64).sum();
    let xor0: u32 = base.iter().fold(0, |a, &v| a ^ v);

    let mut a = base;
    bubble_sort(&mut a);
    let mut b = base;
    insertion_sort(&mut b);
    let mut c = base;
    let n = c.len();
    quicksort(&mut c, 0, n as isize - 1);

    h.check("bubble_sorted", is_sorted(&a));
    h.check("insertion_sorted", is_sorted(&b));
    h.check("quicksort_sorted", is_sorted(&c));
    h.check("sorts_agree", a == b && b == c);
    // Permutation invariants (sum & xor preserved).
    let sum1: u64 = c.iter().map(|&v| v as u64).sum();
    let xor1: u32 = c.iter().fold(0, |acc, &v| acc ^ v);
    h.check("sort_permutation", sum1 == sum0 && xor1 == xor0);

    // Binary search on the sorted array.
    let key = c[40];
    h.check("bsearch_present", binary_search(&c, key).is_some());
    h.check("bsearch_absent", binary_search(&c, 10_001).is_none());
}

fn is_sorted(a: &[u32]) -> bool {
    a.windows(2).all(|w| w[0] <= w[1])
}

fn bubble_sort(a: &mut [u32]) {
    let n = a.len();
    for i in 0..n {
        for j in 0..n - 1 - i {
            if a[j] > a[j + 1] {
                a.swap(j, j + 1);
            }
        }
    }
}

fn insertion_sort(a: &mut [u32]) {
    for i in 1..a.len() {
        let key = a[i];
        let mut j = i;
        while j > 0 && a[j - 1] > key {
            a[j] = a[j - 1];
            j -= 1;
        }
        a[j] = key;
    }
}

fn quicksort(a: &mut [u32], lo: isize, hi: isize) {
    if lo >= hi {
        return;
    }
    let pivot = a[hi as usize];
    let mut i = lo - 1;
    for j in lo..hi {
        if a[j as usize] <= pivot {
            i += 1;
            a.swap(i as usize, j as usize);
        }
    }
    a.swap((i + 1) as usize, hi as usize);
    quicksort(a, lo, i);
    quicksort(a, i + 2, hi);
}

fn binary_search(a: &[u32], key: u32) -> Option<usize> {
    let (mut lo, mut hi) = (0isize, a.len() as isize - 1);
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let v = a[mid as usize];
        if v == key {
            return Some(mid as usize);
        } else if v < key {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
fn memops(h: &mut Harness) {
    h.group("memops");
    let src: [u8; 96] = core::array::from_fn(|i| (i as u8).wrapping_mul(31).wrapping_add(7));
    let mut dst = [0u8; 96];

    // memcpy intrinsic.
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), src.len());
    }
    h.check("memcpy", dst == src);

    // memmove with overlap (shift left by 8).
    let mut buf = src;
    unsafe {
        core::ptr::copy(buf.as_ptr().add(8), buf.as_mut_ptr(), 88);
    }
    h.check("memmove", buf[..88] == src[8..96]);

    // memset intrinsic.
    let mut z = [0xAAu8; 64];
    unsafe {
        core::ptr::write_bytes(z.as_mut_ptr(), 0, z.len());
    }
    h.check("memset", z.iter().all(|&b| b == 0));

    // memcmp via slice equality / ordering.
    h.check("memcmp_eq", src[..] == dst[..]);
    h.check("memcmp_lt", [1u8, 2, 3] < [1u8, 2, 4]);
}

// ---------------------------------------------------------------------------
fn number_theory(h: &mut Harness) {
    h.group("number_theory");

    // gcd / lcm with the algebraic invariant gcd*lcm == a*b.
    let (a, b) = (462u64, 1071u64);
    let g = gcd(a, b);
    let l = a / g * b;
    h.eq_u64("gcd", g, 21);
    h.check("lcm_invariant", g.wrapping_mul(l) == a.wrapping_mul(b));

    // Prime sieve cross-checked against trial division.
    let count = sieve_count(10_000);
    h.eq_usize("prime_count", count.0, 1229);
    h.eq_u64("prime_sum", count.1, 5_736_396);
    h.check("prime_methods_agree", trial_prime_count(10_000) == 1229);

    // Fibonacci: iterative anchor + fast-doubling cross-check.
    h.eq_u64("fib90_iter", fib_iter(90), 2_880_067_194_370_816_120);
    h.eq_u64("fib92_iter", fib_iter(92), 7_540_113_804_746_346_429);
    h.check("fib_methods_agree", fib_iter(92) == fib_fast(92));

    // Factorials (exact and wrapping).
    h.eq_u64("fact20", factorial(20), 2_432_902_008_176_640_000);
    h.eq_u64("fact25_wrap", factorial(25), 0x619F_B090_7BC0_0000);

    // Integer sqrt with the floor(sqrt) invariant.
    let r = isqrt(123_456_789);
    h.eq_u64("isqrt", r, 11_111);
    h.check(
        "isqrt_invariant",
        r * r <= 123_456_789 && (r + 1) * (r + 1) > 123_456_789,
    );

    // Collatz and Ackermann (recursion-heavy).
    h.eq_u64("collatz27", collatz_steps(27), 111);
    h.eq_u64("ackermann33", ackermann(3, 3), 61);
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn sieve_count(n: usize) -> (usize, u64) {
    let mut sieve = [true; 10_000];
    sieve[0] = false;
    sieve[1] = false;
    let mut i = 2;
    while i * i < n {
        if sieve[i] {
            let mut j = i * i;
            while j < n {
                sieve[j] = false;
                j += i;
            }
        }
        i += 1;
    }
    let mut count = 0;
    let mut sum = 0u64;
    for (k, &is_p) in sieve.iter().enumerate().take(n) {
        if is_p {
            count += 1;
            sum += k as u64;
        }
    }
    (count, sum)
}

fn trial_prime_count(n: usize) -> usize {
    let mut count = 0;
    for k in 2..n {
        let mut prime = true;
        let mut d = 2;
        while d * d <= k {
            if k % d == 0 {
                prime = false;
                break;
            }
            d += 1;
        }
        if prime {
            count += 1;
        }
    }
    count
}

fn fib_iter(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let t = a.wrapping_add(b);
        a = b;
        b = t;
    }
    a
}

fn fib_fast(n: u32) -> u64 {
    // Fast doubling.
    fn rec(n: u32) -> (u64, u64) {
        if n == 0 {
            return (0, 1);
        }
        let (a, b) = rec(n >> 1);
        let c = a.wrapping_mul(b.wrapping_mul(2).wrapping_sub(a));
        let d = a.wrapping_mul(a).wrapping_add(b.wrapping_mul(b));
        if n & 1 == 0 {
            (c, d)
        } else {
            (d, c.wrapping_add(d))
        }
    }
    rec(n).0
}

fn factorial(n: u64) -> u64 {
    let mut f = 1u64;
    for i in 1..=n {
        f = f.wrapping_mul(i);
    }
    f
}

fn isqrt(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn collatz_steps(mut n: u64) -> u64 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

fn ackermann(m: u64, n: u64) -> u64 {
    if m == 0 {
        n + 1
    } else if n == 0 {
        ackermann(m - 1, 1)
    } else {
        ackermann(m - 1, ackermann(m, n - 1))
    }
}

// ---------------------------------------------------------------------------
fn control_flow(h: &mut Harness) {
    h.group("control_flow");

    // Arithmetic series closed form vs explicit loop.
    let n = 1000u64;
    let mut s = 0u64;
    for i in 1..=n {
        s += i;
    }
    h.eq_u64("series_sum", s, n * (n + 1) / 2);

    // 3x3 integer matrix multiply (identity * M == M).
    let m = [[1i64, 2, 3], [4, 5, 6], [7, 8, 9]];
    let id = [[1i64, 0, 0], [0, 1, 0], [0, 0, 1]];
    let p = matmul3(&id, &m);
    h.check("matmul_identity", p == m);
    // (M*M) trace.
    let m2 = matmul3(&m, &m);
    h.eq_i64("matmul_trace", m2[0][0] + m2[1][1] + m2[2][2], 261);

    // Dynamic dispatch through a trait object (exercises vtables).
    let shapes: [&dyn Area; 2] = [&Square(4), &Rect(3, 5)];
    let total: i64 = shapes.iter().map(|s| s.area()).sum();
    h.eq_i64("dyn_dispatch", total, 16 + 15);

    // Iterator pipeline with filter/map/fold.
    let folded: u64 = (1u64..=50).filter(|x| x % 3 == 0).map(|x| x * x).sum();
    h.eq_u64("iter_pipeline", folded, iter_pipeline_ref());

    // Substring search and char counting.
    let hay = b"the quick brown fox jumps over the lazy dog";
    h.eq_usize("substr_find", find_sub(hay, b"fox").unwrap(), 16);
    h.eq_usize("char_count", hay.iter().filter(|&&c| c == b'o').count(), 4);
}

trait Area {
    fn area(&self) -> i64;
}
struct Square(i64);
struct Rect(i64, i64);
impl Area for Square {
    fn area(&self) -> i64 {
        self.0 * self.0
    }
}
impl Area for Rect {
    fn area(&self) -> i64 {
        self.0 * self.1
    }
}

fn matmul3(a: &[[i64; 3]; 3], b: &[[i64; 3]; 3]) -> [[i64; 3]; 3] {
    let mut r = [[0i64; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            let mut acc = 0;
            for k in 0..3 {
                acc += a[i][k] * b[k][j];
            }
            *cell = acc;
        }
    }
    r
}

fn iter_pipeline_ref() -> u64 {
    (1u64..=50).filter(|x| x % 3 == 0).map(|x| x * x).sum()
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    for i in 0..=hay.len() - needle.len() {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

// ---------------------------------------------------------------------------
fn fixed_point(h: &mut Harness) {
    h.group("fixed_point");

    // sqrt(x)^2 ~= x within a small fixed-point tolerance.
    for &n in &[2i64, 9, 16, 100, 1000] {
        let x = Fixed::from_int(n);
        let r = x.sqrt();
        let sq = r * r;
        let err = (sq - x).0.unsigned_abs();
        h.check("fixed_sqrt", err < Fixed::SCALE as u64); // < 1.0 absolute
    }

    // 3-4-5 triangle magnitude.
    let v = Vec3::new(Fixed::from_int(3), Fixed::from_int(4), Fixed::zero());
    h.eq_i64("vec3_magnitude", v.magnitude().to_int(), 5);

    // Distributivity: a*(b+c) == a*b + a*c in fixed point.
    let a = Fixed(0x0001_8000); // 1.5
    let b = Fixed(0x0002_4000); // 2.25
    let c = Fixed(0x0000_C000); // 0.75
    h.check("fixed_distributive", (a * (b + c)).0 == (a * b + a * c).0);
}

// ---------------------------------------------------------------------------
fn nbody(h: &mut Harness) {
    h.group("nbody");
    let (cksum_a, energy_a) = run_nbody(50);
    let (cksum_b, energy_b) = run_nbody(50);
    // Determinism: identical inputs -> identical checksum and energy.
    h.check(
        "nbody_deterministic",
        cksum_a == cksum_b && energy_a == energy_b,
    );
    // Energy stays a representable, bounded quantity (no div-by-zero blow-up).
    h.check(
        "nbody_energy_bounded",
        energy_a != i64::MAX >> Fixed::FRAC_BITS && energy_a.unsigned_abs() < 10_000_000,
    );
    // Emit for the host to compare ACROSS architectures.
    println!("NBODY_CKSUM={:#018x}", cksum_a);
}

fn run_nbody(steps: u32) -> (u64, i64) {
    let count = 8usize;
    let bodies: *mut Body = allocator().alloc::<Body>(count).expect("alloc bodies");
    for i in 0..count {
        let x_sign = if i < 4 { 1 } else { -1 };
        let y_sign = if i % 4 < 2 { 1 } else { -1 };
        let x = Fixed::from_int(((i % 4) as i64 + 1) * 25 * x_sign);
        let y = Fixed::from_int(((i % 4) as i64 + 1) * 25 * y_sign);
        unsafe {
            *bodies.add(i) = Body {
                pos: Vec3::new(x, y, Fixed::zero()),
                vel: Vec3::new(
                    Fixed::from_int(-y_sign),
                    Fixed::from_int(x_sign),
                    Fixed::zero(),
                ),
                mass: Fixed::from_int(10),
            };
        }
    }
    let mut sim = NBodySimulation::new(bodies, count);
    let dt = Fixed(Fixed::SCALE / 100);
    for _ in 0..steps {
        sim.step(dt);
    }
    (sim.checksum(), sim.total_energy().to_int())
}

// ---------------------------------------------------------------------------
fn allocator_test(h: &mut Harness) {
    h.group("allocator");
    let before = allocator().allocated_bytes();
    let a: *mut u64 = allocator().alloc::<u64>(128).expect("alloc a");
    let b: *mut u32 = allocator().alloc::<u32>(256).expect("alloc b");
    // Write patterns and read back.
    for i in 0..128 {
        unsafe { *a.add(i) = (i as u64).wrapping_mul(0x9E37_79B9) }
    }
    for i in 0..256 {
        unsafe { *b.add(i) = (i as u32) ^ 0xA5A5_A5A5 }
    }
    let mut ok = true;
    for i in 0..128 {
        ok &= unsafe { *a.add(i) } == (i as u64).wrapping_mul(0x9E37_79B9);
    }
    for i in 0..256 {
        ok &= unsafe { *b.add(i) } == (i as u32) ^ 0xA5A5_A5A5;
    }
    h.check("alloc_readback", ok);
    h.check(
        "alloc_distinct",
        a as usize + 128 * 8 <= b as usize || b as usize + 256 * 4 <= a as usize,
    );
    h.check("alloc_grew", allocator().allocated_bytes() > before);
    h.check(
        "alloc_within_capacity",
        allocator().allocated_bytes() <= allocator().capacity(),
    );
}
