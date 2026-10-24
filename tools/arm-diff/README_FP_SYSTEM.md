# A32/T32 floating-point system oracle

This standalone oracle executes selected instructions in a bare-metal QEMU guest
and compares exact state with RAX. It requires Python3.10+, stable Rust, LLVM
(`llvm-mc`/`ld.lld`), and `qemu-system-arm`; no ARM libc/sysroot is required.

```sh
python3 tools/arm-diff/fp_system_oracle.py \
    --llvm-bin /path/to/llvm/bin \
    --work-dir /Users/int/dev/rax-audit-work/rax-a32-fp-system-v4 \
    --target-dir /Users/int/dev/rax-audit-work/rax-a32-fp-system-target
```

Cargo builds with stable Rust, `--locked --offline`, and
`--no-default-features --features x86_64-suite,smir-jit`. The runner links the
exact library artifact identified by Cargo. `--repo`, `--qemu` and `--timeout`
are available; `--llvm-bin` can also be supplied through `LLVM_BIN` or PATH.

Version4 executes231 instruction pairs in both states with32 operand samples
and64 RM/FZ/DN/AHP/FZ16 control profiles:946176 complete comparisons. Samples
include edge bits, seeded operands and initially sticky status. A32/T32 receive
identical inputs. The original144-pair register/numerical corpus is retained. The87 added pairs
cover scalar selection and additional scalar/vector floating operations.
GPR samples have a separate seed stream so prior D/FPSCR inputs remain intact.

Every case must execute and match. Missing tools, timeouts, guest failure,
missing completion markers, short output, source mutation or any mismatch fail
the command. The work directory retains the ELF, input/reference records,
per-instruction counts, mismatch details, command/version/timing records,
source fingerprints, tool hashes and linked-library SHA. A version4 ownership
marker prevents overwriting unrelated directories or older evidence.

The little-endian wire format is fixed:

| Record | Offset (bytes) | Field | Size (bytes) |
| --- | ---: | --- | ---: |
| Input | 0 | State:0 A32,1 T32 | 4 |
| Input | 4 | Instruction word; T32 high halfword first in the word | 4 |
| Input | 8 | FPSCR | 4 |
| Input | 12 | D0-D31 | 256 |
| Input | 268 | R0 | 4 |
| Input | 272 | R1 | 4 |
| Input | 276 | APSR.NZCVQ bits31:27; other bits zero | 4 |
| Result | 0 | D0-D31 | 256 |
| Result | 256 | FPSCR | 4 |
| Result | 260 | R0 | 4 |
| Result | 264 | R1 | 4 |
| Result | 268 | APSR.NZCVQ bits31:27; other bits zero | 4 |

Inputs/results are280/272 bytes, respectively:264929280/257359872 bytes for the
complete corpus. Guest RAM is512MiB. Generation streams input records; the
comparator uses O(N) host buffer memory and O(N) time for N cases with a fixed
register count. All sources and artifact hashes are recorded for reproduction.

The listed operations use registers only. The captured state is complete
D0-D31, FPSCR, R0/R1 and APSR.NZCVQ; other GPRs, PC, memory and exception delivery are not
covered. Exception enables remain clear. This is independent system emulation;
physical A32 hardware, unlisted instructions/profiles and other architectures
require separate validation. The existing Linux user-mode ARM oracle is unchanged.

A separate probe checks the default non-trapping FPSCR register contract:

```sh
python3 tools/arm-diff/fpscr_profile_oracle.py \
    --llvm-bin /path/to/llvm/bin \
    --work-dir /Users/int/dev/rax-audit-work/rax-fpscr-register-oracle
```

It independently checks MVFR0.FPTrap=0 and all32768 combinations of A32/T32
access,64 supported controls and256 exception/reserved-byte values. Bits15:8
read zero; rounding, DN, FZ, AHP, FZ16, NZCV, QC and sticky status remain intact.
Every command must succeed with the exact guest completion marker. Commands,
versions, timings and source/ELF hashes are retained. Optional trapped profiles
and physical A32 hardware are separate validation.

Version4 supplies condition flags independently of FPSCR. Each sample covers one
of the32 NZCVQ combinations; the guest writes APSR before the instruction and
captures it immediately afterward. This is required for VSEL, whose conditions
use APSR. The previous144 instruction pairs do not consume APSR condition flags;
removing the appended fields and new groups preserves their original records.
The version4 ownership marker prevents reuse of older protocol work directories.
