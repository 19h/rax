#!/usr/bin/env python3
"""Run the rax eBPF Kani proofs in a pinned Linux container.

Kani's C library model does not link on macOS hosts, so the proofs run in a
Linux arm64 image built from the pinned base and Kani release below, with
the repository mounted at /rax. The SBPF proofs compile the pinned Anza
source, which this script verifies and extracts first (as
tools/ebpf-reference/run.py does).

Usage:
  python3 tools/ebpf-formal/run.py                 # the discharged families
  python3 tools/ebpf-formal/run.py --harness NAME  # a subset (substring)
  python3 tools/ebpf-formal/run.py --all           # also the SMIR and SBPF
                                                   # execution harnesses
  python3 tools/ebpf-formal/run.py --jobs 12       # parallel CBMC jobs

The default families are the RFC 9669 direct plane, fetch and meter, the
memory contract, and SBPF admission. The SMIR and SBPF execution harnesses
(with the SBPF fetch contract they rely on) are defined but not discharged
on 16 cores and 64 GiB (see the README for the measurements).
"""

import argparse
import importlib.util
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools/ebpf-formal"
IMAGE = "rax-kani:0.68.0"
KANI_VERSION = "0.68.0"
TARGET = "target/ebpf-formal-linux"
REPORT = ROOT / TARGET / "report.txt"
DEFAULT_FAMILIES = [
    "proofs::opcodes::direct_",
    "proofs::fetch::",
    "proofs::memory::",
    "sbpf::opcodes::sbpf_admission_",
]


def run(command, **kwargs):
    print("+", " ".join(str(part) for part in command), flush=True)
    return subprocess.run(command, check=True, **kwargs)


def extract_reference_source():
    path = ROOT / "tools/ebpf-reference/run.py"
    # Import without leaving a __pycache__ directory in the tool tree.
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("ebpf_reference_run", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.extract(ROOT)


def base_digest():
    return (TOOL / "BASE_IMAGE").read_text().strip()


def build_image():
    dockerfile = (
        f"FROM rust@{base_digest()}\n"
        f"RUN cargo install --locked kani-verifier --version {KANI_VERSION} && cargo kani setup\n"
    )
    run(["docker", "build", "--platform", "linux/arm64", "-t", IMAGE, "-"],
        input=dockerfile.encode())


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--harness", action="append", default=[])
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--all", action="store_true")
    args = parser.parse_args()
    harnesses = args.harness or ([] if args.all else DEFAULT_FAMILIES)

    run([sys.executable, "-I", TOOL / "gen_registry.py", "--check"])
    run([sys.executable, "-I", TOOL / "gen_proofs.py", "--check"])
    extract_reference_source()
    # cargo-kani has no --locked; check that the lockfile is complete first.
    run(["cargo", "+stable", "metadata", "--locked", "--format-version", "1",
         "--manifest-path", TOOL / "Cargo.toml"], stdout=subprocess.DEVNULL)
    build_image()
    command = [
        "docker", "run", "--rm", "--platform", "linux/arm64",
        "-v", f"{ROOT}:/rax",
        "-v", "rax-kani-registry:/usr/local/cargo/registry",
        "-v", "rax-kani-git:/usr/local/cargo/git",
        "-w", "/rax/tools/ebpf-formal", IMAGE,
        "cargo", "kani", "--target-dir", f"/rax/{TARGET}",
        "-Z", "stubbing", "-Z", "restrict-vtable",
        "--jobs", str(args.jobs), "--output-format", "terse",
    ]
    for harness in harnesses:
        command += ["--harness", harness]
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    with REPORT.open("w") as report:
        status = subprocess.run(command, stdout=report, stderr=subprocess.STDOUT).returncode
    text = REPORT.read_text()
    summary = re.findall(r"Complete - (\d+) successfully verified harnesses, (\d+) failures, (\d+) total", text)
    failed = re.findall(r"Verification failed for - (\S+)", text)
    if not summary:
        print(text[-4000:])
        raise SystemExit("Kani did not complete; see " + str(REPORT))
    verified, failures, total = map(int, summary[-1])
    print(f"Kani {KANI_VERSION}: {verified} of {total} harnesses verified, {failures} failed")
    for name in failed:
        print("  failed:", name)
    if status != 0 or failures or verified != total:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
