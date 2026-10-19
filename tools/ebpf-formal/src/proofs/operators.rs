//! Functionally consistent uninterpreted division and remainder.
//!
//! Bit-level SAT cannot prove two 64-bit (or 32-bit) divider circuits
//! equal, even for the same function applied twice to the same operands
//! (measured in the record). The engine (`rax::isa::ebpf::arith`) and the
//! specification (`spec::ops`) therefore compute each quotient and remainder
//! through a named primitive that is exactly the Rust operator, and the
//! direct-plane harnesses replace both with the function here. Each call
//! returns one symbolic result per distinct operand pair: equal operands
//! give equal results, and different operands give independent results.
//!
//! This over-approximates the real operator, so a proof under it holds for
//! the real operator too; it proves that both sides apply the same
//! operation to the same operands under the same zero-divisor and
//! signed-overflow guards, and use the result identically. Each function
//! also asserts the operand domain where the Rust operator is defined (a
//! non-zero divisor, and no `MIN / -1`), so an unguarded call still fails.

/// Distinct operand pairs per operator and harness (engine and spec).
const CAPACITY: usize = 4;

struct Table<A: Copy, R: Copy> {
    operands: [A; CAPACITY],
    results: [R; CAPACITY],
    len: usize,
}

impl<A: Copy + PartialEq, R: Copy + kani::Arbitrary> Table<A, R> {
    fn apply(&mut self, operands: A) -> R {
        let mut index = 0;
        while index < self.len {
            if self.operands[index] == operands {
                return self.results[index];
            }
            index += 1;
        }
        assert!(self.len < CAPACITY, "operator table capacity");
        let result = R::any();
        self.operands[self.len] = operands;
        self.results[self.len] = result;
        self.len += 1;
        result
    }
}

macro_rules! operator {
    ($name:ident, $table:ident, $int:ty, $signed:literal) => {
        static mut $table: Table<($int, $int), $int> = Table {
            operands: [(0, 0); CAPACITY],
            results: [0; CAPACITY],
            len: 0,
        };

        pub(crate) fn $name(dividend: $int, divisor: $int) -> $int {
            assert!(divisor != 0, "division or remainder by zero");
            if $signed {
                assert!(
                    !(dividend == <$int>::MIN && divisor == !0),
                    "signed division or remainder overflow"
                );
            }
            // SAFETY: Kani executes a harness on one thread, and no reference
            // to the table outlives this call.
            unsafe { (*(&raw mut $table)).apply((dividend, divisor)) }
        }
    };
}

operator!(udiv32, UDIV32, u32, false);
operator!(urem32, UREM32, u32, false);
operator!(udiv64, UDIV64, u64, false);
operator!(urem64, UREM64, u64, false);
operator!(sdiv32, SDIV32, i32, true);
operator!(srem32, SREM32, i32, true);
operator!(sdiv64, SDIV64, i64, true);
operator!(srem64, SREM64, i64, true);
