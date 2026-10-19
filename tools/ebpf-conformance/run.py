#!/usr/bin/env python3
"""Run the external RFC 9669 oracles against every rax eBPF execution plane.

Subcommands (default: all):
  conformance  eBPF Foundation bpf_conformance runner and plugin, all planes
  kernel-le    Linux arm64 kernel (BPF JIT) in a privileged Docker container
  kernel-be    Linux s390x kernel (BPF interpreter) booted under QEMU
  admission-le kernel-verifier admission of every encoding partition (arm64)
  admission-be the same admission campaign on the s390x kernel
  all          every oracle above

--export rewrites the checked-in corpora under tests/generated/ebpf/ from the
oracle results instead of only comparing; --scale N selects the number of
random operand pairs per kernel form (corpora always use scale 0).
"""

import argparse
import gzip
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools/ebpf-conformance"
WORK = ROOT / "target/ebpf-conformance"
CARGO_TARGET = ROOT / "target/ebpf-reference"
GENERATED = ROOT / "tests/generated/ebpf"

CONFORMANCE_ARCHIVE = ROOT / "docs/specifications/ebpf/bpf_conformance/bpf_conformance-698e3795.tar.gz"
CONFORMANCE_SHA256 = "4e09b533c6713b39f5833500dcfbf56df3e18ce9403ee8d879e1d9004c17090e"
CONFORMANCE_PREFIX = "bpf_conformance-698e379525f44cf422bc1dbd43efe8495a76ab87/"
ELFIO_URL = "https://codeload.github.com/serge1/ELFIO/tar.gz/660e90514a2e80fba1c55f2cee07bbeda3d1c601"
ELFIO_SHA256 = "418315e42d2c4e6745b66b672e05ca9e4d29a57d7d52404b2dda855155136084"
ELFIO_PREFIX = "ELFIO-660e90514a2e80fba1c55f2cee07bbeda3d1c601/"
DEBIAN_KERNEL_URLS = (
    "http://deb.debian.org/debian/pool/main/l/linux/"
    "linux-image-6.12.107+deb13-s390x_6.12.107-1_s390x.deb",
    # The pool drops a superseded kernel; snapshot.debian.org keeps serving the
    # same package by its SHA-1.
    "https://snapshot.debian.org/file/45344e32ea08150757ab7ec3aea54edfc8cc1039",
)
DEBIAN_KERNEL_SHA256 = "72f968758d4ffbb07ad0887d968beec650f961fdc9d3fabcf87a678626ddfd26"
VMLINUZ = "boot/vmlinuz-6.12.107+deb13-s390x"
VMLINUZ_SHA256 = "354ea1113285149ad81e0487a34867fdfa827b02b21be8153f576ee7cbba71be"
DOCKER_IMAGE = "alpine@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6"
ENGINES = ["direct", "smir-o0", "smir-o1", "smir-o2"]
KERNEL_LE = (
    "Linux 7.0.14-orbstack-00380-ga7e0a2dc9535 aarch64 (OrbStack), "
    "arm64 BPF JIT (CONFIG_BPF_JIT_ALWAYS_ON=y)"
)
KERNEL_BE = (
    "Linux 6.12.107+deb13-s390x (Debian 13 linux-image-6.12.107+deb13-s390x "
    "6.12.107-1) under QEMU TCG, kernel BPF interpreter (no CONFIG_BPF_JIT)"
)


def run(command, **kwargs):
    print("+", " ".join(str(part) for part in command), flush=True)
    return subprocess.run([str(part) for part in command], check=True, **kwargs)


def verified(path, expected):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != expected:
        raise SystemExit(f"checksum mismatch for {path}: {digest}")
    return path


def fetch(urls, expected, name):
    """Download `name` from the first of `urls` that serves the expected bytes."""
    path = WORK / "downloads" / name
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        for url in urls:
            try:
                with urllib.request.urlopen(url) as response:
                    data = response.read()
            except OSError as error:
                print(f"{url}: {error}", flush=True)
                continue
            if hashlib.sha256(data).hexdigest() == expected:
                path.write_bytes(data)
                break
            print(f"{url}: checksum mismatch", flush=True)
        else:
            raise SystemExit(f"no source served {name} with SHA-256 {expected}")
    return verified(path, expected)


def extract(archive, prefix, destination):
    """Extract regular files under `prefix`, rejecting unexpected members."""
    if destination.exists():
        shutil.rmtree(destination)
    with tarfile.open(archive) as source:
        for member in source.getmembers():
            if member.isdir() or member.name == prefix.rstrip("/"):
                continue
            if not member.name.startswith(prefix):
                raise SystemExit(f"unexpected archive member {member.name}")
            relative = Path(member.name[len(prefix):])
            if relative.is_absolute() or ".." in relative.parts:
                raise SystemExit(f"invalid archive member {member.name}")
            if member.issym() or member.islnk():
                continue
            if not member.isfile():
                raise SystemExit(f"unsupported archive member {member.name}")
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(source.extractfile(member).read())
    return destination


def cargo_build():
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(CARGO_TARGET)
    run(
        ["cargo", "+stable", "build", "--release", "--locked", "--manifest-path",
         TOOL / "Cargo.toml", "--bins"],
        env=environment,
    )
    return CARGO_TARGET / "release"


def conformance(binaries, export):
    source = extract(verified(CONFORMANCE_ARCHIVE, CONFORMANCE_SHA256), CONFORMANCE_PREFIX,
                     WORK / "bpf_conformance")
    elfio = fetch([ELFIO_URL], ELFIO_SHA256, "elfio.tar.gz")
    extract(elfio, ELFIO_PREFIX, source / "external/elfio")
    build = WORK / "bpf_conformance-build"
    run(["cmake", "-S", source, "-B", build, "-DCMAKE_BUILD_TYPE=Release"],
        stdout=subprocess.DEVNULL)
    run(["cmake", "--build", build], stdout=subprocess.DEVNULL)
    runner = build / "bin/bpf_conformance_runner"
    plugin = binaries / "rax-bpf-conformance-plugin"
    for engine in ENGINES:
        result = subprocess.run(
            [runner, "--test_file_directory", source / "tests", "--plugin_path", plugin,
             "--plugin_options", f"--engine {engine}", "--cpu_version", "v4"],
            capture_output=True, text=True,
        )
        summary = [line for line in result.stdout.splitlines() if line.startswith("Passed")]
        print(f"bpf_conformance {engine}: {summary}")
        if summary != ["Passed 312 out of 312 tests."]:
            print(result.stdout[-4000:])
            raise SystemExit(f"bpf_conformance failed on {engine}")

    exporter = WORK / "export_corpus"
    library = build / "lib/libbpf_conformance_core.a"
    compiler = shutil.which("c++") or "c++"
    run([compiler, "-std=c++20", "-O2", f"-I{source / 'lib/include'}", "-o", exporter,
         TOOL / "conformance/export_corpus.cc", library])
    vectors = subprocess.run([exporter, source / "tests", source / "negative"],
                             check=True, capture_output=True, text=True).stdout
    # The runner skips the raw reserved-field negatives (their expected error
    # text is empty); every plane must reject each of them.
    rejects = [line.split() for line in vectors.splitlines() if line.startswith("reject ")]
    for _, name, program in rejects:
        hex_program = " ".join(program[i:i + 2] for i in range(0, len(program), 2))
        for engine in ENGINES:
            result = subprocess.run([plugin, "--engine", engine], input=hex_program + "\n",
                                    capture_output=True, text=True)
            if result.returncode == 0:
                raise SystemExit(f"{engine} accepted the negative test {name}")
    print(f"bpf_conformance negatives: {len(rejects)} rejected on {len(ENGINES)} planes")
    if len(rejects) != 45:
        raise SystemExit(f"expected 45 negative tests, found {len(rejects)}")

    corpus = GENERATED / "bpf_conformance.txt"
    header = "".join(line + "\n" for line in corpus.read_text().splitlines() if line.startswith("#"))
    if export:
        corpus.write_text(header + vectors)
        print(f"wrote {corpus}")
    else:
        recorded = "".join(line + "\n" for line in corpus.read_text().splitlines()
                           if not line.startswith("#"))
        if recorded != vectors:
            raise SystemExit("the checked-in bpf_conformance corpus differs from the suite")
        print("bpf_conformance corpus matches the pinned suite")


def harness(arch):
    output = WORK / f"harness-{arch}"
    run(["clang", f"--target={arch}-linux-gnu", "-O2", "-ffreestanding", "-fno-builtin",
         "-nostdlib", "-static", "-fno-pic", "-fno-stack-protector",
         "-fno-asynchronous-unwind-tables", "-Wall", "-Wextra", "-Werror",
         "-fuse-ld=lld", "-Wl,-e,_start", "-o", output, TOOL / "kernel/harness.c"])
    return output


def campaign(binaries, command, directory, endian, scale, *extra):
    run([binaries / "rax-ebpf-kernel-campaign", command, directory, *extra,
         "--endian", endian, "--scale", str(scale)])


def kernel_le(binaries, export, scale):
    directory = WORK / "kernel-le"
    directory.mkdir(parents=True, exist_ok=True)
    shutil.copy(harness("aarch64"), directory / "harness")
    campaign(binaries, "generate", directory, "le", scale)
    (directory / "output.bin").unlink(missing_ok=True)
    run(["docker", "run", "--rm", "--privileged", "--platform", "linux/arm64",
         "-v", f"{directory}:/work", DOCKER_IMAGE, "/work/harness"])
    if export:
        campaign(binaries, "export", directory, "le", 0, GENERATED / "linux_kernel_le.txt",
                 "--kernel", KERNEL_LE)
    else:
        campaign(binaries, "compare", directory, "le", scale)


def initramfs(harness_binary, payload):
    """A gzip newc archive whose /init is the harness."""
    def entry(name, mode, data=b"", rdev=(0, 0), ino=1):
        fields = [ino, mode, 0, 0, 1, 0, len(data), 0, 0, rdev[0], rdev[1], len(name) + 1, 0]
        record = b"070701" + b"".join(f"{value:08x}".encode() for value in fields)
        record += name.encode() + b"\0"
        record += b"\0" * (-len(record) % 4) + data
        return record + b"\0" * (-len(record) % 4)

    archive = b"".join([
        entry("dev", 0o040755, ino=1),
        entry("dev/console", 0o020600, rdev=(5, 1), ino=2),
        entry("init", 0o100755, harness_binary, ino=3),
        entry("input.bin", 0o100644, payload, ino=4),
        entry("TRAILER!!!", 0, ino=5),
    ])
    archive += b"\0" * (-len(archive) % 512)
    return gzip.compress(archive, compresslevel=6, mtime=0)


def vmlinuz():
    package = fetch(DEBIAN_KERNEL_URLS, DEBIAN_KERNEL_SHA256, "linux-image-s390x.deb")
    output = WORK / "vmlinuz-s390x"
    if not output.exists():
        data = package.read_bytes()
        if not data.startswith(b"!<arch>\n"):
            raise SystemExit("unexpected Debian package format")
        offset = 8
        while offset < len(data):
            name = data[offset:offset + 16].decode().strip()
            size = int(data[offset + 48:offset + 58].decode().strip())
            body = data[offset + 60:offset + 60 + size]
            if name.startswith("data.tar"):
                with tarfile.open(fileobj=io.BytesIO(body)) as tar:
                    member = tar.getmember("./" + VMLINUZ)
                    output.write_bytes(tar.extractfile(member).read())
                break
            offset += 60 + size + (size & 1)
    return verified(output, VMLINUZ_SHA256)


def kernel_be(binaries, export, scale):
    directory = WORK / "kernel-be"
    directory.mkdir(parents=True, exist_ok=True)
    campaign(binaries, "generate", directory, "be", scale)
    initrd = directory / "initrd.gz"
    initrd.write_bytes(initramfs(harness("s390x").read_bytes(),
                                 (directory / "input.bin").read_bytes()))
    output = directory / "output.bin"
    output.unlink(missing_ok=True)
    console = subprocess.run(
        ["qemu-system-s390x", "-M", "s390-ccw-virtio", "-cpu", "max", "-m", "4096", "-smp", "1",
         "-nographic", "-no-reboot", "-kernel", vmlinuz(), "-initrd", initrd,
         "-append", "console=ttysclp0 quiet panic=-1",
         "-device", "virtio-serial-ccw",
         "-chardev", f"file,id=rax,path={output}",
         "-device", "virtserialport,chardev=rax,nr=1"],
        capture_output=True, text=True, timeout=7200,
    )
    if "RAXBPF-PORT-DONE" not in console.stdout:
        print(console.stdout[-4000:])
        raise SystemExit("the s390x harness did not complete")
    if export:
        campaign(binaries, "export", directory, "be", 0, GENERATED / "linux_kernel_be.txt",
                 "--kernel", KERNEL_BE)
    else:
        campaign(binaries, "compare", directory, "be", scale)


def admission(binaries, export, endian):
    directory = WORK / f"admission-{endian}"
    directory.mkdir(parents=True, exist_ok=True)
    tool = binaries / "rax-ebpf-kernel-admission"
    run([tool, "generate", directory, "--endian", endian])
    output = directory / "output.bin"
    output.unlink(missing_ok=True)
    if endian == "le":
        shutil.copy(harness("aarch64"), directory / "harness")
        run(["docker", "run", "--rm", "--privileged", "--platform", "linux/arm64",
             "-v", f"{directory}:/work", DOCKER_IMAGE, "/work/harness"])
        kernel = "Linux 7.0.14-orbstack-00380-ga7e0a2dc9535 aarch64 (OrbStack), BPF_PROG_TYPE_SYSCALL verifier"
    else:
        initrd = directory / "initrd.gz"
        initrd.write_bytes(initramfs(harness("s390x").read_bytes(),
                                     (directory / "input.bin").read_bytes()))
        console = subprocess.run(
            ["qemu-system-s390x", "-M", "s390-ccw-virtio", "-cpu", "max", "-m", "6144",
             "-smp", "1", "-nographic", "-no-reboot", "-kernel", vmlinuz(), "-initrd", initrd,
             "-append", "console=ttysclp0 quiet panic=-1", "-device", "virtio-serial-ccw",
             "-chardev", f"file,id=rax,path={output}",
             "-device", "virtserialport,chardev=rax,nr=1"],
            capture_output=True, text=True, timeout=14400,
        )
        if "RAXBPF-PORT-DONE" not in console.stdout:
            print(console.stdout[-4000:])
            raise SystemExit("the s390x harness did not complete")
        kernel = "Linux 6.12.107+deb13-s390x (Debian 13) under QEMU TCG, BPF_PROG_TYPE_SYSCALL verifier"
    corpus = GENERATED / f"linux_kernel_admission_{endian}.txt"
    if export:
        run([tool, "export", directory, corpus, "--endian", endian, "--kernel", kernel])
    else:
        report = subprocess.run([tool, "compare", directory, "--endian", endian],
                                capture_output=True, text=True)
        print(report.stdout.splitlines()[-1] if report.stdout else report.stderr)
        if report.returncode != 0:
            raise SystemExit(f"unexplained admission disagreements ({endian})")
        recorded = [line for line in corpus.read_text().splitlines() if line.startswith("disagree ")]
        observed = [line for line in report.stdout.splitlines() if line.startswith("disagree ")]
        if recorded != observed:
            raise SystemExit(f"admission disagreements differ from {corpus}")


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("oracle", nargs="?", default="all",
                        choices=["conformance", "kernel-le", "kernel-be", "admission-le",
                                 "admission-be", "all"])
    parser.add_argument("--export", action="store_true")
    parser.add_argument("--scale", type=int, default=400)
    arguments = parser.parse_args()
    scale = 0 if arguments.export else arguments.scale
    WORK.mkdir(parents=True, exist_ok=True)
    binaries = cargo_build()
    if arguments.oracle in ("conformance", "all"):
        conformance(binaries, arguments.export)
    if arguments.oracle in ("kernel-le", "all"):
        kernel_le(binaries, arguments.export, scale)
    if arguments.oracle in ("kernel-be", "all"):
        kernel_be(binaries, arguments.export, scale)
    if arguments.oracle in ("admission-le", "all"):
        admission(binaries, arguments.export, "le")
    if arguments.oracle in ("admission-be", "all"):
        admission(binaries, arguments.export, "be")


if __name__ == "__main__":
    sys.exit(main())
