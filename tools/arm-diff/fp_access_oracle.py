#!/usr/bin/env python3
"""Independent A32/T32 CPACR permissions through actual Undefined exceptions."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import time
from fp_system_oracle import source_fingerprint


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def guest_source() -> tuple[str,list[dict]]:
    ops=['vadd.f32 s0,s0,s1','vadd.i32 d0,d1,d2','vmrs r0,fpscr','vmrs r0,fpexc']
    source='''.syntax unified
    .arm
    .global _start
    .align 5
    _start:
        b init
        b undefined
        b supervisor
        b failed
        b failed
        b failed
        b failed
        b failed
    init:
        ldr r0,=_start
        mcr p15,0,r0,c12,c0,0
        mov r0,#0xf00000
        mcr p15,0,r0,c1,c0,2
        isb
        mov r0,#0x40000000
        vmsr fpexc,r0
        mov r7,#0
    '''
    cases=[]
    for thumb in [False,True]:
     for user in [False,True]:
      for access in [0,1,3]:
       for op,instruction in enumerate(ops):
        denied=access==0 or (access==1 and user) or (op==3 and user)
        cases.append({'thumb':thumb,'user':user,'access':access,'instruction':instruction,'expected_undefined':denied})
        source+=f'\n    ldr r0,={access<<20|access<<22:#x}\n    mcr p15,0,r0,c1,c0,2\n    isb\n    ldr r9,={"thumb" if thumb else "arm"}_slot_{op}'+ ('+1' if thumb else '')+'\n'
        if user:source+='    msr cpsr_c,#0x10\n'
        source+=f'    mov r2,#0\n    blx r9\n    svc #0\n    cmp r2,#{int(denied)}\n    bne failed\n    add r7,r7,#1\n'
    source+='''    cmp r7,#48
        bne failed
        mov r2,#0
        ldr r1,=success
        b finish
    failed:
        mov r2,#1
        ldr r1,=failure
    finish:
        mov r0,#4
        svc #0x123456
        ldr r1,=exit_args
        str r2,[r1,#4]
        mov r0,#0x20
        svc #0x123456
        b .
    undefined:
        mov r2,#1
        mrs r3,spsr
        tst r3,#32
        addne lr,lr,#2
        movs pc,lr
    supervisor:
        mrs r3,spsr
        bic r3,r3,#31
        orr r3,r3,#0x13
        msr spsr_cxsf,r3
        movs pc,lr
    .ltorg
    '''
    for thumb in [False,True]:
     source+='\n.thumb\n' if thumb else '\n.arm\n'
     for op,instruction in enumerate(ops):
      source+=f'{"thumb" if thumb else "arm"}_slot_{op}:\n    {instruction}\n    bx lr\n'
    source+='''.arm
    .align 2
    success:
        .asciz "RAX_CPACR_ACCESS_OK_48\\n"
    failure:
        .asciz "RAX_CPACR_ACCESS_FAILED\\n"
    .align 2
    exit_args:
        .word 0x20026,0
    '''
    return source,cases


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
    source, cases = guest_source()
    assert len(cases) == 48
    (out / "guest.s").write_text(source)
    (out / "cases.json").write_text(json.dumps(cases, indent=2) + "\n")
    (out / "link.ld").write_text("ENTRY(_start)\nSECTIONS {. = 0x40010000; .text : {*(.text*)}}\n")
    operations = ["vadd.f32 s0,s0,s1", "vadd.i32 d0,d1,d2", "vmrs r0,fpscr", "vmrs r0,fpexc"]
    instructions = ".syntax unified\n.arm\n" + "\n".join(operations) + "\n.thumb\n" + "\n".join(operations) + "\n"
    (out / "instructions.s").write_text(instructions)
    commands = [
        ("qemu-version", [qemu, "--version"]),
        ("llvm-version", [str(llvm / "llvm-mc"), "--version"]),
        ("encode", [str(llvm / "llvm-mc"), "-triple=armv8.2a-none-eabi", "-mattr=+neon,+fullfp16,+fp16fml", "-show-encoding", "instructions.s"]),
        ("assemble", [str(llvm / "llvm-mc"), "-triple=armv8.2a-none-eabi", "-mattr=+neon,+fullfp16,+fp16fml", "-filetype=obj", "guest.s", "-o", "guest.o"]),
        ("link", [str(llvm / "ld.lld"), "-m", "armelf", "-T", "link.ld", "guest.o", "-o", "guest.elf"]),
        ("qemu", [qemu, "-machine", "virt", "-cpu", "max", "-accel", "tcg,thread=single", "-m", "128M", "-nodefaults", "-display", "none", "-monitor", "none", "-serial", "none", "-semihosting-config", "enable=on,target=native", "-kernel", str(out / "guest.elf")]),
    ]
    for name, args in commands:
        if run(args, out, name, manifest).returncode:
            raise RuntimeError(f"{name} failed")
    text = (out / "qemu.stdout").read_text() + (out / "qemu.stderr").read_text()
    if "RAX_CPACR_ACCESS_OK_48" not in text.splitlines():
        raise RuntimeError("Guest did not complete all48 independent access checks")
    encoded = re.findall(r"encoding: \[([^\]]+)\]", (out / "encode.stdout").read_text())
    if len(encoded) != 8:
        raise RuntimeError("LLVM did not encode all four operations in both states")
    words = []
    for i, encoding in enumerate(encoded):
        data = bytes(int(part, 16) for part in encoding.split(","))
        if len(data) != 4:
            raise RuntimeError("Four-byte operation required")
        words.append((int.from_bytes(data[:2], "little") << 16 | int.from_bytes(data[2:], "little")) if i >= 4 else int.from_bytes(data, "little"))
    with (out / "cases.bin").open("wb") as output:
        for case in cases:
            index = operations.index(case["instruction"]) + (4 if case["thumb"] else 0)
            output.write(struct.pack("<IIIII", int(case["thumb"]), int(case["user"]), case["access"], words[index], int(case["expected_undefined"])))
    manifest["artifact_sha256"] = {name: digest(out / name) for name in ["guest.s", "guest.elf", "cases.json", "cases.bin"]}


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
    owner = out / ".rax-fp-access"
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != "version=1\n"):
        raise ValueError("Use an empty directory or an owned version1 access directory")
    owner.write_text("version=1\n")
    here = Path(__file__).resolve().parent
    manifest = {"version": 1, "cases": 48, "commands": [],
                "tool_sha256": {name: digest(here / name) for name in ["fp_access_oracle.py", "fp_access_compare.rs"]},
                "source_fingerprint": source_fingerprint(args.repo.resolve()),
                "scope": "Actual CPACR0/1/3 access and Undefined delivery for four operations in A32/T32 and privileged/user modes. cp10=2 constrained-UNPREDICTABLE policy and other trap controls remain separate."}
    reference(out, here, args.llvm_bin.resolve(), args.qemu, manifest)
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(args.target_dir.resolve())
    env["CARGO_BUILD_JOBS"] = "1"
    command = ["cargo", "+stable", "build", "--locked", "--offline", "--lib", "--no-default-features", "--features", "x86_64-suite,smir-jit"]
    start = time.monotonic()
    with (out / "rax-build.log").open("w") as log:
        result = subprocess.run(command, cwd=args.repo.resolve(), env=env, stdout=log, stderr=subprocess.STDOUT, timeout=1200)
    manifest["commands"].append({"name": "rax-build", "args": command, "code": result.returncode, "seconds": time.monotonic() - start})
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError("RAX build failed")
    library = args.target_dir.resolve() / "debug/librax.rlib"
    before = digest(library)
    shutil.copyfile(library, out / "librax.rlib")
    assert before == digest(library) == digest(out / "librax.rlib")
    manifest["rax_library_sha256"] = before
    result = run(["rustc", "+stable", "--edition=2024", str(here / "fp_access_compare.rs"), "--extern", "rax=" + str(out / "librax.rlib"), "-L", "dependency=" + str(library.parent / "deps"), "-o", str(out / "compare")], out, "runner-build", manifest)
    if result.returncode:
        raise RuntimeError("Comparator build failed")
    result = run([str(out / "compare"), str(out / "cases.bin")], out, "compare", manifest)
    report = json.loads(result.stdout)
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    assert manifest["source_fingerprint"] == source_fingerprint(args.repo.resolve())
    manifest["result"] = "match" if result.returncode == 0 else "mismatch"
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(report))
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
