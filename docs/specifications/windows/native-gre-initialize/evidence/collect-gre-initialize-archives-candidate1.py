from pathlib import Path
import argparse
import hashlib
import json


def identity(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return {
        "path": str(path),
        "bytes": path.stat().st_size,
        "sha256": digest.hexdigest(),
    }


def archive(path):
    row = identity(path)
    count = 0
    size = row["bytes"]
    with path.open("rb") as stream:
        if stream.read(8) != b"!<arch>\n":
            raise ValueError("archive magic " + str(path))
        while stream.tell() < size:
            header = stream.read(60)
            if len(header) != 60 or header[58:] != b"`\n":
                raise ValueError("member header")
            length = int(header[48:58])
            end = stream.tell() + length + (length & 1)
            if length < 0 or end > size:
                raise ValueError("member extent")
            stream.seek(end)
            count += 1
        if stream.tell() != size:
            raise ValueError("archive extent")
    return {**row, "members": count, "structural-member-walk": "pass"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--messages", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    for row in manifest["sources"]:
        actual = identity(args.source / row["path"])
        if (actual["bytes"], actual["sha256"]) != (row["bytes"], row["sha256"]):
            raise ValueError("current source " + row["path"])
    core = carrier = None
    for line in args.messages.read_text().splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        name = message["target"]["name"]
        if (
            name == "rax"
            and Path(message["manifest_path"]).parent.resolve() == args.source.resolve()
        ):
            core = message
        if name == "assist_rs":
            carrier = message
    if core is None or carrier is None:
        raise ValueError("actual core/carrier compiler-artifact records missing")
    if core["features"] != []:
        raise ValueError("core feature set")
    rlib = next(Path(name) for name in core["filenames"] if name.endswith(".rlib"))
    library = next(
        Path(name) for name in carrier["filenames"] if name.endswith((".a", ".lib"))
    )
    result = {
        "host": args.host,
        "source-epoch": "native-gre-initialize-candidate1",
        "sources": manifest["sources"],
        "core-features": [],
        "compiler-messages": identity(args.messages),
        "core-artifact": core,
        "carrier-artifact": carrier,
        "archives": [archive(library), archive(rlib)],
        "scope": "actual current Cargo artifact paths and full archive walks; retained Assist carrier, not whole current root",
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(
        args.host,
        [(row["bytes"], row["members"], row["sha256"]) for row in result["archives"]],
    )


if __name__ == "__main__":
    main()
