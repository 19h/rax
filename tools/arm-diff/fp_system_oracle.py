#!/usr/bin/env python3
"""Compare A32/T32 floating register state with an independent QEMU system guest.

Uses LLVM assembly and a bare-metal guest, without an ARM libc/sysroot or
qemu-user. Artifacts are retained in the reported work directory. Every case
must execute and match; a missing tool, timeout, incomplete output or mismatch
is a failure. This is independent system emulation, not native ARM execution.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
SEED = 0x71293E84C65A0FDB
CASE_BYTES = 276
RESULT_BYTES = 268
SAMPLES = 32
CONTROLS = 64
VERSION = 3
OWNER = ".rax-a32-fp-system-oracle"

EDGES = (
    0, 0xFFFFFFFFFFFFFFFF, 0x8000800080008000, 0x0001000100010001,
    0x03FF03FF03FF03FF, 0x0400040004000400, 0x3C003C003C003C00,
    0x7BFF7BFF7BFF7BFF, 0x7C007C007C007C00, 0xFC00FC00FC00FC00,
    0x7C017C017C017C01, 0x7E017E017E017E01, 0x7FFFFFFF80000000,
    0x7F800000FF800000, 0x7F8123457FC12345, 0x7FF0000000000000,
    0x7FF0000000000001, 0x7FF8123456789ABC, 0x0010000000000000,
    1, 0x3FF0000000000000, 0x3F8000003F800000,
    0x4100C1003E00BE00, 0x40003C0040003C00,
)

INSTRUCTIONS = (
    'vadd.f16 s0,s2,s4',
    'vsub.f16 s0,s2,s4',
    'vmul.f16 s0,s2,s4',
    'vdiv.f16 s0,s2,s4',
    'vmla.f16 s0,s2,s4',
    'vmls.f16 s0,s2,s4',
    'vfma.f16 s0,s2,s4',
    'vfms.f16 s0,s2,s4',
    'vsqrt.f16 s0,s2',
    'vrintr.f16 s0,s2',
    'vrintx.f16 s0,s2',
    'vrintz.f16 s0,s2',
    'vrinta.f16 s0,s2',
    'vrintn.f16 s0,s2',
    'vrintp.f16 s0,s2',
    'vrintm.f16 s0,s2',
    'vadd.f32 s0,s2,s4',
    'vsub.f32 s0,s2,s4',
    'vmul.f32 s0,s2,s4',
    'vdiv.f32 s0,s2,s4',
    'vmla.f32 s0,s2,s4',
    'vmls.f32 s0,s2,s4',
    'vfma.f32 s0,s2,s4',
    'vfms.f32 s0,s2,s4',
    'vsqrt.f32 s0,s2',
    'vrintr.f32 s0,s2',
    'vrintx.f32 s0,s2',
    'vrintz.f32 s0,s2',
    'vrinta.f32 s0,s2',
    'vrintn.f32 s0,s2',
    'vrintp.f32 s0,s2',
    'vrintm.f32 s0,s2',
    'vadd.f64 d0,d1,d2',
    'vsub.f64 d0,d1,d2',
    'vmul.f64 d0,d1,d2',
    'vdiv.f64 d0,d1,d2',
    'vmla.f64 d0,d1,d2',
    'vmls.f64 d0,d1,d2',
    'vfma.f64 d0,d1,d2',
    'vfms.f64 d0,d1,d2',
    'vsqrt.f64 d0,d1',
    'vrintr.f64 d0,d1',
    'vrintx.f64 d0,d1',
    'vrintz.f64 d0,d1',
    'vrinta.f64 d0,d1',
    'vrintn.f64 d0,d1',
    'vrintp.f64 d0,d1',
    'vrintm.f64 d0,d1',
    'vcvt.s32.f16 s0,s2',
    'vcvtr.s32.f16 s0,s2',
    'vcvta.s32.f16 s0,s2',
    'vcvtn.s32.f16 s0,s2',
    'vcvtp.s32.f16 s0,s2',
    'vcvtm.s32.f16 s0,s2',
    'vcvt.u32.f16 s0,s2',
    'vcvtr.u32.f16 s0,s2',
    'vcvta.u32.f16 s0,s2',
    'vcvtn.u32.f16 s0,s2',
    'vcvtp.u32.f16 s0,s2',
    'vcvtm.u32.f16 s0,s2',
    'vcvt.f16.s32 s0,s2',
    'vcvt.f16.u32 s0,s2',
    'vcvtb.f16.f32 s0,s2',
    'vcvtt.f16.f32 s0,s2',
    'vcvtb.f32.f16 s0,s2',
    'vcvtt.f32.f16 s0,s2',
    'vcvt.f16.f32 d0,q1',
    'vcvt.f32.f16 q0,d2',
    'vcvt.f64.f32 d0,s2',
    'vcvt.f32.f64 s0,d1',
    'vrecpe.f16 q0,q1',
    'vrsqrte.f16 q0,q1',
    'vrecps.f16 q0,q1,q2',
    'vrsqrts.f16 q0,q1,q2',
    'vrecpe.f32 q0,q1',
    'vrsqrte.f32 q0,q1',
    'vrecps.f32 q0,q1,q2',
    'vrsqrts.f32 q0,q1,q2',
    'vcvta.s16.f16 q0,q1',
    'vcvta.u16.f16 q0,q1',
    'vcvtn.s16.f16 q0,q1',
    'vcvtn.u16.f16 q0,q1',
    'vcvtp.s16.f16 q0,q1',
    'vcvtp.u16.f16 q0,q1',
    'vcvtm.s16.f16 q0,q1',
    'vcvtm.u16.f16 q0,q1',
    'vfmal.f16 d0,s4,s6',
    'vfmsl.f16 d0,s4,s6',
    'vfmal.f16 q0,d0,d4',
    'vfmsl.f16 q0,d0,d4',
    'vfmal.f16 d0,s4,s6[1]',
    'vfmsl.f16 d0,s4,s6[1]',
    'vfmal.f16 q0,d4,d6[3]',
    'vfmsl.f16 q0,d4,d6[3]',
    'vcvtb.f16.f64 s0,d1',
    'vcvtt.f16.f64 s0,d1',
    'vcvtb.f64.f16 d0,s2',
    'vcvtt.f64.f16 d0,s2',
    'vcvt.f16.s16 q0,q1',
    'vcvt.f16.u16 q0,q1',
    'vcvt.s16.f16 q0,q1',
    'vcvt.u16.f16 q0,q1',
    'vcvt.f32.s32 q8,q9',
    'vcvt.s32.f32 q8,q9',
    'vcvt.f16.s16 s0,s0,#8',
    'vcvt.s16.f16 s0,s0,#8',
    'vcvt.f16.u16 s0,s0,#8',
    'vcvt.u16.f16 s0,s0,#8',
    'vcvt.f16.s32 s0,s0,#8',
    'vcvt.s32.f16 s0,s0,#8',
    'vcvt.f16.u32 s0,s0,#8',
    'vcvt.u32.f16 s0,s0,#8',
    'vcvt.f32.s16 s0,s0,#8',
    'vcvt.s16.f32 s0,s0,#8',
    'vcvt.f32.u16 s0,s0,#8',
    'vcvt.u16.f32 s0,s0,#8',
    'vcvt.f32.s32 s0,s0,#8',
    'vcvt.s32.f32 s0,s0,#8',
    'vcvt.f32.u32 s0,s0,#8',
    'vcvt.u32.f32 s0,s0,#8',
    'vcvt.f64.s16 d0,d0,#8',
    'vcvt.s16.f64 d0,d0,#8',
    'vcvt.f64.u16 d0,d0,#8',
    'vcvt.u16.f64 d0,d0,#8',
    'vcvt.f64.s32 d0,d0,#8',
    'vcvt.s32.f64 d0,d0,#8',
    'vcvt.f64.u32 d0,d0,#8',
    'vcvt.u32.f64 d0,d0,#8',
    'vfma.f16 q0,q1,q2',
    'vfms.f16 q0,q1,q2',
    'vmaxnm.f16 q0,q1,q2',
    'vminnm.f16 q0,q1,q2',
    'vfma.f32 q0,q1,q2',
    'vfms.f32 q0,q1,q2',
    'vmaxnm.f32 q0,q1,q2',
    'vminnm.f32 q0,q1,q2',
    'vcvt.f16.s16 q0,q1,#8',
    'vcvt.f16.u16 q0,q1,#8',
    'vcvt.s16.f16 q0,q1,#8',
    'vcvt.u16.f16 q0,q1,#8',
    'vmovx.f16 s0,s2',
    'vins.f16 s0,s2',
    'vmov.f16 s0,r0',
    'vmov.f16 r0,s2',
)


def digest(path: Path) -> str:
    state = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            state.update(chunk)
    return state.hexdigest()


def source_fingerprint(repo: Path) -> dict:
    state = hashlib.sha256()
    files = sorted((repo / "src").rglob("*.rs"))
    files += [repo / name for name in ("Cargo.toml", "Cargo.lock", "build.rs")
              if (repo / name).is_file()]
    for path in files:
        state.update(path.relative_to(repo).as_posix().encode() + b"\0")
        state.update(bytes.fromhex(digest(path)))
    return {"rust_and_build_sources_sha256": state.hexdigest(), "files": len(files)}


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def command(args: list[str], work: Path, name: str, manifest: dict,
            env: dict | None = None, timeout: float | None = None,
            cwd: Path | None = None) -> str:
    """Capture each complete command and its outcome, including failures."""
    start = time.monotonic()
    record = {"name": name, "args": args, "cwd": str(cwd or work)}
    manifest["commands"].append(record)
    try:
        with (work / (name + ".log")).open("w") as errors:
            result = subprocess.run(args, cwd=cwd or work, env=env, text=True,
                                    stdout=subprocess.PIPE, stderr=errors,
                                    timeout=timeout, check=False)
        (work / (name + ".stdout")).write_text(result.stdout)
        record["returncode"] = result.returncode
        if result.returncode:
            raise RuntimeError(f"{name} failed with status {result.returncode}")
        return result.stdout
    except subprocess.TimeoutExpired:
        record["returncode"] = "timeout"
        raise RuntimeError(f"{name} exceeded {timeout} s") from None
    finally:
        record["elapsed_seconds"] = time.monotonic() - start
        write_json(work / "manifest.json", manifest)


def make_cases(work: Path, llvm: Path, manifest: dict) -> list[dict]:
    if len(INSTRUCTIONS) != 144:
        raise ValueError("Version 3 requires exactly 144 instruction pairs")
    source = ".syntax unified\n.arm\n" + "\n".join(INSTRUCTIONS)
    source += "\n.thumb\n" + "\n".join(INSTRUCTIONS) + "\n"
    (work / "instructions.s").write_text(source)
    text = command([str(llvm / "llvm-mc"), "-triple=armv8.2a-none-eabi",
                    "-mattr=+neon,+fullfp16,+fp16fml", "-show-encoding",
                    str(work / "instructions.s")], work, "encode", manifest)
    encodings = re.findall(r"encoding: \[([^\]]+)\]", text)
    if len(encodings) != 2 * len(INSTRUCTIONS):
        raise ValueError("LLVM did not encode every A32 and T32 instruction")
    metadata = []
    count = 0
    with (work / "cases.bin").open("wb") as output:
        for group, encoding in enumerate(encodings):
            raw = bytes(int(part, 16) for part in encoding.split(","))
            if len(raw) != 4:
                raise ValueError("Corpus requires four-byte instructions")
            thumb = group >= len(INSTRUCTIONS)
            word = ((int.from_bytes(raw[:2], "little") << 16)
                    | int.from_bytes(raw[2:], "little")) if thumb else int.from_bytes(raw, "little")
            op_index = group % len(INSTRUCTIONS)
            # The same operation receives identical input states in A32/T32.
            rng = random.Random(SEED ^ op_index)
            gpr_rng = random.Random(SEED ^ op_index ^ 0x9E3779B97F4A7C15)
            first = count
            for sample in range(SAMPLES):
                for c in range(CONTROLS):
                    control = ((c & 3) << 22 | ((c >> 2) & 1) << 24
                               | ((c >> 3) & 1) << 25 | ((c >> 4) & 1) << 26
                               | ((c >> 5) & 1) << 19 | 0xA8000000)
                    if sample >= 28:
                        control |= (0, 1, 0x80, 0x9F)[sample - 28]
                    regs = [EDGES[(sample + j) % len(EDGES)]
                            if sample < len(EDGES) else rng.getrandbits(64)
                            for j in range(32)]
                    gprs = ((0, 0xFFFFFFFF, 0x80000000, 0x1234ABCD)[sample % 4],
                            gpr_rng.getrandbits(32))
                    output.write(struct.pack("<III32QII", int(thumb), word, control, *regs, *gprs))
                    count += 1
            metadata.append({"thumb": thumb, "word": f"{word:08x}",
                             "instruction": INSTRUCTIONS[op_index],
                             "first_case": first, "cases": count - first})
    (work / "cases.inc").write_text(f'.equ CASE_COUNT,{count}\ncases:\n.incbin "cases.bin"\n')
    manifest["corpus"] = {"version": VERSION, "seed": hex(SEED), "cases": count,
                          "instruction_pairs": len(INSTRUCTIONS), "samples": SAMPLES,
                          "control_profiles": CONTROLS, "case_bytes": CASE_BYTES,
                          "result_bytes": RESULT_BYTES}
    write_json(work / "corpus.json", metadata)
    return metadata


def run_guest(work: Path, here: Path, llvm: Path, qemu: str, timeout: float,
              manifest: dict) -> None:
    output = work / "results.bin"
    output.unlink(missing_ok=True)
    filename = ",".join(str(byte) for byte in os.fsencode(output))
    guest = (here / "fp_system_guest.s").read_text()
    if guest.count("@@FILENAME_BYTES@@") != 1:
        raise ValueError("Guest filename placeholder must occur exactly once")
    (work / "guest.s").write_text(guest.replace("@@FILENAME_BYTES@@", filename))
    (work / "link.ld").write_text("""ENTRY(_start)
PHDRS { ram PT_LOAD FLAGS(7); }
SECTIONS {
    . = 0x40010000;
    .text : { *(.text*) *(.rodata*) *(.data*) } :ram
    .bss : { *(.bss*) *(COMMON) } :ram
}
""")
    command([str(llvm / "llvm-mc"), "-triple=armv8.2a-none-eabi",
             "-mattr=+neon,+fullfp16,+fp16fml", "-filetype=obj", "guest.s",
             "-o", "guest.o"], work, "assemble", manifest)
    command([str(llvm / "ld.lld"), "-m", "armelf", "-T", "link.ld",
             "guest.o", "-o", "guest.elf"], work, "link", manifest)
    stdout = command([qemu, "-machine", "virt", "-cpu", "max", "-accel",
                      "tcg,thread=single", "-m", "256M", "-nodefaults", "-display",
                      "none", "-monitor", "none", "-serial", "none",
                      "-semihosting-config", "enable=on,target=native",
                      "-kernel", str(work / "guest.elf")],
                     work, "qemu", manifest, timeout=timeout)
    # QEMU semihosting writes its console to stderr; both streams are retained.
    console = stdout + (work / "qemu.log").read_text()
    if "RAX_A32_CORPUS_OK" not in console.splitlines():
        raise ValueError("QEMU did not report completion of the complete corpus")
    expected = manifest["corpus"]["cases"] * RESULT_BYTES
    if not output.is_file() or output.stat().st_size != expected:
        raise ValueError(f"QEMU result length must be exactly {expected} bytes")


def build_runner(work: Path, here: Path, repo: Path, target: Path,
                 manifest: dict) -> Path:
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    env.setdefault("CARGO_BUILD_JOBS", "1")
    # The standalone runner must link a library for this host.
    env.pop("CARGO_BUILD_TARGET", None)
    build = command(["cargo", "+stable", "build", "--locked", "--offline", "--lib",
                     "--no-default-features", "--features", "x86_64-suite,smir-jit",
                     "--message-format=json-render-diagnostics"],
                    work, "rax-build", manifest, env=env, cwd=repo)
    artifacts = []
    for line in build.splitlines():
        event = json.loads(line)
        if (event.get("reason") == "compiler-artifact"
                and event.get("target", {}).get("name") == "rax"
                and "lib" in event["target"]["kind"]):
            artifacts.extend(Path(name) for name in event["filenames"] if name.endswith(".rlib"))
    if len(artifacts) != 1:
        raise ValueError("Cargo must identify exactly one rax library artifact")
    library = artifacts[0]
    dependencies = library.parent if library.parent.name == "deps" else library.parent / "deps"
    library_hash = digest(library)
    runner = work / "compare"
    command(["rustc", "+stable", "--edition=2024", str(here / "fp_system_compare.rs"),
             "--extern", "rax=" + str(library), "-L", "dependency=" + str(dependencies),
             "-o", str(runner)], work, "runner-build", manifest, env=env)
    if digest(library) != library_hash:
        raise ValueError("RAX library artifact changed while linking the runner")
    manifest["rax_library_sha256"] = library_hash
    return runner


def compare(work: Path, runner: Path, metadata: list[dict], manifest: dict) -> int:
    text = command([str(runner), str(work / "cases.bin"), str(work / "results.bin")],
                   work, "compare", manifest)
    rows = []
    for line, meta in zip(text.splitlines(), metadata, strict=True):
        group, count, bad, registers, flags, undefined = map(int, line.split())
        if (group != len(rows) or count != meta["cases"]
                or not 0 <= bad <= count
                or any(not 0 <= value <= bad for value in (registers, flags, undefined))):
            raise ValueError("Invalid or incomplete comparison counters")
        rows.append(dict(meta, failed=bad, register_mismatches=registers,
                         fpscr_mismatches=flags, undefined=undefined))
    total = sum(row["cases"] for row in rows)
    failed = sum(row["failed"] for row in rows)
    if total != manifest["corpus"]["cases"]:
        raise ValueError("Comparison did not execute the complete corpus")
    write_json(work / "report.json", {"cases": total, "passed": total - failed,
                                      "failed": failed, "rows": rows})
    print(f"{total - failed}/{total} complete A32/T32 FP state comparisons match")
    print(f"Evidence: {work}")
    return 1 if failed else 0


def main() -> int:
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=here.parents[1])
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--llvm-bin", type=Path)
    parser.add_argument("--qemu", default="qemu-system-arm")
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    repo = args.repo.resolve()
    work = (args.work_dir or Path(tempfile.mkdtemp(prefix="rax-a32-fp-oracle-"))).resolve()
    work.mkdir(parents=True, exist_ok=True)
    marker = work / OWNER
    ownership = f"version={VERSION}\n"
    if any(work.iterdir()) and (not marker.is_file() or marker.read_text() != ownership):
        raise ValueError("Use an empty work directory or an existing version 3 oracle directory")
    marker.write_text(ownership)
    if args.timeout <= 0:
        raise ValueError("Guest timeout must be positive")
    mc = shutil.which("llvm-mc")
    llvm = args.llvm_bin or (Path(os.environ["LLVM_BIN"]) if "LLVM_BIN" in os.environ
                             else Path(mc).parent if mc else None)
    if llvm is None:
        raise ValueError("llvm-mc was not found; provide --llvm-bin")
    llvm = llvm.resolve()
    target = (args.target_dir or Path(os.environ.get("CARGO_TARGET_DIR", repo / "target"))).resolve()
    manifest = {"version": VERSION, "commands": [], "repo": str(repo),
                "work_dir": str(work), "target_dir": str(target)}
    tool_names = ("fp_system_oracle.py", "fp_system_guest.s", "fp_system_compare.rs")
    manifest["tool_sha256"] = {name: digest(here / name) for name in tool_names}
    manifest["source_fingerprint"] = source_fingerprint(repo)
    try:
        manifest["revision"] = command(["git", "rev-parse", "HEAD"], work, "revision", manifest,
                                       cwd=repo).strip()
        manifest["tracked_changes"] = command(["git", "diff", "--name-only", "HEAD"],
                                              work, "tracked-changes", manifest, cwd=repo).splitlines()
        for name, cmd in [("qemu-version", [args.qemu, "--version"]),
                          ("llvm-version", [str(llvm / "llvm-mc"), "--version"]),
                          ("rust-version", ["rustc", "+stable", "-vV"])]:
            manifest[name] = command(cmd, work, name, manifest).strip()
        metadata = make_cases(work, llvm, manifest)
        run_guest(work, here, llvm, args.qemu, args.timeout, manifest)
        runner = build_runner(work, here, repo, target, manifest)
        status = compare(work, runner, metadata, manifest)
        if source_fingerprint(repo) != manifest["source_fingerprint"]:
            raise ValueError("RAX sources changed during the oracle run")
        if {name: digest(here / name) for name in tool_names} != manifest["tool_sha256"]:
            raise ValueError("Oracle sources changed during the run")
        manifest["result"] = "match" if status == 0 else "mismatch"
        return status
    finally:
        names = ("cases.bin", "results.bin", "guest.elf", "guest.s", "instructions.s")
        manifest["artifact_sha256"] = {name: digest(work / name) for name in names
                                       if (work / name).is_file()}
        write_json(work / "manifest.json", manifest)


if __name__ == "__main__":
    raise SystemExit(main())
