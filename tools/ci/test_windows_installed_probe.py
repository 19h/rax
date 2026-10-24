"""The diagnostic branch walker cannot escape or loop through native memory."""

import struct
import unittest

from windows_installed_probe_v2 import code_chain


class BoundedCodeObservation(unittest.TestCase):
    def test_signed_x64_branch_uses_image_relative_target(self):
        image = bytearray(128)
        image[100:105] = b"\xe9" + struct.pack("<i", -85)
        reads = []

        def read(offset, length):
            reads.append((offset, length))
            return bytes(image[offset : offset + length])

        rows = code_chain(read, len(image), 0x8664, 100)
        self.assertEqual([row["rva"] for row in rows], [100, 20])
        self.assertEqual(reads, [(100, 28), (20, 64)])

    def test_outside_target_and_cycles_never_cause_another_read(self):
        for displacement in (-5, 100):
            with self.subTest(displacement=displacement):
                reads = []

                def read(offset, length):
                    reads.append((offset, length))
                    return b"\xe9" + struct.pack("<i", displacement) + bytes(59)

                with self.assertRaises(RuntimeError):
                    code_chain(read, 64, 0x8664, 0)
                self.assertEqual(reads, [(0, 64)])

    def test_arm64_negative_branch_and_observation_budget(self):
        image = bytearray(128)
        image[80:84] = struct.pack("<I", 0x14000000 | ((-15) & 0x3FFFFFF))
        read = lambda offset, length: bytes(image[offset : offset + length])
        rows = code_chain(read, 128, 0xAA64, 80)
        self.assertEqual([row["rva"] for row in rows], [80, 20])

        reads = []

        def chain(offset, length):
            reads.append(offset)
            return b"\xe9\0\0\0\0" + bytes(length - 5)

        with self.assertRaisesRegex(RuntimeError, "four-hop"):
            code_chain(chain, 128, 0x14C, 0)
        self.assertEqual(reads, [0, 5, 10, 15])


if __name__ == "__main__":
    unittest.main()
