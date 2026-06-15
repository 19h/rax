# 05. Flags

## 1. Model

SMIR uses lazy flags. Arithmetic and logical operations record enough information to compute flags later instead of eagerly materializing every flag after every instruction.

```text
FlagUpdate  = None | All | Specific(FlagSet)
FlagSet     = CF, ZF, SF, OF, PF, AF combinations
LazyFlags   = op, result, left, right, width, high
FlagState   = Option<LazyFlags> + MaterializedFlags
```

## 2. Materialized flags

`MaterializedFlags` contains:

```text
cf, zf, sf, of, pf, af, df
```

`to_rflags` and `from_rflags` convert to/from x86 RFLAGS. `to_nzcv` and `from_nzcv` convert to/from ARM NZCV. The ARM carry flag differs from x86 borrow conventions for subtraction; conversion code must retain that distinction.

## 3. Lazy operations

Lazy flag operations include arithmetic (`Add`, `Sub`, `Adc`, `Sbb`, `Inc`, `Dec`, `Neg`), logical, shifts/rotates, multiply, bit-test, and BMI-specific forms such as `Bextr` and `Bzhi`.

A lazy descriptor MUST store original operands when flags depend on original operands. For example, ADC and SBB store carry-in separately in `high` so carry/overflow/auxiliary flags are computed from original inputs, not from an already-folded operand.

## 4. Flag consumers

Consumers include:

- conditional branches;
- `CMove` / `SetCC` / `TestCondition`;
- explicit `ReadFlags` or `MaterializeFlags`;
- operations whose semantics depend on CF/OF/DF;
- runtime exit state when flags are live at a frontier.

## 5. Optimizer constraints

An optimization may drop or rewrite a flag-producing operation only when the relevant written flags are dead. The optimizer performs frontier-aware liveness so all flags are live when a region exits to the interpreter unless proven internal to the region.

## 6. Lowering strategies

A lowerer may:

1. rely on host flags directly, when host flag semantics exactly match the guest operation;
2. synthesize guest flags into `GuestRegs.rflags` or `Aarch64GuestRegs.nzcv`;
3. avoid materialization when a terminal branch can use the live host flags;
4. call helper/runtime code;
5. reject the region.

The x86 lowerer folds a trailing `TestCondition` feeding a `CondBranch` into a direct `Jcc` when safe, avoiding a virtual-temp write under the identity register map.

## 7. Host flag hygiene

Native trampolines must prevent guest control flags from leaking into host Rust execution. The x86 runtime explicitly sanitizes problematic host EFLAGS bits after running guest code.
