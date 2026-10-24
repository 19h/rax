#!/usr/bin/env python3
"""Check the selected FP register masks and a bounded QEMU system comparison.

Raw QEMU outputs are retained. Len/Stride and FPEXC.RES1 differences are recorded
separately; complete QEMU equivalence to the selected register profile is not claimed.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import struct
from fp_system_oracle import source_fingerprint
import subprocess
import time


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args: list[str], out: Path, name: str, manifest: dict) -> subprocess.CompletedProcess:
    start = time.monotonic()
    result = subprocess.run(args, cwd=out, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=120)
    (out / (name + ".stdout")).write_text(result.stdout)
    (out / (name + ".stderr")).write_text(result.stderr)
    manifest["commands"].append({"name": name, "args": args, "code": result.returncode,
                                 "seconds": time.monotonic() - start})
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return result


def reference(out: Path, here: Path, llvm: Path, qemu: str, manifest: dict) -> None:
    values = [0, 0xFFFFFFFF, 0xFFC8009F, 0x40000700]
    for bit in range(32):
        values.extend([1 << bit, (1 << bit) ^ 0xFFFFFFFF])
    rng = random.Random(0x43F00D9821)
    values.extend(rng.getrandbits(32) for _ in range(4096))
    assert len(values) == 4164
    (out / "input.bin").write_bytes(struct.pack("<4164I", *values))
    size = 16 + 2 * len(values) * 12
    source = (here / "fp_register_profile_guest.s").read_text()
    assert source.count("@@COUNT@@") == 1 and source.count("@@OUTPUT_BYTES@@") == 2
    (out / "guest.s").write_text(source.replace("@@COUNT@@", str(len(values)))
                                 .replace("@@OUTPUT_BYTES@@", str(size)))
    (out / "link.ld").write_text("ENTRY(_start)\nSECTIONS {. = 0x40010000; .text : {*(.text*)}}\n")
    (out / "results.bin").unlink(missing_ok=True)
    commands = [
        ("qemu-version", [qemu, "--version"]),
        ("llvm-version", [str(llvm / "llvm-mc"), "--version"]),
        ("assemble", [str(llvm / "llvm-mc"), "-triple=armv8.2a-none-eabi",
                      "-mattr=+neon,+fullfp16,+fp16fml", "-filetype=obj", "guest.s", "-o", "guest.o"]),
        ("link", [str(llvm / "ld.lld"), "-m", "armelf", "-T", "link.ld", "guest.o", "-o", "guest.elf"]),
        ("qemu", [qemu, "-machine", "virt", "-cpu", "max", "-accel", "tcg,thread=single",
                  "-m", "128M", "-nodefaults", "-display", "none", "-monitor", "none", "-serial", "none",
                  "-semihosting-config", "enable=on,target=native", "-kernel", str(out / "guest.elf")]),
    ]
    for name, args in commands:
        result = run(args, out, name, manifest)
        if result.returncode:
            raise RuntimeError(f"{name} failed with status {result.returncode}")
    qemu_text = (out / "qemu.stdout").read_text() + (out / "qemu.stderr").read_text()
    if "RAX_FP_REGISTER_PROFILE_COMPLETE" not in qemu_text.splitlines():
        raise RuntimeError("Guest did not confirm complete output and close")
    data = (out / "results.bin").read_bytes()
    if len(data) != size:
        raise RuntimeError("Incomplete reference records")
    manifest["qemu_feature_registers"] = dict(zip(["MVFR0", "MVFR1", "MVFR2", "ID_ISAR6"], [f"{value:08x}" for value in struct.unpack_from("<IIII", data)]))
    manifest["artifact_sha256"] = {name: digest(out / name) for name in ["input.bin", "results.bin", "guest.elf"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, required=True)
    parser.add_argument("--llvm-bin", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-arm")
    args = parser.parse_args()
    out = args.work_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    owner = out / ".rax-fp-register-profile"
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != "version=2\n"):
        raise ValueError("Use an empty directory or an owned version2 profile directory")
    owner.write_text("version=2\n")
    here = Path(__file__).resolve().parent
    manifest = {"version": 2, "cases": 8328, "commands": [],
                "tool_sha256": {name: digest(here / name) for name in
                                  ["fp_register_profile.py", "fp_register_profile_guest.s", "fp_register_profile_compare.rs"]},
                "independent_scope": "Common register fields only; raw Len/Stride and FPEXC.RES1 differences are retained, not treated as full profile equivalence."}
    manifest["source_fingerprint"] = source_fingerprint(args.repo.resolve())
    reference(out, here, args.llvm_bin.resolve(), args.qemu, manifest)
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(args.target_dir.resolve())
    env["CARGO_BUILD_JOBS"] = "1"
    build = ["cargo", "+stable", "build", "--locked", "--offline", "--lib", "--no-default-features", "--features", "x86_64-suite,smir-jit"]
    start = time.monotonic()
    with (out / "rax-build.log").open("w") as log:
        result = subprocess.run(build, cwd=args.repo.resolve(), env=env, stdout=log, stderr=subprocess.STDOUT, timeout=1200)
    manifest["commands"].append({"name": "rax-build", "args": build, "code": result.returncode, "seconds": time.monotonic() - start})
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError("RAX build failed")
    library = args.target_dir.resolve() / "debug/librax.rlib"
    before = digest(library)
    shutil.copyfile(library, out / "librax.rlib")
    assert digest(library) == digest(out / "librax.rlib") == before
    manifest["rax_library_sha256"] = before
    result = run(["rustc", "+stable", "--edition=2024", str(here / "fp_register_profile_compare.rs"),
                  "--extern", "rax=" + str(out / "librax.rlib"), "-L", "dependency=" + str(library.parent / "deps"),
                  "-o", str(out / "compare")], out, "runner-build", manifest)
    if result.returncode:
        raise RuntimeError("Comparator build failed")
    result = run([str(out / "compare"), str(out / "input.bin"), str(out / "results.bin")], out, "compare", manifest)
    report = json.loads(result.stdout)
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    assert manifest["source_fingerprint"] == source_fingerprint(args.repo.resolve())
    manifest["result"] = "match_local_profile_and_independent_common_fields" if result.returncode == 0 else "mismatch"
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(report))
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
