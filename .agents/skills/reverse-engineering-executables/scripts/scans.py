#!/usr/bin/env python3
"""Byte-pattern scans over a PE32 executable (stdlib only, no capstone/pefile).

Promotes the throwaway /tmp/re_probeN.py patterns into one CLI. All scans are
raw byte matches over .text, so an opcode byte can occasionally match inside a
longer instruction: confirm any cited site in r2 (`../r2q.sh 'pd N @ VA'`)
before writing it into a findings file.

Subcommands:
  calls VA     E8 rel32 caller census (exact direct-caller count)
  pushes SPEC  68 imm32 references (VA as 0x..., or 4-char tag like GOOD)
  tags         3D cmp-eax inventory grouped by FOURCC-looking immediate
  slots        FF /2,/4 indirect-call slot census (vtable usage per slot)
  buckets      68 string-ref clustering per 64KB text bucket (atlas labels)

Examples:
  scans.py calls 0x64A20E
  scans.py pushes GOOD
  scans.py tags --min 3
  scans.py slots
  scans.py buckets
"""
import argparse
import hashlib
import os
import re
import struct
import sys
from collections import Counter, defaultdict

SPEC = os.path.dirname(os.path.abspath(__file__))
CANDIDATES = (
    "civ3-gog/app/Conquests/Civ3Conquests.exe",
    "re/Civ3Conquests.exe",
)


def resolve_exe(flag):
    if flag:
        return flag
    roots = []
    d = os.getcwd()
    for _ in range(6):
        roots.append(d)
        d = os.path.dirname(d)
    # skill-anchored workspace root: scripts/RE/skills/.agents/civ3-clone/<root>
    roots.append(os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.dirname(SPEC))))))
    for root in roots:
        for cand in CANDIDATES:
            p = os.path.join(root, cand)
            if os.path.isfile(p):
                return p
    sys.exit("no exe found; pass --exe PATH")


class Image:
    def __init__(self, path):
        self.path = path
        self.data = open(path, "rb").read()
        pe = struct.unpack("<I", self.data[0x3C:0x40])[0]
        nsec = struct.unpack("<H", self.data[pe + 6:pe + 8])[0]
        optsize = struct.unpack("<H", self.data[pe + 20:pe + 22])[0]
        magic = struct.unpack("<H", self.data[pe + 24:pe + 26])[0]
        if magic != 0x10B:
            sys.exit(f"not PE32 (optional magic {magic:#x})")
        self.base = struct.unpack("<I", self.data[pe + 24 + 28:pe + 24 + 32])[0]
        self.secs = {}
        for s in range(nsec):
            off = pe + 24 + optsize + s * 40
            name = self.data[off:off + 8].rstrip(b"\x00")
            vsize, vaddr, rawsz, rawptr = struct.unpack("<IIII",
                                                       self.data[off + 8:off + 24])
            self.secs[name] = (vaddr, rawsz, rawptr)
        vaddr, rawsz, rawptr = self.secs[b".text"]
        self.tbase = self.base + vaddr
        self.text = self.data[rawptr:rawptr + rawsz]
        self.digest = hashlib.sha256(self.data).hexdigest()[:16]

    def va2raw(self, va):
        rva = va - self.base
        for vaddr, rawsz, rawptr in self.secs.values():
            if vaddr <= rva < vaddr + rawsz:
                return rva - vaddr + rawptr
        return None

    def cstr(self, va, n=48):
        raw = self.va2raw(va)
        if raw is None:
            return None
        b = self.data[raw:raw + n].split(b"\x00")[0]
        if len(b) < 4 or not all(32 <= c < 127 or c in (9, 10, 13) for c in b):
            return None
        return b.decode()

    def data_range(self):
        lo, hi = None, 0
        for name in (b".rdata", b".data"):
            if name in self.secs:
                vaddr, rawsz, _ = self.secs[name]
                va = self.base + vaddr
                lo = va if lo is None else min(lo, va)
                hi = max(hi, va + rawsz)
        return lo, hi


def parse_va(s):
    try:
        return int(s, 0)
    except ValueError:
        return int(s, 16)


def parse_push_spec(s):
    if len(s) == 4 and all(32 <= ord(c) < 127 for c in s) and not s.startswith("0x"):
        return struct.unpack("<I", s.encode("latin1"))[0], s
    v = parse_va(s)
    return v, f"{v:#x}"


def show_sites(sites, limit=60, force_all=False):
    if not force_all and len(sites) > limit:
        print("  " + " ".join(f"{a:#x}" for a in sites[:limit]))
        print(f"  ... ({len(sites) - limit} more, --all to list)")
    else:
        for a in sites:
            print(f"  {a:#x}")


def cmd_calls(img, args):
    target = parse_va(args.va)
    op = b"\xE9" if args.jmp else b"\xE8"
    kind = "jmps" if args.jmp else "calls"
    sites = []
    i, n = 0, len(img.text)
    while True:
        i = img.text.find(op, i)
        if i < 0 or i + 5 > n:
            break
        rel = struct.unpack_from("<i", img.text, i + 1)[0]
        if img.tbase + i + 5 + rel == target:
            sites.append(img.tbase + i)
        i += 1
    print(f"{kind} to {target:#x}: {len(sites)}")
    show_sites(sites, force_all=args.all)


def cmd_pushes(img, args):
    imm, label = parse_push_spec(args.spec)
    sites = []
    for m in re.finditer(b"\x68....", img.text):
        if struct.unpack("<I", m.group(0)[1:])[0] == imm:
            sites.append(img.tbase + m.start())
    print(f"pushes of {label} ({imm:#x}): {len(sites)}")
    show_sites(sites, force_all=args.all)


def printable4(imm):
    b = struct.pack("<I", imm)
    if all(32 <= c < 127 for c in b):
        return True, b.decode("ascii")
    return False, ""


def cmd_tags(img, args):
    hits = defaultdict(list)
    for m in re.finditer(b"\x3D....", img.text):
        imm = struct.unpack("<I", m.group(0)[1:])[0]
        ok, tag = printable4(imm)
        if ok:
            hits[tag].append(img.tbase + m.start())
    rows = sorted(hits.items(), key=lambda kv: (-len(kv[1]), kv[0]))
    rows = [(t, s) for t, s in rows if len(s) >= args.min]
    print(f"3D cmp tags: {len(rows)} distinct (min {args.min})")
    for tag, sites in rows:
        first = " ".join(f"{a:#x}" for a in sites[:3])
        print(f"  {tag}: {len(sites)}  {first}")


def cmd_slots(img, args):
    # FF /2 = call r/m32, FF /4 = jmp r/m32; mod 01 (disp8) / 10 (disp32).
    calls, jmps = Counter(), Counter()
    call_sites = defaultdict(list)
    t = img.text
    i, n = 0, len(t)
    while i < n - 2:
        if t[i] != 0xFF:
            i += 1
            continue
        modrm = t[i + 1]
        mod, reg, rm = modrm >> 6, (modrm >> 3) & 7, modrm & 7
        if reg not in (2, 4) or mod not in (1, 2):
            i += 1
            continue
        j = i + 2 + (1 if rm == 4 else 0)  # skip SIB byte when present
        need = j + (1 if mod == 1 else 4)
        if need > n:
            break
        slot = t[j] if mod == 1 else struct.unpack_from("<I", t, j)[0]
        if slot > 0x400:  # not a vtable slot, some other displacement
            i += 1
            continue
        if reg == 2:
            calls[slot] += 1
            call_sites[slot].append(img.tbase + i)
        else:
            jmps[slot] += 1
        i += 1
    print(f"indirect slots: {len(set(calls) | set(jmps))} distinct "
          f"(slot numbers collide across classes)")
    for slot in sorted(set(calls) | set(jmps),
                       key=lambda s: -(calls[s] + jmps[s])):
        first = " ".join(f"{a:#x}" for a in call_sites[slot][:3])
        print(f"  {slot:#06x}: {calls[slot]} calls {jmps[slot]} jmps  {first}")


def cmd_buckets(img, args):
    lo, hi = img.data_range()
    buckets = defaultdict(list)
    for m in re.finditer(b"\x68....", img.text):
        imm = struct.unpack("<I", m.group(0)[1:])[0]
        if not (lo <= imm < hi):
            continue
        s = img.cstr(imm)
        if s:
            buckets[(img.tbase + m.start()) >> 16].append(s)
    for b in sorted(buckets):
        strs = buckets[b]
        samp = sorted(set(strs), key=lambda s: (-len(s), s))[:args.samples]
        print(f"--- text bucket {b:#06x}0000 ({len(strs)} string refs)")
        for s in samp:
            print(f"    {s[:90]}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--exe", help="PE32 path (default: find Civ3Conquests.exe)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("calls", help="E8/E9 rel32 sites resolving to VA")
    p.add_argument("va")
    p.add_argument("--jmp", action="store_true", help="scan E9 instead of E8")
    p.add_argument("--all", action="store_true")
    p = sub.add_parser("pushes", help="68 imm32 sites for a VA or 4-char tag")
    p.add_argument("spec")
    p.add_argument("--all", action="store_true")
    p = sub.add_parser("tags", help="3D cmp immediates grouped by FOURCC text")
    p.add_argument("--min", type=int, default=1)
    sub.add_parser("slots", help="FF indirect-call slot census over .text")
    p = sub.add_parser("buckets", help="string-ref clustering per 64KB bucket")
    p.add_argument("--samples", type=int, default=6)
    args = ap.parse_args()

    img = Image(resolve_exe(args.exe))
    print(f"# {img.path} sha256:{img.digest} base:{img.base:#x} "
          f".text:{img.tbase:#x}+{len(img.text):#x}", file=sys.stderr)
    {"calls": cmd_calls, "pushes": cmd_pushes, "tags": cmd_tags,
     "slots": cmd_slots, "buckets": cmd_buckets}[args.cmd](img, args)


if __name__ == "__main__":
    main()
