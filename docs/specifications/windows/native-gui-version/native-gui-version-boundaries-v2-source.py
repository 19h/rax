from pathlib import Path
import struct, hashlib, subprocess, json, base64, os, re

R = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
source = R / "target/native-win32k-callback-v5/probe.exe"
original = source.read_bytes()
assert (
    hashlib.sha256(original).hexdigest()
    == "3492609b8ccd9dc96ebf3c13c12db656c6330c7606aacc302968a837b4640084"
)
root = R / "target/native-gui-version-boundaries-v2"
assert not root.exists()
root.mkdir()
(root / "tmp").mkdir()
opt = struct.unpack_from("<I", original, 0x3C)[0] + 24
rows = []
for name, osv, ssv in [
    ("os100-subsystem6-255", (10, 0), (6, 255)),
    ("os100-subsystem7-256", (10, 0), (7, 256)),
    ("os100-subsystem8-256", (10, 0), (8, 256)),
    ("os100-subsystem7-65535", (10, 0), (7, 65535)),
]:
    raw = bytearray(original)
    struct.pack_into("<HH", raw, opt + 40, *osv)
    struct.pack_into("<HH", raw, opt + 48, *ssv)
    exe = root / (name + ".exe")
    exe.write_bytes(raw)
    before = hashlib.sha256(raw).hexdigest()
    log = root / (name + ".log")
    env = dict(
        os.environ,
        TMP=str(root / "tmp"),
        TEMP=str(root / "tmp"),
        TMPDIR=str(root / "tmp"),
    )
    with log.open("wb") as out:
        r = subprocess.run(
            [str(exe), "normal"],
            cwd=root,
            env=env,
            stdout=out,
            stderr=subprocess.STDOUT,
            timeout=45,
        )
    after = hashlib.sha256(exe.read_bytes()).hexdigest()
    assert before == after
    b = log.read_bytes()
    rows.append(
        {
            "name": name,
            "os-version": osv,
            "subsystem-version": ssv,
            "bytes": len(raw),
            "exe-sha256-before": before,
            "exe-sha256-after": after,
            "exit-code": r.returncode,
            "log": log.name,
            "log-bytes": len(b),
            "log-sha256": hashlib.sha256(b).hexdigest(),
            "observed-client-version-words": re.findall(
                r"^delta=teb offset=0810 before=[0-9a-fA-F]+ after=([0-9a-fA-F]+)",
                b.decode(),
                re.MULTILINE,
            ),
        }
    )
assert source.read_bytes() == original
record = {
    "state": "finished",
    "scope": "Independent PE subsystem minor-width controls in owned native processes; original source executable unchanged.",
    "source-executable-sha256": hashlib.sha256(original).hexdigest(),
    "profiles": rows,
}
(root / "receipt.json").write_text(json.dumps(record, indent=2) + "\n")
print(
    json.dumps(
        {
            "state": "finished",
            "source-executable-sha256": hashlib.sha256(original).hexdigest(),
            "profiles": rows,
        }
    )
)
