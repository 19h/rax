"""Copy declared evidence; retain private compiler provenance by projections."""

from pathlib import Path
import hashlib
import json
import shutil

Q = Path(__file__).parent
S = Path("/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
V = Path(
    "/Users/int/.assist-native-nls-sorting-20261021/docs/specifications/windows/native-nls-sorting/validation"
)


def fingerprint(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def locate(n):
    candidates = [p / n for p in [Q, S] if (p / n).exists()]
    if not candidates:
        raise FileNotFoundError(n)
    raw = candidates[0].read_bytes()
    for p in candidates[1:]:
        assert p.read_bytes() == raw, "ambiguous " + n
    return raw


def main():
    receipts = [
        "nls-sorting-candidate1-windows.json",
        "nls-sorting-native-owning-candidate1.json",
        "nls-sorting-native-ordinary-candidate1.json",
        "nls-sorting-owning-capi-candidate1.json",
        "nls-sorting-macos-candidate1-gates.json",
        "nls-sorting-linux-candidate1-gates.json",
        "macos-nls-sorting-candidate1-owning-gates.json",
        "linux-nls-sorting-candidate1-owning-gates.json",
    ]
    names = set(receipts)
    names.update(
        [
            "macos-nls-sorting-candidate1-linkage.json",
            "nls-sorting-final-native-audit-v2.json",
            "audit-nls-sorting-final-native-v2.py",
        ]
    )
    for n in receipts:
        a = json.loads(locate(n))
        assert a["state"] == "finished", n
        names.update(x["log"] for x in a["steps"])
    names.update(
        [
            "nls-sorting-candidate1-source.json",
            "nls-sorting-candidate1-overrides.tar.gz",
            "nls-sorting-owning-source-hashes-candidate1.json",
            "nls-sorting-observer-source-candidate1.json",
            "nls-sorting-owning-capi-source.json",
            "nls-sorting-ordinary-owner-candidate1.rs",
            "nls-sorting-owning-capi.cpp",
            "nls-sorting-retained-owning-candidate1.json",
            "native-nls-sorting-candidate1-pre-cpp-archives.json",
            "native-nls-sorting-candidate1-final-owning-archive-identities.json",
            "nls-sorting-owning-retarget-macos.json",
            "nls-sorting-owning-retarget-linux.json",
            "nls-sorting-cross-final.json",
            "nls-sorting-clippy-policy.json",
            "nls-sorting-clippy-linux.log",
            "nls-sorting-clippy-linux-canonical.log",
            "nls-sorting-native-proof-stress-v1.json",
            "stress-nls-sorting-native-proof-v1.py",
            "nls-sorting-native-proof-macos-v1.json",
            "nls-sorting-native-proof-linux-v1.json",
            "nls-sorting-native-proof-windows-v1.json",
            "nls-sorting-red-tests.rs",
            "nls-sorting-red-prefix-binding.json",
            "nls-sorting-native-ordinary-candidate1-smoke.log",
            "nls-sorting-native-ordinary-candidate1-whoami.log",
            "nls-sorting-owning-capi-candidate1-ordinary.log",
        ]
    )
    for host in ["macos", "linux"]:
        names.update(
            host + "-nls-sorting-candidate1-" + suffix
            for suffix in [
                "archives-before.json",
                "archives-after.json",
                "owning-commands.json",
            ]
        )
    for arch in ["aarch64", "x64"]:
        names.update(
            [
                "nls-sorting-candidate1-cross-"
                + arch
                + ".log",
                "nls-sorting-cross-" + arch + "-clean.log",
            ]
        )
    for n in [
        "run-nls-sorting-candidate1.py",
        "run-nls-sorting-unix-candidate1.py",
        "run-nls-sorting-owning-candidate1.py",
        "run-nls-sorting-native-owning-candidate1.py",
        "run-nls-sorting-native-ordinary-candidate1.py",
        "run-nls-sorting-owning-capi-candidate1.py",
        "collect-nls-sorting-archives-candidate1.py",
        "nls-sorting-owned-cpp-build-candidate1.py",
        "owning-linux-cpp-build-nls-sorting-candidate1.py",
        "retain-nls-sorting-artifacts-candidate1.py",
    ]:
        names.add(n)
    for stem in ["native-mitigation-frontier-diagnostic-v1"]:
        for suffix in [".rs", "-source.json", ".json", "-smoke.log", "-whoami.log"]:
            names.add(stem + suffix)
        names.add("run-" + stem + ".py")
    for epoch in [1, 2, 3]:
        n = "nls-sorting-red" + str(epoch) + "-windows.json"
        names.add(n)
        a = json.loads(locate(n))
        names.update(row["log"] for row in a["steps"])
        names.add("run-nls-sorting-red" + str(epoch) + ".py")
        names.add("nls-sorting-red" + str(epoch) + "-source.json")
    names.update(["nls-sorting-red1-overrides.tar.gz", "nls-sorting-red3-target-trim.json", "nls-sorting-native-obsolete-cleanup-v1.json", "nls-sorting-old-native-symbol-cleanup-v1.json", "obsolete-task-target-cleanup.json"])
    cargo = {
        "nls-sorting-native-owning-candidate1-artifacts-before.log",
        "nls-sorting-native-owning-candidate1-artifacts-after.log",
        "macos-nls-sorting-candidate1-owning-prime-artifacts.log",
        "linux-nls-sorting-candidate1-owning-shipping.log",
        "linux-nls-sorting-candidate1-owning-artifacts.log",
    }
    projections = []
    for n in sorted(names):
        raw = locate(n)
        published = raw
        if n in cargo:
            kept = []
            for line in raw.decode(errors="replace").splitlines():
                try:
                    a = json.loads(line)
                except ValueError:
                    continue
                if a.get("reason") == "compiler-artifact":
                    kept.append(line)
            assert kept, n
            published = ("\n".join(kept) + "\n").encode()
            projections.append(
                {
                    "path": n,
                    "kind": "cargo-artifact-only",
                    "original": fingerprint(raw),
                    "published": fingerprint(published),
                }
            )
        if n == "macos-nls-sorting-candidate1-owning-build.log":
            published = b"Private Assist compiler transcript retained locally; owning receipt retains the successful result and original byte identity.\n"
            projections.append(
                {
                    "path": n,
                    "kind": "private-build-transcript-withheld",
                    "original": fingerprint(raw),
                    "published": fingerprint(published),
                }
            )
        (V / n).write_bytes(published)
    (V / "public-projections.json").write_text(
        json.dumps({"files": projections}, indent=2) + "\n"
    )
    print("declared", len(names), "projected", len(projections))


if __name__ == "__main__":
    main()
