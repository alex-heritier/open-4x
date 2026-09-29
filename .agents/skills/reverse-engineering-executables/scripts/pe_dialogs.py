#!/usr/bin/env python3
"""Dump every dialog resource of a PE (RC/DLGTEMPLATE or DLGTEMPLATEEX).

The Civ3 scenario editor (`Civ3ConquestsEdit.exe`) ships its property pages as
dialog resources, so this names the BIQ rule fields that the game's own code
shows only as offsets (e.g. "Movement Rate Along Roads", "Appearance Ratio",
"Zone of Control"). See `reverse-engineering/editor.md`.

Usage:
  python3 pe_dialogs.py <exe> [substring ...]

With no substring, prints all dialogs. Each item is printed as
`id <n> <KIND> (<x>,<y>,<cx>,<cy>) '<text>'` — the control id is what the
editor's handler code pushes, so it is the key back to the code path.
"""
import struct
import sys


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


def load(exe):
    data = open(exe, "rb").read()
    pe, secs = sections(data)

    def rva2off(rva):
        for _, va, vs, ra, rs in secs:
            if va <= rva < va + max(vs, rs):
                return ra + (rva - va)
        return None

    rsrc_rva, _ = struct.unpack_from("<II", data, pe + 24 + 96 + 2 * 8)
    base = rva2off(rsrc_rva)

    def entries(off):
        nn, nid = struct.unpack_from("<HH", data, base + off + 12)
        out = []
        for i in range(nn + nid):
            e = base + off + 16 + 8 * i
            nm, sub = struct.unpack_from("<II", data, e)
            out.append((nm, sub))
        return out

    dialogs = {}
    for nm, sub in entries(0):
        if nm != 5:
            continue
        for nm2, sub2 in entries(sub & 0x7FFFFFFF):
            for nm3, sub3 in entries(sub2 & 0x7FFFFFFF):
                if sub3 & 0x80000000:
                    continue
                drva, dsz = struct.unpack_from("<II", data, base + sub3)
                off = rva2off(drva)
                dialogs[nm2] = data[off:off + dsz]
    return dialogs


def u16(buf, o):
    w = struct.unpack_from("<H", buf, o)[0]
    if w == 0:
        return "", o + 2
    if w == 0xFFFF:
        return "#%d" % struct.unpack_from("<H", buf, o + 2)[0], o + 4
    e = o
    while buf[e:e + 2] != b"\0\0":
        e += 2
    return buf[o:e].decode("utf-16-le"), e + 2


def parse(buf):
    ex = struct.unpack_from("<H", buf, 0)[0] == 1
    if ex:
        exs, style = struct.unpack_from("<II", buf, 8)
        o = 16
        n = struct.unpack_from("<H", buf, o)[0]
        o += 2
        x, y, cx, cy = struct.unpack_from("<hhhh", buf, o)
        o += 8
        # DLGTEMPLATEEX: a menu is a DWORD ordinal, not a WORD string.
        w = struct.unpack_from("<H", buf, o)[0]
        if w == 0:
            menu, o = "", o + 2
        elif w == 0xFFFF:
            menu, o = "#%d" % struct.unpack_from("<H", buf, o + 2)[0], o + 4
        else:
            menu, o = "#%d" % struct.unpack_from("<I", buf, o)[0], o + 4
    else:
        style, exs = struct.unpack_from("<II", buf, 0)
        o = 8
        n = struct.unpack_from("<H", buf, o)[0]
        o += 2
        x, y, cx, cy = struct.unpack_from("<hhhh", buf, o)
        o += 8
        menu, o = u16(buf, o)
    cls, o = u16(buf, o)
    title, o = u16(buf, o)
    font = None
    if (style & 0x40) and not ex:  # DS_SETFONT
        sz = struct.unpack_from("<H", buf, o)[0]
        o += 2
        face, o = u16(buf, o)
        font = (sz, face)
    rows = []
    for _ in range(n):
        o = (o + 3) & ~3
        st, exst = struct.unpack_from("<II", buf, o)
        ix, iy, icx, icy = struct.unpack_from("<hhhh", buf, o + 8)
        if ex:
            cid = struct.unpack_from("<I", buf, o + 16)[0]
            o += 20
        else:
            cid = struct.unpack_from("<H", buf, o + 16)[0]
            o += 18
        cl, o = u16(buf, o)
        tx, o = u16(buf, o)
        exsz = struct.unpack_from("<H", buf, o)[0]
        o += 2 + exsz
        rows.append((cid, cl, tx, st, ix, iy, icx, icy))
    return title, rows, ex, font


KINDS = {0x80: "BUTTON", 0x81: "EDIT", 0x82: "STATIC", 0x83: "LISTBOX",
         0x84: "SCROLLBAR", 0x85: "COMBOBOX"}


def main():
    exe = sys.argv[1]
    needles = [n.lower() for n in sys.argv[2:]]
    for did in sorted(load(exe).items(), key=lambda kv: kv[0]):
        try:
            title, rows, ex, font = parse(did[1])
        except Exception as e:  # a template we cannot walk; report, keep going
            print(f"=== dialog {did[0]} PARSE-FAIL {e}")
            continue
        blob = (title + " " + " ".join(t for _, _, t, _, _, _, _, _ in rows)).lower()
        if needles and not any(n in blob for n in needles):
            continue
        print(f"=== dialog {did[0]} {title!r} items={len(rows)} ex={ex} font={font}")
        for cid, cl, tx, st, ix, iy, icx, icy in rows:
            kind = KINDS.get(st & 0xFF, hex(st & 0xFF))
            print(f"   id {cid:5d} {kind:8s} ({ix:4d},{iy:4d},{icx:4d},{icy:4d}) {tx!r}")


if __name__ == "__main__":
    main()
