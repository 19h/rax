"""Verify original CSR replies and matching readonly metadata primary evidence."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(row):
    name = row["path"].replace("\\", "/").rsplit("/", 1)[-1]
    data = (ROOT / name).read_bytes()
    require(len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], name + " identity")
    return data.decode("utf-8").replace("\r\n", "\n")


def dump(text, name):
    rows = re.findall(rf"^dump name={re.escape(name)} .*requested=(\d+) ok=1 error=0 got=(\d+) hex=([A-F0-9]*)$", text, re.M)
    require(len(rows) == 1, name + " unique successful dump")
    requested, got, data = rows[0]
    data = bytes.fromhex(data)
    require(int(requested) == int(got) == len(data), name + " bounded exact copy")
    return data


def word(data, offset, size=8):
    require(offset + size <= len(data), "bounded field")
    return int.from_bytes(data[offset:offset + size], "little")


def main():
    require(__debug__, "optimized verification rejected")
    identities = json.loads((ROOT / "native-csr-metadata-identities.json").read_text())
    verify(identities["inspector"])
    metadata = {}
    for row in identities["producers"]:
        verify(row["source"])
        text = verify(row["stdout"])
        peb, prefix = dump(text, "peb"), dump(text, "selected-prefix")
        require(word(peb, 0x98) - word(peb, 0x88) == 0x750, "own PEB statics offset")
        require(word(peb, 0x380) == word(prefix, 40) == word(prefix, 48), "same selected server base")
        require(word(peb, 0x2C0, 4) == 0 and word(peb, 0x120, 2) == 29683, "session0/build29683")
        require("complete status=0 read-only=1" in text, "readonly producer terminal state")
        if "server-pid" in row:
            value = re.findall(r"^csr-pid-value value=([A-F0-9]+)$", text, re.M)
            require(len(value) == 1, "pointer-width getter value")
            metadata = {"base": word(prefix, 40), "pid": int(value[0], 16)}
    require(metadata == {"base": 0x7FF5F57C0000, "pid": 0x348}, "retained replay profile")
    identities = json.loads((ROOT / "native-secure-connect-arm64-identities.json").read_text())
    verify(identities["inspector"])
    for row in identities["producers"]:
        verify(row["source"])
        text = verify(row["stdout"])
        before, after = dump(text, "connection-before"), dump(text, "connection-after")
        require(len(before) == len(after) == 48 and before[:32] == bytes(32), "original startup packet")
        require(word(after, 0) == metadata["base"] and word(after, 8) == metadata["base"] + 0x750
                and word(after, 16) == metadata["pid"] and word(after, 24) == 0, "matched CSR reply fields")
        require(before[32:] == after[32:] and word(before, 32) == 1, "untouched trailing16 bytes")
        require(word(dump(text, "max-message-after"), 0, 4) == 952, "native maximum message bytes")
        require(dump(text, "security-qos") == bytes.fromhex("000000000200000001010000"), "native QoS profile")
        require(dump(text, "required-server-sid") == bytes.fromhex("010100000000000512000000"), "native System SID")
        view = dump(text, "client-view-after")
        require(len(view) == 48 and word(view, 24) == 65536 and word(view, 32) and word(view, 40), "actual successful client view geometry")
        require("return ordinal=0 status=00000000" in text and text.splitlines()[-1] == "complete captured=1 calls=1 exit=41", "natural original child exit")
        if row["kind"] == "secure-connect-first-call2":
            require(dump(text, "server-view-after") == bytes(24), "corrected fixed24 remote output")
        # The first producer used post-call Length0 when dumping remote output.
        # It is retained but is explicitly not evidence of the remote24 bytes.
    observer = json.loads((ROOT / "native-csr-startup-calls-identities.json").read_text())
    verify(observer["source"])
    verify(observer["inspector"])
    text = verify(observer["stdout"])
    require(text.splitlines()[-1] == "complete installed=1 entered=1 returned=1 exit=41", "bounded four-entry observer terminal")
    require(len(re.findall(r"^entry ordinal=", text, re.M)) == 1 and "entry ordinal=0 name=NtSecureConnectPort" in text, "single monitored secure connection")
    for name in ("NtSecureConnectPort", "NtAlpcConnectPort", "NtAlpcSendWaitReceivePort", "NtRequestWaitReplyPort"):
        require("hook name=" + name in text, "installed monitored export " + name)
    print("Verified two original CSR replies, two readonly metadata producers, and one independently checked four-entry observer; natural native child exits41,emulated ordinary exits0")


if __name__ == "__main__":
    main()
