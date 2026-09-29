#!/usr/bin/env python3
"""Join the editor's flag *unpackers* with its property-page checkboxes.

The Civ3 scenario editor reads a row's flag dwords with the MSVC idiom

    mov  r, [row + SRC]      ; SRC = the flag dword inside the row
    shr  r, N                ; shr r,1 uses the short form d1 e8...
    and  r, 1
    mov  [obj + SLOT], r     ; SLOT = a per-record flag slot of the page object

and binds each slot to a dialog control with

    lea  r, [obj + SLOT]
    push r ; push <controlId> ; push <hwnd> ; call 0x4be16e   (tri-state check)

so `SRC/bit N -> SLOT -> controlId -> label` is recoverable statically: labels
come from `pe_dialogs.py`, the control id from the `lea/push/push/call` run.

Usage:
  python3 edit_slots.py <editor-exe> [--block LO:HI] [--src 0x8c] [--dialog 151]

Defaults target the Units page (dialog 151) reading the PRTO AI-strategy dword
at row `+0x8c`; its setter run is `0x459e7f..0x45a3c0`.
"""
import re
import struct
import sys

HERE = __file__.rsplit("/", 1)[0]
sys.path.insert(0, HERE)
from pe_dialogs import parse  # noqa: E402

KINDS = {0x80: "BUTTON", 0x81: "EDIT", 0x82: "STATIC", 0x83: "LISTBOX",
         0x84: "SCROLLBAR", 0x85: "COMBOBOX"}


def sections(data):
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    nsec = struct.unpack_from("<H", data, pe + 6)[0]
    optsz = struct.unpack_from("<H", data, pe + 20)[0]
    o = pe + 24 + optsz
    out = []
    for i in range(nsec):
        h = o + 40 * i
        name = data[h:h + 8].rstrip(b"\0").decode(errors="replace")
        vs, va, rs, ra = struct.unpack_from("<IIII", data, h + 8)
        out.append((name, va, vs, ra, rs))
    return pe, out


def dialogs(data, pe, secs):
    def rva2off(rva):
        for _, va, vs, ra, rs in secs:
            if va <= rva < va + max(vs, rs):
                return ra + (rva - va)

    rsrc_rva, _ = struct.unpack_from("<II", data, pe + 24 + 96 + 16)
    base = rva2off(rsrc_rva)

    def entries(off):
        nn, nid = struct.unpack_from("<HH", data, base + off + 12)
        out = []
        for i in range(nn + nid):
            e = base + off + 16 + 8 * i
            nm, sub = struct.unpack_from("<II", data, e)
            out.append((nm, sub))
        return out

    out = {}
    for nm, sub in entries(0):
        if nm != 5:
            continue
        for nm2, sub2 in entries(sub & 0x7FFFFFFF):
            for _, sub3 in entries(sub2 & 0x7FFFFFFF):
                if sub3 & 0x80000000:
                    continue
                drva, dsz = struct.unpack_from("<II", data, base + sub3)
                off = rva2off(drva)
                out[nm2] = data[off:off + dsz]
    return out


def bit_unpackers(buf, base, src):
    """`mov r,[reg+SRC]; shr r,N; and r,1; mov [reg+slot],r` -> (bit, slot)."""
    out = {}
    pat = re.compile(rb"\x8b[\x80-\xbf]" + struct.pack("<I", src)
                     + rb"\xc1[\xe8-\xef](.)\x83[\xe0-\xe7]\x01\x89[\x80-\xbf](.{4})", re.S)
    for m in pat.finditer(buf):
        out[struct.unpack("<I", m.group(2))[0]] = m.group(1)[0]
    pat1 = re.compile(rb"\x8b[\x80-\xbf]" + struct.pack("<I", src)
                      + rb"\xd1[\xe8-\xef]\x83[\xe0-\xe7]\x01\x89[\x80-\xbf](.{4})", re.S)
    for m in pat1.finditer(buf):
        out[struct.unpack("<I", m.group(1))[0]] = 1
    # bit 0 has no shift at all
    pat0 = re.compile(rb"\x8b[\x80-\xbf]" + struct.pack("<I", src)
                      + rb"\x83[\xe0-\xe7]\x01\x89[\x80-\xbf](.{4})", re.S)
    for m in pat0.finditer(buf):
        out.setdefault(struct.unpack("<I", m.group(1))[0], 0)
    return out


def slot_checkboxes(buf, base, lo, hi, helper=0x4BE16E):
    out = {}
    push = (0x50, 0x51, 0x52, 0x53, 0x56, 0x57)
    for i in range(lo - base, hi - base):
        if buf[i] != 0x8D or not 0x80 <= buf[i + 1] <= 0xBF:
            continue
        slot = struct.unpack_from("<I", buf, i + 2)[0]
        j = i + 6
        if buf[j] not in push or buf[j + 1] != 0x68:
            continue
        cid = struct.unpack_from("<I", buf, j + 2)[0]
        if buf[j + 6] not in push or buf[j + 7] != 0xE8:
            continue
        rel = struct.unpack_from("<i", buf, j + 8)[0]
        if base + j + 12 + rel == helper:
            out[slot] = cid
    return out


def main():
    exe = sys.argv[1]
    block = (0x459E7F, 0x45A3C0)
    src, dialog = 0x8C, 151
    a = sys.argv[2:]
    for i, tok in enumerate(a):
        if tok == "--block":
            lo, hi = a[i + 1].split(":"); block = (int(lo, 0), int(hi, 0))
        elif tok == "--src":
            src = int(a[i + 1], 0)
        elif tok == "--dialog":
            dialog = int(a[i + 1], 0)
    data = open(exe, "rb").read()
    pe, secs = sections(data)
    text = [s for s in secs if s[0] == ".text"][0]
    _, tva, tvs, tra, trs = text
    buf = data[tra:tra + trs]
    base = 0x400000 + tva
    bit2slot = bit_unpackers(buf, base, src)
    slot2id = slot_checkboxes(buf, base, block[0], block[1])
    lab = {}
    dlg = dialogs(data, pe, secs).get(dialog)
    if dlg:
        title, rows, ex, font = parse(dlg)
        lab = {cid: tx for cid, cl, tx, st, x, y, cx, cy in rows}
        print(f"# dialog {dialog} {title!r}: {len(rows)} controls")
    print(f"# PRTO-style flag dword at row+{src:#x}; page setter {block[0]:#x}..{block[1]:#x}")
    print("bit  slot   control  label")
    for slot, bit in sorted(bit2slot.items(), key=lambda kv: kv[1]):
        cid = slot2id.get(slot)
        print(f"{bit:3d}  {slot:#06x} {str(cid):>8}  {lab.get(cid, '?')!r}")
    rest = {s: c for s, c in slot2id.items() if s not in bit2slot}
    if rest:
        print("\n# same page, slots NOT filled from this dword (another source):")
        print("slot   control  label")
        for slot, cid in sorted(rest.items()):
            print(f"{slot:#06x} {cid:>8}  {lab.get(cid, '?')!r}")


if __name__ == "__main__":
    main()
