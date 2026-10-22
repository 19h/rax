"""Validate frozen GRE selection source, native observations and execution gates."""

from pathlib import Path
import argparse, hashlib, json, os, re, tarfile

BASE = "a9ca828fa18ab92a8a3ed866da97f360030b079c"
FRAME = "e2994771bbad47110bbe6c7ed3b9a2973711e2388f3cf7fa612169cdfa7ce122"


def require(value, message):
    if not value:
        raise ValueError(message)


def pair(row):
    return row["bytes"], row["sha256"]


def identity(raw):
    return len(raw), hashlib.sha256(raw).hexdigest()


def load(path):
    return json.loads(path.read_bytes())


def fields(line):
    return dict(word.split("=", 1) for word in line.split()[1:])


def check(directory, root):
    require(__debug__, "optimized Python is not an evidence checker")
    evidence = directory / "evidence"
    proof = load(directory / "proof.json")
    require(proof["schema"] == 1 and proof["base"] == BASE, "parent/schema")
    paths = set()
    for row in proof["files"]:
        path = Path(row["path"])
        require(not path.is_absolute() and ".." not in path.parts, "evidence path")
        require(row["path"] not in paths, "duplicate evidence")
        paths.add(row["path"])
        require(
            identity((directory / path).read_bytes()) == pair(row),
            "inventory " + str(path),
        )
    actual = {
        str(p.relative_to(directory)).replace("\\", "/")
        for folder in ["native", "evidence"]
        for p in (directory / folder).rglob("*")
        if p.is_file()
    } | {"sources.json"}
    require(paths == actual, "complete evidence inventory")

    def receipt(name):
        return load(evidence / name)

    def log(row):
        name = row.get("log", row.get("file", row.get("path")))
        name = name.replace("\\", "/").split("/")[-1]
        path = evidence / name
        if not path.exists():
            path = directory / "native" / name
        raw = path.read_bytes()
        require(identity(raw) == pair(row), "receipt log " + name)
        return raw.decode()

    source = evidence / "gre-initialize-candidate1-source.json"
    require(identity(source.read_bytes())[1] == FRAME, "compiler frame digest")
    frame = load(source)
    require(
        frame["base"] == BASE
        and len(frame["files"]) == 29180
        and len(frame["overrides"]) == 11,
        "compiler frame extent/base",
    )
    source_rows = {row["path"]: row for row in frame["files"]}
    require(len(source_rows) == 29180, "duplicate compiler inputs")
    for row in frame["files"]:
        path = root / row["path"]
        raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        expected = pair(row)
        if os.name == "nt" and row["path"] == ".cargo/config.toml":
            require(
                expected
                == (
                    552,
                    "a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd",
                ),
                "host config",
            )
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        require(identity(raw) == expected, "compiler input " + row["path"])
    require(
        identity((evidence / frame["archive"]).read_bytes()) == pair(frame),
        "source archive",
    )
    with tarfile.open(evidence / frame["archive"], "r:gz") as bundle:
        expected = {row["path"]: row for row in frame["overrides"]}
        members = bundle.getmembers()
        require(
            len(members) == 11 and {m.name for m in members} == set(expected),
            "source overlay selection",
        )
        for member in members:
            require(
                member.isfile()
                and identity(bundle.extractfile(member).read())
                == pair(expected[member.name]),
                "source overlay bytes",
            )

    launch = receipt("native-gre-initialize-v1-four-views-launch.json")
    require(
        launch["state"] == "finished"
        and launch["exit-code"] == 0
        and len(launch["steps"]) == 16
        and len(launch["outcomes"]) == 4,
        "native launch complete",
    )
    require(
        launch["source-before"] == launch["source-after"]
        and identity((directory / "native/native-gre-initialize-v1.cpp").read_bytes())
        == pair(launch["source-before"]),
        "native producer source",
    )
    require(
        launch["runtime-modules-before"] == launch["runtime-modules-after"]
        and len(launch["runtime-modules-before"]) == 12,
        "native DLL stability",
    )
    for step in launch["steps"]:
        require(step["exit-code"] == 0, "native step exit")
        text = log(step)
        if step["name"].endswith(("-before", "-after")):
            pe = json.loads(text)
            view = pe["profile"]
            require(
                pe["machine"]
                == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                    view
                ]
                and pe["large-address-aware"] == (view != "x86"),
                "native PE selection",
            )
    canonical = None
    native_lines = {}
    for outcome in launch["outcomes"]:
        view = outcome["profile"]
        require(
            outcome["exit-code"] == 0
            and outcome["executable-before"] == outcome["executable-after"],
            "native executable stability",
        )
        name = f"native-gre-initialize-v1-four-views-{view}.log"
        fixture = directory / "native" / name
        require(
            identity(fixture.read_bytes())
            == pair(
                source_rows[
                    "docs/specifications/windows/native-gre-initialize/native/" + name
                ]
            ),
            "exact compiled fixture " + name,
        )
        text = log(next(row for row in launch["steps"] if row["name"] == view))
        lines = text.splitlines()
        native_lines[view] = lines
        groups = {}
        opens = []
        for line in lines:
            if line.startswith(("TREE ", "VALUE ", "CHILD ", "TREE_END ")):
                tag = line.split()[0]
                row = fields(line)
                key = (row.pop("sample"), row.pop("view"))
                groups.setdefault(key, []).append((tag, row))
            elif line.startswith("OPEN "):
                opens.append(fields(line))
        require(
            set(groups) == {(s, v) for s in ["0", "1"] for v in ["000", "100", "200"]},
            "complete tree samples/views",
        )
        for group in groups.values():
            if canonical is None:
                canonical = group
            require(group == canonical, "complete raw tree equality")
            nodes = [row for kind, row in group if kind == "TREE"]
            values = [row for kind, row in group if kind == "VALUE"]
            end = next(row for kind, row in group if kind == "TREE_END")
            require(
                len(nodes) == 3
                and len(values) == 7
                and all(row["status"] == "00000000" for row in nodes),
                "complete native nodes/values",
            )
            require(
                sum(len(bytes.fromhex(row["name"])) for row in values) == 180
                and sum(len(bytes.fromhex(row["data"])) for row in values) == 142,
                "native name/data extents",
            )
            require(
                end == {"keys": "3", "values": "7", "bytes": "322", "paths": "502"},
                "native tree budgets",
            )
            for row in values:
                require(
                    len(bytes.fromhex(row["data"])) == int(row["bytes"]),
                    "lossless native value length",
                )
        require(
            len(opens) == 144
            and len({(row["spelling"], row["flags"], row["grant"]) for row in opens})
            == 144,
            "complete native open matrix",
        )
        for row in opens:
            require(
                row["exception"] == "00000000" and row["last-error"] == "12345678",
                "native open envelope",
            )
            require(
                int(row["status"], 16)
                == (0xC0000022 if int(row["grant"], 16) == 0 else 0),
                "native access-mask observation",
            )
            require(
                (int(row["handle"], 16) != 0) == (int(row["status"], 16) == 0),
                "native returned handle",
            )
        summary = fields(next(line for line in lines if line.startswith("SUMMARY ")))
        require(
            summary["calls"] == "144"
            and summary["opened"] == summary["closed"] == "128"
            and summary["handles-before"] == summary["handles-after"],
            "native handle balance",
        )
    repeat = receipt("native-gre-repeat-v1.json")
    require(
        repeat["state"] == "finished"
        and repeat["exit-code"] == 0
        and len(repeat["steps"]) == 4,
        "native repeat complete",
    )
    require(
        repeat["runtime-before"]
        == repeat["runtime-after"]
        == launch["runtime-modules-before"],
        "repeat native DLLs",
    )
    for step in repeat["steps"]:
        require(
            step["exit-code"] == 0
            and step["calls"] == 144
            and step["executable-before"] == step["executable-after"],
            "repeat native execution",
        )
        require(
            step["read-tree-records-match"]
            and step["all-open-fields-match-except-opaque-handle-number"],
            "repeat native fields",
        )
        rows = log(step["log"]).splitlines()
        old = native_lines[step["profile"]]
        tree = lambda rs: [
            line
            for line in rs
            if line.startswith(("TREE ", "VALUE ", "CHILD ", "TREE_END "))
        ]
        require(tree(rows) == tree(old), "repeated full raw tree records")
        a = [fields(row) for row in rows if row.startswith("OPEN ")]
        b = [fields(row) for row in old if row.startswith("OPEN ")]
        require(len(a) == len(b) == 144, "repeat open extent")
        for x, y in zip(a, b):
            hx = int(x.pop("handle"), 16)
            hy = int(y.pop("handle"), 16)
            require(x == y and (hx != 0) == (hy != 0), "repeat exact open effects")

    failed = receipt("gre-initialize-macos-candidate1-gates.json")
    require(
        failed["state"] == "failed"
        and failed["exit-code"] == 1
        and failed["manifest-sha256"] == FRAME,
        "retained Mac failed run",
    )
    full = next(row for row in failed["steps"] if row["name"] == "full")
    text = log(full)
    require(
        full["exit-code"] == 101
        and "8198 passed; 5 failed; 2 ignored;" in text
        and "Resource temporarily unavailable" in text,
        "Mac fork outage result",
    )
    expected_forks = {
        "user::linux::tests::pidfd::a_host_watch_sees_another_process_end",
        "user::linux::tests::pidfd::another_process_is_seen_through_the_host",
        "user::linux::tests::ptrace_jobctl::sigstop_to_a_linked_process_goes_along_the_link",
        "user::mm::shared_words::tests::unix_mapping_remains_shared_after_fork",
        "user::readiness::tests::unix_levels_survive_fork",
    }
    observed = set(
        re.findall(r"^    (\S+)\r?$", text[text.rfind("\nfailures:\n") :], re.M)
    )
    require(observed == expected_forks, "Mac exact original fork failures")
    recheck = receipt("gre-initialize-macos-fork-recheck-v1.json")
    require(
        recheck["state"] == "finished"
        and recheck["exit-code"] == 0
        and recheck["image-before"] == recheck["image-after"]
        and recheck["source-frame-sha256"] == FRAME,
        "unchanged Mac fork recheck image",
    )
    require(
        {row["name"] for row in recheck["steps"]} == expected_forks,
        "all original fork failures rechecked",
    )
    for row in recheck["steps"]:
        require(
            row["exit-code"] == 0 and "1 passed; 0 failed;" in log(row),
            "fork assertion passed unchanged",
        )
    interrupted = receipt("gre-initialize-linux-candidate1-gates.json")
    require(
        interrupted["state"] == "failed"
        and "BlockingIOError" in interrupted["error"]
        and "Resource temporarily unavailable" in interrupted["error"],
        "Linux failed spawn retained",
    )
    require(
        len(interrupted["steps"]) == 1 and interrupted["steps"][0]["exit-code"] == 0,
        "Linux original compiled ordinal pass",
    )
    log(interrupted["steps"][0])
    stopped = receipt("gre-initialize-linux-candidate1-retry-v2-gates.json")
    require(
        stopped["state"] == "failed"
        and stopped["steps"][0]["exit-code"] == 1
        and "docker.sock" in log(stopped["steps"][0]),
        "Linux stopped service retained",
    )
    summaries = {}
    for host, version, count in [("macos", "v4", 8203), ("linux", "v3", 8198)]:
        gates = receipt(f"gre-initialize-{host}-candidate1-retry-{version}-gates.json")
        require(
            gates["state"] == "finished"
            and gates["exit-code"] == 0
            and gates["manifest-sha256"] == FRAME,
            "Unix complete source-bound gates",
        )
        require(
            [s["name"] for s in gates["steps"]]
            == ["ordinals", "focus", "full", "capi", "integration", "all-targets"],
            "Unix gate selection",
        )
        for step, n in zip(gates["steps"], [4, 7, count, 183, 544, None]):
            text = log(step)
            require(step["exit-code"] == 0, "Unix gate exit")
            if n is not None:
                require(f"{n} passed; 0 failed;" in text, "Unix pass count")
            if step["name"] == "full":
                require(
                    "2 ignored; 0 measured; 0 filtered out;" in text,
                    "Unix unfiltered library",
                )
        summaries[host] = {
            "focus": 7,
            "full-passed": count,
            "full-failed": 0,
            "full-ignored": 2,
        }
    windows = receipt("gre-initialize-candidate1-windows.json")
    require(
        windows["state"] == "finished"
        and windows["exit-code"] == 0
        and windows["manifest"]["sha256"] == FRAME
        and windows["source-inputs"] == 29180,
        "Windows complete source-bound gates",
    )
    require(
        windows["test-image-before"] == windows["test-image-after"],
        "Windows exact test executable",
    )
    failures = set(receipt("mutant-native-baseline-failures.json")["failures"])
    require(
        len(failures) == 5 and set(windows["exact-full-failures"]) == failures,
        "Windows exact remaining failures",
    )
    by_name = {row["name"]: row for row in windows["steps"]}
    for name, n in [
        ("ordinals", 4),
        ("focus", 10),
        ("capi", 183),
        ("integration", 4),
        ("all-targets", None),
    ]:
        text = log(by_name[name])
        require(by_name[name]["exit-code"] == 0, "Windows gate exit")
        if n is not None:
            require(f"{n} passed; 0 failed;" in text, "Windows pass count")
    text = log(by_name["full"])
    observed = set(
        re.findall(r"^    (\S+)\r?$", text[text.rfind("\nfailures:\n") :], re.M)
    )
    require(
        by_name["full"]["exit-code"] == 101
        and "7678 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in text
        and observed == failures,
        "Windows truthful full result",
    )
    summaries["windows"] = {
        "focus": 10,
        "full-passed": 7678,
        "full-failed": 5,
        "full-ignored": 2,
    }

    for host in ["macos", "linux", "windows"]:
        version = "v3" if host == "linux" else "v2"
        name = (
            f"{host}-gre-initialize-candidate1-owning-gates-{version}.json"
            if host != "windows"
            else "gre-initialize-native-owning-candidate1.json"
        )
        gates = receipt(name)
        require(
            gates["state"] == "finished"
            and gates["exit-code"] == 0
            and gates["archive-identities-stable"],
            "owning consumer gates",
        )
        for row in gates["steps"]:
            require(row["exit-code"] == 0, "owning consumer exit")
            log(row)
        prefix = (
            f"{host}-gre-initialize-candidate1"
            if host != "windows"
            else "native-gre-initialize-candidate1"
        )
        before = receipt(
            prefix
            + (
                "-archives-before.json"
                if host != "windows"
                else "-pre-cpp-archives.json"
            )
        )
        after = receipt(
            prefix
            + (
                "-archives-after.json"
                if host != "windows"
                else "-final-owning-archive-identities.json"
            )
        )
        require(
            before["archives"] == after["archives"] and len(before["archives"]) == 2,
            "actual owning archives stable",
        )
        require(before["core-features"] == [], "embedded core feature contract")
        for archive in before["archives"]:
            require(
                archive["structural-member-walk"] == "pass"
                and archive["members"] > 0
                and archive["bytes"] > 0,
                "full archive structural walk",
            )
        name = (
            "gre-initialize-owning-source-hashes-candidate1-linux-retry-v3.json"
            if host == "linux"
            else "gre-initialize-owning-source-hashes-candidate1.json"
        )
        owned = receipt(name)
        require(
            owned["source-frame"]["sha256"] == FRAME
            and len(owned["sources"]) == 3981
            and owned["source-parent-rax"] == BASE,
            "owning source frame and parent",
        )
        for row in owned["sources"]:
            require(
                row["path"] in source_rows
                and pair(row) == pair(source_rows[row["path"]]),
                "owning source compiler binding",
            )
    quality = receipt("gre-initialize-quality-v1.json")
    require(
        quality["state"] == "finished"
        and quality["exit-code"] == 0
        and quality["source-frame-sha256"] == FRAME
        and quality["owned-rust-files"] == 7,
        "formatter/Clippy source",
    )
    require(
        [row["name"] for row in quality["steps"]]
        == ["rustfmt", "strict-lib-clippy", "all-target-clippy"],
        "quality selection",
    )
    for row in quality["steps"]:
        require(
            row["exit-code"] == 0 and "warning:" not in log(row),
            "formatter/Clippy result",
        )
    parent = receipt("native-gre-parent-regression-v1.json")
    require(
        parent["state"] == "finished"
        and parent["expected-regression-observed"]
        and parent["exit-code"] == 0,
        "parent regression control",
    )
    require(
        parent["core-before"] == parent["core-after"]
        and parent["observer-before"] == parent["observer-after"],
        "parent core/source stable",
    )
    require(
        [row["exit-code"] for row in parent["steps"]] == [0, 0, 101],
        "compiled expected parent failure",
    )
    text = log(next(row for row in parent["steps"] if row["name"] == "whoami"))
    require(
        "GRE_Initialize" in text
        and "independently present GRE_Initialize root remains outside" in text
        and "terminal turn=2872" in text,
        "actual owning parent failure observed",
    )
    archives = receipt(
        "native-gre-initialize-candidate1-final-owning-archive-identities.json"
    )["archives"]
    core = next(row for row in archives if row["path"].endswith(".rlib"))
    carrier = next(row for row in archives if row["path"].endswith(".lib"))
    for version in [1, 2, 3, 4]:
        ordinary = receipt(f"native-gre-next-frontier-v{version}.json")
        require(
            ordinary["state"] == "finished"
            and ordinary["exit-code"] == 0
            and ordinary["former-GRE-regression-cleared"],
            "ordinary Rust observer completed",
        )
        for field in ["core", "observer", "executable", "runtime"]:
            require(
                ordinary[field + "-before"] == ordinary[field + "-after"],
                "ordinary stable " + field,
            )
        require(
            pair(ordinary["core-before"]) == pair(core),
            "actual owning core observer binding",
        )
        if version == 1:
            require(
                pair(ordinary["observer-before"]) == pair(parent["observer-before"]),
                "same before/after regression source",
            )
        for row in ordinary["steps"]:
            require(row["exit-code"] == 0, "ordinary Rust observer exit")
            text = log(row)
            if row["name"] == "smoke":
                require("Complete(Exited(0))" in text, "Rust smoke guest exit")
            if row["name"] == "whoami":
                require(
                    "terminal turn=2875" in text
                    and "raw Windows arm64 service 0x12fd" in text
                    and "has no admitted stub" in text,
                    "Rust next unsupported frontier",
                )
                if version == 4:
                    require(
                        'module="win32u.dll" rva=0xbfd0' in text
                        and 'name="NtGdiInit2" rva=0xbfd0 displacement=0x0' in text,
                        "bounded code/export correspondence",
                    )
        for outcome in ordinary["outcomes"]:
            require(
                outcome["program-before"] == outcome["program-after"]
                and outcome["native-before"] == outcome["native-after"]
                and outcome["native-before"]["exit-code"] == 0,
                "ordinary native controls",
            )

    api = receipt("gre-initialize-owning-capi-candidate1.json")
    require(
        api["state"] == "finished" and api["exit-code"] == 0,
        "fresh C API observer complete",
    )
    for field in [
        "native",
        "programs",
        "runtime",
        "source",
        "carrier",
        "adapter-inputs",
        "executable",
    ]:
        require(
            api[field + "-before"] == api[field + "-after"], "C API stable " + field
        )
    require(
        pair(api["carrier-before"]) == pair(carrier),
        "actual owning carrier observer binding",
    )
    for row in api["steps"]:
        require(row["exit-code"] == 0, "C API observer exit")
        log(row)
    smoke, ordinary = api["outcomes"]
    require(
        smoke["reason"] == 4
        and smoke["exit-code"] == 0
        and smoke["stdout-hex"] == smoke["stderr-hex"] == "",
        "C API smoke guest exit",
    )
    require(
        ordinary["reason"] == 5
        and ordinary["turns-started"] == 2876
        and "raw Windows arm64 service 0x12fd" in ordinary["diagnostic"]
        and "has no admitted stub" in ordinary["diagnostic"],
        "C API next frontier is not false guest exit",
    )
    require(
        all(row["exit-code"] == 0 for row in api["native-before"]),
        "native ordinary controls succeed",
    )
    cleanup = receipt("native-gre-probe-cleanup-v1.json")
    require(
        cleanup["state"] == "finished"
        and cleanup["exit-code"] == 0
        and cleanup["active-probe-processes"] == []
        and cleanup["removed"],
        "generated native probe cleanup",
    )
    return {
        "native-open-calls": 576,
        "native-repeat-open-calls": 576,
        "complete-tree-captures": 24,
        "complete-tree-repeat-captures": 24,
        "keys-per-tree": 3,
        "values-per-tree": 7,
        "name-and-data-bytes": 322,
        "compiler-inputs": 29180,
        "owning-consumer-hosts": 3,
        "gates": summaries,
        "next-boundary": "NtGdiInit2-corresponding Win32u leaf, no admitted service",
        "independent-x64-kernel": "unknown",
    }


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path, default=here)
    parser.add_argument("--source-root", type=Path, default=here.parents[3])
    args = parser.parse_args()
    print(json.dumps(check(args.evidence_dir, args.source_root), indent=2))


if __name__ == "__main__":
    main()
