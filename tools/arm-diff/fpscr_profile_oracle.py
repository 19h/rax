#!/usr/bin/env python3
"""Verify the non-trapping A32/T32 FPSCR register contract in QEMU system mode."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--llvm-bin", type=Path, required=True)
    parser.add_argument("--qemu", default="qemu-system-arm")
    args = parser.parse_args()
    out = (args.work_dir or Path(tempfile.mkdtemp(prefix="rax-fpscr-profile-"))).resolve()
    out.mkdir(parents=True, exist_ok=True)
    owner = out / ".rax-fpscr-profile"
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != "version=1\n"):
        raise ValueError("Use an empty directory or an existing FPSCR-profile work directory")
    owner.write_text("version=1\n")
    source = Path(__file__).with_name("fpscr_profile_guest.s")
    shutil.copyfile(source, out / "register-oracle.s")
    (out / "link.ld").write_text("ENTRY(_start)\nPHDRS {ram PT_LOAD FLAGS(7); }\n"
                                "SECTIONS {. = 0x40010000;.text : {*(.text*) *(.rodata*) *(.data*)} :ram}\n")
    llvm = args.llvm_bin.resolve()
    manifest = {"cases": 32768, "commands": [], "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
    commands = [
        ("qemu-version", [args.qemu, "--version"]),
        ("llvm-version", [str(llvm / "llvm-mc"), "--version"]),
        ("assemble", [str(llvm / "llvm-mc"), "-triple=armv8.2a-none-eabi",
                      "-mattr=+neon,+fullfp16,+fp16fml", "-filetype=obj",
                      "register-oracle.s", "-o", "register-oracle.o"]),
        ("link", [str(llvm / "ld.lld"), "-m", "armelf", "-T", "link.ld",
                  "register-oracle.o", "-o", "register-oracle.elf"]),
        ("qemu", [args.qemu, "-machine", "virt", "-cpu", "max", "-accel",
                  "tcg,thread=single", "-m", "128M", "-nodefaults", "-display",
                  "none", "-monitor", "none", "-serial", "none",
                  "-semihosting-config", "enable=on,target=native", "-kernel",
                  str(out / "register-oracle.elf")]),
    ]
    for name, command in commands:
        start = time.monotonic()
        record = {"name": name, "args": command}
        manifest["commands"].append(record)
        try:
            with (out / (name + ".log")).open("w") as log:
                result = subprocess.run(command, cwd=out, stdout=log,
                                        stderr=subprocess.STDOUT, timeout=30)
            record["code"] = result.returncode
            if result.returncode:
                raise RuntimeError(f"{name} failed: status {result.returncode}")
        except subprocess.TimeoutExpired:
            record["code"] = "timeout"
            raise RuntimeError(f"{name} exceeded 30 s") from None
        finally:
            record["seconds"] = time.monotonic() - start
            (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if "RAX_A32_NONTRAPPING_PROFILE_OK_32768" not in (out / "qemu.log").read_text().splitlines():
        raise RuntimeError("Guest did not complete all32768 register checks")
    manifest["result"] = "match"
    manifest["elf_sha256"] = hashlib.sha256((out / "register-oracle.elf").read_bytes()).hexdigest()
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"All32768 A32/T32 non-trapping FPSCR register checks passed. Evidence: {out}")


if __name__ == "__main__":
    main()
