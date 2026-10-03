"""Fixtures for the Rust port of the exe's CRT qsort (0x64baaf), made by running the exe.

The game numbers continents with this qsort and its comparator returns 0 on ties, so the
ids of equal-sized islands are whatever the unstable sort leaves. The port has to
reproduce that, so it is checked against the exe itself.

    /tmp/emu/bin/python gen_qsort_fixtures.py [seed] [cases] > rust/tests/data/crt_qsort.txt

One line per case:  n | key[0] .. key[n-1] | order[0] .. order[n-1]
`order` is the int array the exe leaves after qsort(order, n, 4, cmp) with
cmp(a, b) = key[*a] - key[*b], starting from order = 0..n-1.
"""
import random
import struct
import sys

from emu import Emu

QSORT = 0x64BAAF


def comparator(keys_addr):
    code = bytes.fromhex(
        "8B442404"  # mov eax,[esp+4]
        "8B00"  # mov eax,[eax]
        "8B4C2408"  # mov ecx,[esp+8]
        "8B09"  # mov ecx,[ecx]
    )
    code += b"\x8B\x04\x85" + struct.pack("<I", keys_addr)  # mov eax,[eax*4+keys]
    code += b"\x8B\x0C\x8D" + struct.pack("<I", keys_addr)  # mov ecx,[ecx*4+keys]
    code += bytes.fromhex("2BC1C3")  # sub eax,ecx ; ret
    return code


def main():
    rnd = random.Random(int(sys.argv[1]) if len(sys.argv) > 1 else 3)
    cases = int(sys.argv[2]) if len(sys.argv) > 2 else 120
    e = Emu()
    keys_addr = e.alloc(4 * 256)
    cmp_addr = e.alloc(64)
    e.write(cmp_addr, comparator(keys_addr))
    arr = e.alloc(4 * 256)
    for _ in range(cases):
        n = rnd.choice([2, 3, 5, 8, 9, 10, 13, 17, 24, 40, 77, 160, 220])
        span = rnd.choice([1, 2, 3, 6, 40])
        keys = [rnd.randrange(span) for _ in range(n)]
        e.write(keys_addr, struct.pack(f"<{n}i", *keys))
        e.write(arr, struct.pack(f"<{n}I", *range(n)))
        e.call(QSORT, (arr, n, 4, cmp_addr))
        order = struct.unpack(f"<{n}I", e.read(arr, 4 * n))
        assert sorted(order) == list(range(n))
        print(n, "|", *keys, "|", *order)


if __name__ == "__main__":
    main()
