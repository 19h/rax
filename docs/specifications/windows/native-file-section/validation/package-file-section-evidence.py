"""Publish declared records with artifact-only private compiler projections."""

import hashlib
import json
from pathlib import Path
import shutil

Q = Path(__file__).parent
S = Path("/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
DEST = Path(
    "/Users/int/.assist-native-file-section-20261021/docs/specifications/windows/native-file-section"
)
V = DEST / "validation"


def fingerprint(data):
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def main():
    names = {
        "file-section-candidate1-windows.json",
        "file-section-candidate1-final-gates.json",
        "file-section-native-owning-candidate1.json",
        "native-file-section-candidate1-pre-cpp-archives.json",
        "native-file-section-candidate1-final-owning-archive-identities.json",
        "file-section-native-ordinary-candidate1.json",
        "file-section-owning-capi-candidate1.json",
        "file-section-retained-owning-candidate1.json",
        "file-section-owning-source-hashes-candidate1.json",
        "file-section-ordinary-owner-candidate1.rs",
        "file-section-owning-capi.cpp",
        "file-section-observer-source-candidate1.json",
        "file-section-owning-capi-source.json",
        "run-file-section-native-ordinary-candidate1.py",
        "run-file-section-owning-capi-candidate1.py",
        "run-file-section-native-owning-candidate1.py",
        "run-file-section-owning-candidate1.py",
        "retain-file-section-artifacts-candidate1.py",
        "collect-file-section-archives-candidate1.py",
        "file-section-owned-cpp-build-candidate1.py",
        "owning-linux-cpp-build-file-section-candidate1.py",
        "run-file-section-candidate1.py",
        "run-file-section-unix-candidate1-v2.py",
        "audit-file-section-native-gates-candidate1.py",
        "run-file-section-oracle-stress.py",
        "file-section-candidate1-cross-aarch64.log",
        "file-section-candidate1-cross-x64.log",
        "macos-file-section-candidate1-linkage.json",
    }
    receipts = [
        "file-section-candidate1-windows.json",
        "file-section-native-owning-candidate1.json",
        "file-section-native-ordinary-candidate1.json",
        "file-section-owning-capi-candidate1.json",
    ]
    cargo = {
        "file-section-native-owning-candidate1-artifacts-before.log",
        "file-section-native-owning-candidate1-artifacts-after.log",
        "macos-file-section-candidate1-owning-prime-artifacts.log",
        "linux-file-section-candidate1-owning-shipping.log",
        "linux-file-section-candidate1-owning-artifacts.log",
    }
    for host in ["macos", "linux"]:
        receipts += [
            f"file-section-{host}-candidate1-gates.json",
            f"{host}-file-section-candidate1-owning-gates.json",
        ]
        names.update(
            {
                f"{host}-file-section-candidate1-owning-commands.json",
                f"{host}-file-section-candidate1-archives-before.json",
                f"{host}-file-section-candidate1-archives-after.json",
            }
        )
    for name in receipts:
        path = next(p / name for p in [Q, S] if (p / name).exists())
        value = json.loads(path.read_text())
        names.add(name)
        names.update(step["log"] for step in value["steps"])
    for host in ["macos", "linux", "windows"]:
        for kind in ["oracle", "owning"]:
            path = Q / f"file-section-{kind}-stress-{host}-candidate1.json"
            if path.exists():
                names.add(path.name)
    projections = []
    for name in sorted(names):
        candidates = [p / name for p in [Q, S] if (p / name).exists()]
        if not candidates:
            raise FileNotFoundError(name)
        data = candidates[0].read_bytes()
        if len(candidates) == 2:
            assert candidates[1].read_bytes() == data, "ambiguous source " + name
        published = data
        if name in cargo:
            kept = []
            for line in data.decode(errors="replace").splitlines():
                try:
                    row = json.loads(line)
                except ValueError:
                    continue
                if row.get("reason") == "compiler-artifact":
                    kept.append(line)
            assert kept, name
            published = ("\n".join(kept) + "\n").encode()
            projections.append(
                {
                    "path": name,
                    "kind": "cargo-artifact-only",
                    "original": fingerprint(data),
                    "published": fingerprint(published),
                    "withheld-records": len(data.decode(errors="replace").splitlines())
                    - len(kept),
                }
            )
        if name == "macos-file-section-candidate1-owning-build.log":
            published = b"Private Assist compiler transcript retained locally. Original identity and successful build result remain in the owning receipt.\n"
            projections.append(
                {
                    "path": name,
                    "kind": "private-build-transcript-withheld",
                    "original": fingerprint(data),
                    "published": fingerprint(published),
                    "withheld-records": len(data.decode(errors="replace").splitlines()),
                }
            )
        (V / name).write_bytes(published)
    (V / "public-projections.json").write_text(
        json.dumps({"files": projections}, indent=2) + "\n"
    )
    print(
        "declared records", len(names), "private compiler projections", len(projections)
    )


if __name__ == "__main__":
    main()
