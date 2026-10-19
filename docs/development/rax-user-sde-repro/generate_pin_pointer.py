#!/usr/bin/env python3
"""Create a minimal ELF64 regression for Pin's RIP-relative pointer load."""

from pathlib import Path
import struct
import sys


def main():
    output = Path(sys.argv[1])
    base = 0x400000
    entry = base + 0x1000
    slot = base + 0x1800
    callee = base + 0x1100
    # PINSRQ xmm0,[rip+disp32],1; PEXTRQ rax,xmm0,1; CALL rax;
    # MOV eax,60; XOR edi,edi; SYSCALL. The callee contains RET.
    code = bytes.fromhex("66 48 0f 3a 22 05")
    code += struct.pack("<i", slot - (entry + 11)) + b"\x01"
    code += bytes.fromhex("66 48 0f 3a 16 c0 01 ff d0 b8 3c 00 00 00 31 ff 0f 05")
    elf = bytearray(0x2000)
    elf[:64] = struct.pack(
        "<16sHHIQQQIHHHHHH",
        b"\x7fELF\x02\x01\x01" + b"\0" * 9,
        2, 62, 1, entry, 64, 0, 0, 64, 56, 1, 0, 0, 0,
    )
    elf[64:120] = struct.pack(
        "<IIQQQQQQ", 1, 7, 0, base, base, len(elf), len(elf), 0x1000
    )
    elf[0x1000:0x1000 + len(code)] = code
    elf[0x1100] = 0xC3
    elf[0x1800:0x1808] = struct.pack("<Q", callee)
    with output.open("xb") as destination:
        destination.write(elf)
    output.chmod(0o755)


if __name__ == "__main__":
    main()
