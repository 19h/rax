from pathlib import Path
import json, shutil, time

Q = Path(__file__).resolve().parent
S = Path("/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
D = Path(
    "/Users/int/.assist-native-display-policy-20261022/docs/specifications/windows/native-gre-initialize/evidence"
)
F = Q / "gre-initialize-linux-candidate1-retry-v3-gates.json"
while True:
    try:
        state = json.loads(F.read_bytes())
    except json.JSONDecodeError:
        time.sleep(1)
        continue
    if state["state"] in ["finished", "failed"]:
        break
    time.sleep(3)
assert state["state"] == "finished" and state["exit-code"] == 0
names = [
    "native-gre-initialize-v1-four-views-launch.json",
    "native-gre-repeat-v1.json",
    "native-gre-probe-cleanup-v1.json",
    "gre-initialize-candidate1-windows.json",
    "gre-initialize-native-owning-candidate1.json",
    "native-gre-initialize-candidate1-pre-cpp-archives.json",
    "native-gre-initialize-candidate1-final-owning-archive-identities.json",
    "gre-initialize-owning-capi-candidate1.json",
    "mutant-native-baseline-failures.json",
    "current-cpp-consumers-wait-root.json",
    "inspect-system-time-v2-pe.py",
]
names += [f"native-gre-next-frontier-v{v}.json" for v in [1, 2, 3, 4]]
for name in names:
    source = S / name
    if not source.exists():
        source = Q / name
    assert source.is_file(), name
    if source != Q / name:
        shutil.copy2(source, Q / name)


def logs(value):
    if isinstance(value, dict):
        for item in value.values():
            yield from logs(item)
    elif isinstance(value, list):
        for item in value:
            yield from logs(item)
    elif isinstance(value, str) and value.endswith(".log"):
        yield value.replace("\\", "/").split("/")[-1]


for path in list(Q.glob("*.json")):
    document = json.loads(path.read_bytes())
    assert not isinstance(document, dict) or document.get("state") not in [
        "running",
        "preparing",
        "starting",
        "waiting-for-focus",
    ], path
    if not isinstance(document, dict) or document.get("state") not in [
        "finished",
        "failed",
    ]:
        continue
    for step in document.get("steps", []):
        value = step.get("log")
        if not value:
            continue
        if isinstance(value, dict):
            value = value.get("file", value.get("path"))
        name = value.replace("\\", "/").split("/")[-1]
        if not (Q / name).exists():
            assert (S / name).is_file(), name
            shutil.copy2(S / name, Q / name)
D.mkdir(exist_ok=True)
excluded = {
    "native-gre-initialize-v1.cpp",
    *[
        f"native-gre-initialize-v1-four-views-{view}.log"
        for view in ["arm64", "x64", "x86", "x86-laa"]
    ],
}
for path in sorted(Q.iterdir()):
    if path.is_file() and path.name not in excluded:
        target = D / path.name
        assert not target.exists() or target.read_bytes() == path.read_bytes(), path
        shutil.copy2(path, target)
print("Retained evidence files", len(list(D.iterdir())), flush=True)
