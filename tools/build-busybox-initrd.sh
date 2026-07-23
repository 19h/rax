#!/usr/bin/env bash
# Build the repository's deterministic BusyBox initramfs.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
SOURCE_ROOT="$REPO_ROOT/initrd_root"
OUTPUT=${1:-"$REPO_ROOT/initrd.cpio.gz"}
if [[ $OUTPUT != /* ]]; then
    OUTPUT="$(pwd)/$OUTPUT"
fi
STAGING=$(mktemp -d /tmp/rax-busybox-initrd.XXXXXX)
ARCHIVE_TMP=$(mktemp "${OUTPUT}.tmp.XXXXXX")

cleanup() {
    rm -rf -- "$STAGING"
    rm -f -- "$ARCHIVE_TMP"
}
trap cleanup EXIT

command -v cpio >/dev/null
command -v cc >/dev/null
command -v fakeroot >/dev/null
command -v gzip >/dev/null

fakeroot -- sh -eu -c '
    source_root=$1
    staging=$2
    archive_tmp=$3

    cp -a "$source_root/." "$staging/"
    cp "$source_root/init.busybox" "$staging/etc/init.sh"
    chmod 0755 "$staging/etc/init.sh"
    cc -nostdlib -static -s -Wl,--build-id=none \
        -o "$staging/init" "$source_root/init.S"
    chmod 0755 "$staging/init"
    rm -f \
        "$staging/BUSYBOX.md" \
        "$staging/busybox.config" \
        "$staging/init.bak" \
        "$staging/init.busybox" \
        "$staging/init.S"

    # The kernel opens this node before execve(/init). Without it BusyBox does
    # run, but inherits unusable standard descriptors and appears silent.
    mknod -m 0600 "$staging/dev/console" c 5 1
    mknod -m 0600 "$staging/dev/ttyS0" c 4 64

    cd "$staging"
    find . -exec touch -h -d "@0" {} +
    find . -print0 \
        | LC_ALL=C sort -z \
        | cpio --null --create --format=newc --owner=0:0 --reproducible --quiet \
        | gzip -9 -n >"$archive_tmp"
' sh "$SOURCE_ROOT" "$STAGING" "$ARCHIVE_TMP"

mv -- "$ARCHIVE_TMP" "$OUTPUT"
chmod 0644 "$OUTPUT"
echo "built $OUTPUT"
