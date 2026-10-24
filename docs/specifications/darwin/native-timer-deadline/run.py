#!/usr/bin/env python3
"""Measure native timer deadline conversion independently of the emulator."""

import collections
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
import tempfile


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    require(sys.platform == "darwin", "native macOS kernel required")
    folder = Path(__file__).resolve().parent
    root = folder.parents[3]
    target = root / "target"
    target.mkdir(exist_ok=True)
    source = folder / "native-timer-cancel.c"
    identity = hashlib.sha256(source.read_bytes()).hexdigest()
    # This probe owns only this fresh in-tree directory. All subprocesses finish
    # before TemporaryDirectory releases its binaries and compiler temporaries.
    with tempfile.TemporaryDirectory(
        prefix="native-timer-deadline-", dir=target
    ) as name:
        build = Path(name)
        temporary = build / "tmp"
        temporary.mkdir()
        env = dict(os.environ)
        for key in ("CCACHE_DIR", "SCCACHE_DIR"):
            env.pop(key, None)
        env.update(
            TMPDIR=str(temporary),
            TMP=str(temporary),
            TEMP=str(temporary),
            CCACHE_DISABLE="1",
            SCCACHE_DISABLE="1",
        )
        for arch, machine in (("arm64", 0x100000C), ("x86_64", 0x1000007)):
            frequency = int(
                subprocess.check_output(
                    ["arch", "-" + arch, "/usr/sbin/sysctl", "-n", "hw.tbfrequency"],
                    env=env,
                    text=True,
                    timeout=30,
                )
            )
            require(frequency > 0, "positive native hardware frequency")
            executable = build / ("probe-" + arch)
            subprocess.run(
                [
                    "xcrun",
                    "clang",
                    "-arch",
                    arch,
                    "-std=c11",
                    "-O1",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-o",
                    str(executable),
                    str(source),
                ],
                cwd=root,
                env=env,
                check=True,
                timeout=30,
            )
            binary = executable.read_bytes()
            require(
                struct.unpack_from("<II", binary) == (0xFEEDFACF, machine),
                "native producer Mach-O architecture",
            )
            for qos in ("default", "utility"):
                command = ["arch", "-" + arch, str(executable)]
                if qos == "utility":
                    command = ["/usr/sbin/taskpolicy", "-c", "utility", *command]
                result = subprocess.run(
                    command,
                    cwd=root,
                    env=env,
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=30,
                )
                rows = [json.loads(line) for line in result.stdout.splitlines()]
                require(len(rows) == 514, "complete bounded native capture")
                timebase, cases, terminal = rows[0], rows[1:-1], rows[-1]
                require(timebase["kind"] == "timebase", "native timebase record")
                numer, denom = timebase["numer"], timebase["denom"]
                require(numer > 0 and denom > 0, "positive native timebase")
                require(
                    terminal
                    == {"kind": "complete", "cases": 512, "errors": 0, "destroy": 0},
                    "successful calls and timer destruction",
                )
                require(
                    [row["ordinal"] for row in cases] == list(range(512)),
                    "case ordinals",
                )
                scale = frequency * numer
                units = 1_000_000_000 * denom
                quantum = (units + scale - 1) // scale
                deltas, mismatches = [], 0
                for row in cases:
                    require(
                        row["kind"] == "case"
                        and row["arm1"] == row["arm2"] == row["cancel"] == 0
                        and row["returned"] > 0,
                        "native arm/rearm/cancel success",
                    )
                    deadline, returned = row["deadline"], row["returned"]
                    delta = returned - deadline
                    require(delta == row["delta_ticks"], "native delta arithmetic")
                    deltas.append(delta)
                    roundtrip = (deadline * scale // units) * units // scale
                    mismatches += returned != roundtrip
                # Report only scalars, relative deltas and content identities.
                print(
                    json.dumps(
                        {
                            "arch": arch,
                            "qos": qos,
                            "kernel": platform.release(),
                            "hardware_frequency_hz": frequency,
                            "timebase_ns_per_tick": [numer, denom],
                            "hardware_period_reported_ticks_ceil": quantum,
                            "cases": len(cases),
                            "delta_histogram": dict(collections.Counter(deltas)),
                            "double_floor_mismatches": mismatches,
                            "outside_one_reported_tick": sum(
                                abs(value) > 1 for value in deltas
                            ),
                            "source_sha256": identity,
                            "binary_sha256": hashlib.sha256(binary).hexdigest(),
                        },
                        sort_keys=True,
                    ),
                    flush=True,
                )
                require(
                    all(-quantum <= value <= 0 for value in deltas),
                    "native one-period bound",
                )
    require(
        hashlib.sha256(source.read_bytes()).hexdigest() == identity,
        "producer source changed",
    )


if __name__ == "__main__":
    main()
