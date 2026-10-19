"""Independent resource encoder for the LZFSE raw/LZVN block profile.

Wire fields follow Apple's published LZFSE block headers and LZVN opcode
descriptions. This deliberately emits no FSE (bvx1/bvx2) blocks. The resource
decoder accepts this bounded profile, not arbitrary third-party LZFSE streams.
No third-party encoder or decoder implementation is used here.
"""

import struct
from collections import deque

MAX_DECODED = 64 * 1024 * 1024
END = b"bvx$"


def compress(data: bytes) -> bytes:
    """Deterministic greedy LZ77 matching with at most eight candidates per key."""
    if len(data) > MAX_DECODED:
        raise ValueError("resource exceeds the 64 MiB decoded limit")
    stream = bytearray()
    positions = {}

    def literals(start, end):
        while start < end:
            count = min(end - start, 271)
            if count < 16:
                stream.append(0xE0 | count)
            else:
                stream.extend((0xE0, count - 16))
            stream.extend(data[start : start + count])
            start += count

    def remember(position):
        if position >= 65536:
            stale = position - 65536
            key = data[stale : stale + 4]
            previous = positions.get(key)
            if previous and previous[0] == stale:
                previous.popleft()
                if not previous:
                    del positions[key]
        if position + 4 <= len(data):
            positions.setdefault(data[position : position + 4], deque(maxlen=8)).append(
                position
            )

    cursor = pending = 0
    while cursor + 4 <= len(data):
        best_length = best_distance = 0
        candidates = positions.get(data[cursor : cursor + 4], ())
        for earlier in reversed(candidates):
            distance = cursor - earlier
            if distance > 65535:
                break
            length = 4
            maximum = min(len(data) - cursor, 4096)
            while length < maximum and data[earlier + length] == data[cursor + length]:
                length += 1
            if length > best_length:
                best_length, best_distance = length, distance
        if best_length < 4:
            remember(cursor)
            cursor += 1
            continue
        literals(pending, cursor)
        first = min(best_length, 10)
        # No coupled literals: LL=0, M=length-3, distance is little endian.
        stream.append(((first - 3) << 3) | 7)
        stream.extend(struct.pack("<H", best_distance))
        remaining = best_length - first
        while remaining:
            count = min(remaining, 271)
            if count < 16:
                stream.append(0xF0 | count)
            else:
                stream.extend((0xF0, count - 16))
            remaining -= count
        for position in range(cursor, cursor + best_length):
            remember(position)
        cursor += best_length
        pending = cursor
    literals(pending, len(data))
    stream.extend(b"\x06" + bytes(7))
    compressed = b"bvxn" + struct.pack("<II", len(data), len(stream)) + stream + END
    raw = b"bvx-" + struct.pack("<I", len(data)) + data + END
    return compressed if len(compressed) < len(raw) else raw
