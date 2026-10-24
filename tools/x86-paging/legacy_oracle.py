#!/usr/bin/env python3
"""Execute selected non-PAE x86 paging contracts in QEMU, with documented oracle limits."""
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
    parser.add_argument("--qemu", default="qemu-system-i386")
    args = parser.parse_args()
    out = (args.work_dir or Path(tempfile.mkdtemp(prefix="rax-x86-paging-"))).resolve()
    out.mkdir(parents=True, exist_ok=True)
    owner = out / ".rax-x86-paging"
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != "version=1\n"):
        raise ValueError("Use an empty directory or an existing x86-paging work directory")
    owner.write_text("version=1\n")
    source = Path(__file__).with_name("legacy_guest.s")
    shutil.copyfile(source, out / "legacy-oracle.s")
    (out / "link.ld").write_text("ENTRY(_start)\nSECTIONS {. = 0x100000;"
                                ".multiboot : {*(.multiboot)} .text : {*(.text*)} "
                                ".rodata : {*(.rodata*)} .data : {*(.data*)} "
                                ".bss : {*(.bss*) *(COMMON)}}\n")
    llvm = args.llvm_bin.resolve()
    manifest = {"expected_page_faults": 3, "commands": [], "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
    commands = [
        ("qemu-version", [args.qemu, "--version"]),
        ("llvm-version", [str(llvm / "llvm-mc"), "--version"]),
        ("assemble", [str(llvm / "llvm-mc"), "-triple=i386-none-elf", "-filetype=obj",
                      "legacy-oracle.s", "-o", "legacy-oracle.o"]),
        ("link", [str(llvm / "ld.lld"), "-m", "elf_i386", "-T", "link.ld",
                  "legacy-oracle.o", "-o", "legacy-oracle.elf"]),
        ("qemu", [args.qemu, "-machine", "pc", "-cpu", "qemu32,-pse36,-pat",
                  "-accel", "tcg,thread=single", "-m", "64M", "-nodefaults",
                  "-display", "none", "-monitor", "none", "-serial", "none",
                  "-debugcon", "file:" + str(out / "guest.log"),
                  "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
                  "-no-reboot", "-kernel", str(out / "legacy-oracle.elf")]),
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
            if result.returncode != (33 if name == "qemu" else 0):
                raise RuntimeError(f"{name} failed: status {result.returncode}")
        except subprocess.TimeoutExpired:
            record["code"] = "timeout"
            raise RuntimeError(f"{name} exceeded 30 s") from None
        finally:
            record["seconds"] = time.monotonic() - start
            (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if "RAX_X86_LEGACY_PAGING_OK" not in (out / "guest.log").read_text().splitlines():
        raise RuntimeError("Guest did not complete the selected paging contracts")
    manifest["result"] = "selected_contracts_passed"
    manifest["limits"] = [
        "TCG accepts some PAT/PSE36 fields despite CPUID feature absence.",
        "TCG reserved fault has P=0 (code8), Intel086 requires P=1 (code9).",
        "No full reserved-bit, PAE, long-mode or physical-hardware claim.",
    ]
    manifest["elf_sha256"] = hashlib.sha256((out / "legacy-oracle.elf").read_bytes()).hexdigest()
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Selected non-PAE paging checks passed; see README for oracle limitations. Evidence: {out}")


if __name__ == "__main__":
    main()
