#!/usr/bin/env python3
"""Compile the handwritten Intel HVF register/type/function contract against Apple SDK.

This checks source/API parity without creating a VM or executing a guest.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time

TYPES = ("hv_return_t", "hv_vcpuid_t", "hv_gpaddr_t", "hv_uvaddr_t",
         "hv_memory_flags_t", "hv_vmx_capability_t")
PRIMITIVES = {"u8": "uint8_t", "u16": "uint16_t", "u32": "uint32_t",
              "u64": "uint64_t", "i32": "int32_t", "usize": "size_t",
              "bool": "bool", "c_void": "void"}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def c_type(rust: str) -> str:
    rust = rust.strip()
    if rust.startswith("*mut "):
        return c_type(rust[5:]) + " *"
    if rust.startswith("*const "):
        return "const " + c_type(rust[7:]) + " *"
    if rust in PRIMITIVES:
        return PRIMITIVES[rust]
    if rust in TYPES:
        return "rax_" + rust
    raise ValueError(f"Unmapped Rust FFI type: {rust}")


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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--bindings", type=Path)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--sdk", type=Path)
    args = parser.parse_args()
    source = (args.bindings or args.repo / "src/backend/hvf/bindings.rs").resolve()
    out = args.work_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    owner = out / ".rax-hvf-sdk-abi"
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != "version=1\n"):
        raise ValueError("Use an empty directory or an owned version1 ABI directory")
    owner.write_text("version=1\n")
    sdk = args.sdk or Path(subprocess.check_output(["xcrun", "--show-sdk-path"], text=True).strip())
    sdk = sdk.resolve()
    headers = sdk / "System/Library/Frameworks/Hypervisor.framework/Headers"
    manifest = {"version": 1, "bindings": str(source), "bindings_sha256": digest(source),
                "tool_sha256": digest(Path(__file__)), "sdk": str(sdk), "commands": [],
                "sdk_header_sha256": {name: digest(headers / name) for name in
                                      ["hv_arch_x86.h", "hv.h", "hv_vmx.h", "hv_types.h", "hv_error.h"]},
                "arm64_error_header_sha256": digest(sdk / "usr/include/arm64/hv/hv_kern_types.h"),
                "scope": "Intel register IDs, selected ABI type sizes/alignments, HV constants, and every declared x86 foreign function signature. VMCS field encodings and native hypervisor/guest execution are separate."}
    text = source.read_text()
    aliases = {}
    for name in TYPES:
        found = re.search(r"^pub type " + name + r"\s*=\s*([^;]+);", text, re.M)
        if not found:
            raise ValueError(f"Missing type {name}")
        aliases[name] = found.group(1)
    enum_at = text.index("pub enum hv_x86_reg_t {")
    enum_start = text.rfind("#[repr(", 0, enum_at)
    enum_end = text.index("\n}", enum_at) + 2
    if enum_start < 0:
        raise ValueError("Register enum must have explicit representation")
    enum_source = text[enum_start:enum_end]
    registers = re.findall(r"^\s+(HV_X86_[A-Z0-9_]+)\s*=", enum_source, re.M)
    if not registers:
        raise ValueError("Empty register enum")
    constants = re.findall(r"^pub const (HV_[A-Z0-9_]+)\s*:\s*([^=]+)=\s*([^;]+);", text, re.M)
    foreign_at = text.index('unsafe extern "C" {')
    foreign_end = text.index("\n}", foreign_at)
    functions = re.findall(r"pub fn (\w+)\((.*?)\)\s*->\s*(\w+)\s*;", text[foreign_at:foreign_end], re.S)
    if not functions or len(functions) != len(re.findall(r"pub fn ", text[foreign_at:foreign_end])):
        raise ValueError("Unparsed or empty foreign function declaration list")
    rust = '#![allow(non_camel_case_types,dead_code)]\nuse std::ffi::c_void;\n'
    rust += "\n".join(f"type {name} = {value};" for name, value in aliases.items()) + "\n"
    rust += enum_source + "\n"
    rust += "\n".join(f"const {name}: {kind.strip()} = {value};" for name, kind, value in constants) + "\n"
    rust += 'fn main() { println!("{{\\\"values\\\":{{");\n'
    expressions = [(name, "hv_x86_reg_t::" + name) for name in registers]
    expressions += [(name, name) for name, _, _ in constants]
    for i, (name, expression) in enumerate(expressions):
        comma = "," if i else ""
        rust += f'println!("{comma}\\\"{name}\\\":{{}}", {expression} as u64);\n'
    rust += 'println!("}},\\\"types\\\":{{");\n'
    for i, name in enumerate([*TYPES, "hv_x86_reg_t"]):
        comma = "," if i else ""
        rust += f'println!("{comma}\\\"{name}\\\":[{{}},{{}}]",std::mem::size_of::<{name}>(),std::mem::align_of::<{name}>());\n'
    rust += 'println!("}}}}"); }\n'
    (out / "probe.rs").write_text(rust)
    for name, command in [("rust-version", ["rustc", "+stable", "-vV"]),
                          ("clang-version", ["xcrun", "clang", "--version"]),
                          ("sdk-version", ["xcrun", "--show-sdk-version"]),
                          ("rust-build", ["rustc", "+stable", "--edition=2024", str(out / "probe.rs"), "-o", str(out / "rust-probe")])]:
        if run(command, out, name, manifest).returncode:
            raise RuntimeError(f"{name} failed")
    result = run([str(out / "rust-probe")], out, "rust-values", manifest)
    if result.returncode:
        raise RuntimeError("Rust metadata probe failed")
    values = json.loads(result.stdout)
    c = '#include <stdbool.h>\n#include <stdint.h>\n#include <stddef.h>\n#include <Hypervisor/hv.h>\n#include <Hypervisor/hv_vmx.h>\n'
    for name, value in aliases.items():
        c += f"typedef {c_type(value)} rax_{name};\n"
        c += f'_Static_assert(__builtin_types_compatible_p(rax_{name},{name}),"type {name}");\n'
    for name, (size, alignment) in values["types"].items():
        c += f'_Static_assert(sizeof({name})=={size},"size {name}");\n'
        c += f'_Static_assert(_Alignof({name})=={alignment},"alignment {name}");\n'
    for name, value in values["values"].items():
        c += f'_Static_assert((uint64_t)({name})==0x{value:016x}ULL,"value {name}");\n'
    for name, parameters, result in functions:
        parameters = re.sub(r"//[^\n]*", "", parameters)
        types = [c_type(part.split(":", 1)[1]) for part in parameters.split(",") if part.strip()]
        c += f'typedef {c_type(result)} (*rax_signature_{name})({",".join(types) or "void"});\n'
        c += f'_Static_assert(__builtin_types_compatible_p(__typeof__(&{name}),rax_signature_{name}),"signature {name}");\n'
    (out / "probe.c").write_text(c)
    result = run(["xcrun", "clang", "-target", "x86_64-apple-macosx13.0", "-isysroot", str(sdk),
                  "-std=c11", "-Werror", "-Wno-deprecated-declarations", "-fsyntax-only", str(out / "probe.c")], out, "sdk-compile", manifest)
    arm = '#include <stdint.h>\n#include <Hypervisor/hv_vm.h>\n'
    common = [name for name, kind, _ in constants if kind.strip() in ("hv_return_t", "hv_memory_flags_t")]
    for name in common:
        arm += f'_Static_assert((uint64_t)({name})==0x{values["values"][name]:016x}ULL,"shared value {name}");\n'
    (out / "probe-arm64.c").write_text(arm)
    arm_result = run(["xcrun", "clang", "-target", "arm64-apple-macosx13.0", "-isysroot", str(sdk),
                      "-std=c11", "-Werror", "-fsyntax-only", str(out / "probe-arm64.c")], out, "sdk-arm64-constants", manifest)
    manifest["arm64_shared_constants"] = len(common)
    manifest["counts"] = {"register_ids": len(registers), "constants": len(constants), "types": len(TYPES),
                          "type_size_alignment_pairs": len(values["types"]), "function_signatures": len(functions)}
    manifest["artifact_sha256"] = {name: digest(out / name) for name in ["probe.rs", "probe.c", "probe-arm64.c", "rust-values.stdout"]}
    if digest(source) != manifest["bindings_sha256"]:
        raise RuntimeError("Bindings changed during parity check")
    manifest["result"] = "match" if result.returncode == 0 and arm_result.returncode == 0 else "mismatch"
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"result": manifest["result"], "counts": manifest["counts"], "evidence": str(out)}))
    return result.returncode or arm_result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
