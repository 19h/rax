

## Dormant worker-factory idle-timeout requests (2026-10-19)

Class 2 accepts the measured installed native/WoW64 `LARGE_INTEGER` contract:
native64 exact length 8, WoW64 length at least 8, 4-byte alignment and an 8-byte
capture before typed handle/right 0x4 validation. Nonnegative values fail after
access checking. Negative values clamp to [-6,000,000,000,-10,000,000] signed
100 ns ticks (relative 600 s through 1 s); `i64::MIN` is handled without negation.
The optional normalized request is shared by aliases, preserves constructor
unknowns, and remains independent of classes 3/5/13/14. Failures preserve prior
state; cross-page guard consumption matches the native records. Active worker
scheduling is outside this dormant setter contract. Public Rust enum constructors
need the new field; opaque C API 1.14.0 and defaults are unchanged.

Evidence: 8,944 observations over four ABI profiles on one Windows ARM64 kernel,
14 rebound native corruptions rejected, a compiled 0-pass/3-failure regression,
six shared tests and seven native Windows tests, including actual NTDLL/WoW64
leaves and x86 20-byte return cleanup. ACTUAL_VALIDATION_INLINE
Actual retained-owner core: ACTUAL_CORE_BYTES bytes, SHA-256 `ACTUAL_CORE_SHA`.
ACTUAL_ORDINARY_RESULT
Full process objective remains active until ordinary guests exit successfully.
[Evidence, assumptions and limits](../../../docs/specifications/windows/native-worker-factory-idle-timeout/README.md).
