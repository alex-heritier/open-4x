"""Run the real corruption routine 0x4B1190 over a loaded save, varying the state.

Usage: CIV3_DEFAULT_BIQ=conq.raw python corruption.py save.raw out.json [scenarios]
Every city is handed to one player; each scenario sets its government (and
that government's corruption class), Courthouses (by hooking 0x4ACB50),
disorder/celebration flags and a Reduces-Corruption small wonder city. Each
record holds the inputs read through the binary's own helpers and the result.
Used for economy.md (corruption) and rust/src/economy.rs golden vectors.
"""
import sys, json, struct
sys.path.insert(0, __import__('os').path.dirname(__file__))
from emu import *
from loadsave import setup, BUF_BASE
e = setup(sys.argv[1])
e.call(0x590030, args=(BUF_BASE + e.hdr, 0), timeout_us=300_000_000)
def s32(a):
    v = e.u32(a); return v - (1 << 32) if v & 0x80000000 else v
def s16(a):
    v = struct.unpack('<h', e.rd(a, 2))[0]; return v
PL = 0xA52E98; PS = 0x20E4
def player(p): return PL + p * PS
base = e.u32(0xA52E6C); last = s32(0xA52E78)
def city(i):
    if base == 0 or i < 0 or i > last: return 0
    n = e.u32(base + i * 8 + 4)
    return n - 0x1C if n else 0
BL = e.u32(0x9C40AC); NB = s32(0x9C3D80)
GV = e.u32(0x9C71D8); W = s32(0x9C74D4); H = s32(0x9C74C0)
B = s32(e.u32(0x9C7330) + s32(0x9C73A0) * 84 + 4)
DF = e.u32(0x9C40C0); CZ = e.u32(0x9C40B0)
print('W', W, 'H', H, 'B', B, 'cities', last + 1, file=sys.stderr)

# --- mutate: every city to the owner of city 0
cities = [city(i) for i in range(last + 1)]
cities = [c for c in cities if c]
P0 = e.rd(cities[0] + 0x28, 1)[0]
for c in cities: e.wr(c + 0x28, bytes([P0]))
e.w32(player(P0) + 0x194, len(cities))
_sw = next(b for b in range(NB) if e.u32(BL+b*0x110+0xF0) & 8 and e.u32(BL+b*0x110+0xD4) == 0xFFFFFFFF and not e.u32(BL+b*0x110+0xF4) & 0x400)
e.w32(BL+_sw*0x110+0xF4, e.u32(BL+_sw*0x110+0xF4) | 0x20)
FP = next(b for b in range(NB) if e.u32(BL+b*0x110+0xF4) & 0x20 and e.u32(BL+b*0x110+0xF0) & 8 and (e.u32(BL+b*0x110+0xD4) == 0xFFFFFFFF))
CH = next(b for b in range(NB) if e.u32(BL+b*0x110+0xEC) & 0x100)
print('FP', FP, 'CH', CH, file=sys.stderr)
court_ids = set()
orig = 0x4ACB50
def has(em, b, flag):
    c = em.reg('ecx')
    return 1 if (b == CH and c in court_ids) else 0
probe = {}
def _p(uc, a, sz, ud):
    sp = uc.reg_read(UC_X86_REG_ESP)
    rd = lambda o: struct.unpack('<i', uc.mem_read(sp+o, 4))[0]
    probe['rank'] = rd(0x18); probe['O'] = rd(0x20); probe['share'] = rd(0x28); probe['C'] = rd(0x14)
e.uc.hook_add(UC_HOOK_CODE, _p, begin=0x4B18D1, end=0x4B18D1)
def _q(uc, a, sz, ud):
    probe['d'] = uc.reg_read(UC_X86_REG_EBX); probe['e0'] = uc.reg_read(UC_X86_REG_ESI)
e.uc.hook_add(UC_HOOK_CODE, _q, begin=0x4B14F0, end=0x4B14F0)
out = []
e.hook_func(0x4ACB50, has, nargs=2, callee_pops=True)
e.uc.ctl_remove_cache(0x400000, 0x700000)

for scen in range(int(sys.argv[3]) if len(sys.argv) > 3 else 8):
    govt = scen % 8
    e.w32(player(P0) + 0xA0, govt)
    e.w32(GV + govt * 0x1E8 + 0x18C, (scen * 5) % 6)
    court_ids = set(c for k, c in enumerate(cities) if (k + scen) % 3 == 0)
    for k, c in enumerate(cities):
        f = e.u32(c + 0x30) & ~3
        if (k + scen) % 5 == 1: f |= 1
        if (k + scen) % 7 == 2: f |= 2
        e.w32(c + 0x30, f)
    tab = e.u32(player(P0) + 0x15E8)
    e.w32(tab + FP * 4, s32(cities[(scen * 5) % len(cities)] + 0x20) if scen % 2 else 0xFFFFFFFF)
    words = set()
    for c in cities:
        owner = e.rd(c + 0x28, 1)[0]
        P = player(owner)
        cap_id = s32(P + 0x2C)
        cap = city(cap_id) if cap_id >= 0 else 0
        govt = s32(P + 0xA0)
        klass = s32(GV + govt * 0x1E8 + 0x18C)
        flags = e.u32(c + 0x30)
        court = 0
        for b in range(NB):
            if (b == CH and c in court_ids) and not (e.call(0x4ACCC0, args=(b,), ecx=c) & 0xFF) \
               and e.u32(BL + b * 0x110 + 0xEC) & 0x100:
                court += 1
        x, y = s16(c + 0x24), s16(c + 0x26)
        capd = 2**31 - 1; conn = False
        if cap:
            capd = e.call(0x4378D0, args=(s16(cap + 0x24), s16(cap + 0x26), x, y), ecx=0x9C736C)
            capd = capd - (1 << 32) if capd & 0x80000000 else capd
            conn = bool(e.call(0x57F0A0, args=(c, cap, 0xFFFFFFFF), ecx=0xB72888) & 0xFF)
        # palace-like wonders
        pd = 2**31 - 1; here = 0
        for b in range(NB):
            row = BL + b * 0x110
            rq = s32(row + 0xD4)
            if not (rq == govt or rq == -1): continue
            if not e.u32(row + 0xF4) & 0x20: continue
            oth = e.u32(row + 0xF0)
            wc = 0
            if oth & 8:
                wc = city(s32(e.u32(P + 0x15E8) + b * 4))
            elif oth & 4:
                wi = e.call(0x539030, args=(b,), ecx=0xA52658); wc = city(wi - (1<<32) if wi & 0x80000000 else wi)
                if wc and e.rd(wc + 0x28, 1)[0] != owner: wc = 0
            if wc:
                if wc == c: here += 1
                d = e.call(0x4378D0, args=(s16(wc + 0x24), s16(wc + 0x26), x, y), ecx=0x9C736C)
                pd = min(pd, d - (1<<32) if d & 0x80000000 else d)
        ocn = e.call(0x5676C0, ecx=P); ocn = ocn - (1<<32) if ocn & 0x80000000 else ocn
        # rank: replicate with tie words recorded
        rank = 0
        if klass == 5:
            rank = s32(P + 0x194) // 2 if s32(P + 0x194) >= 0 else -((-s32(P + 0x194)) // 2)
        elif cap:
            cx, cy = s16(cap + 0x24), s16(cap + 0x26)
            for j in range(last + 1):
                o = city(j)
                if not o or o == c or e.rd(o + 0x28, 1)[0] != owner: continue
                words.add(tuple(s32(o + k) for k in (0x358, 0x35C, 0x360, 0x364)))
                dd = e.call(0x4378D0, args=(cx, cy, s16(o + 0x24), s16(o + 0x26)), ecx=0x9C736C)
                dd = dd - (1<<32) if dd & 0x80000000 else dd
                if dd < capd: rank += 1
                elif dd == capd and s32(o + 0x20) < s32(c + 0x20): rank += 1
        spec = 0
        cnt = s32(c + 0xEC); arr = e.u32(c + 0xE0)
        for k in range(cnt + 1):
            if not arr: break
            n = e.u32(arr + k * 8 + 4)
            if not n: continue
            z = n - 0x1C
            if e.rd(z + 0x20, 1)[0] == 0:
                spec += s32(CZ + s32(z + 0x13C) * 0x80 + 0x78)
        diff = s32(DF + s32(P + 0x30) * 0x7C + 0x74)
        for gross in (1, 7, 23, 60):
            for waste in (0, 1):
                probe.clear()
                r = e.call(0x4B1190, args=(gross, waste), ecx=c)
                r = r - (1 << 32) if r & 0x80000000 else r
                out.append(dict(gross=gross, waste=bool(waste), disorder=bool(flags & 1), celebrating=bool(flags & 2),
                    has_capital=bool(cap), klass=klass, courthouses=court, is_capital=(c == cap), palaces_here=here,
                    ocn=ocn, world_base=B, capital_distance=capd, palace_distance=pd, connected=conn, width=W, height=H,
                    rank=rank, specialists=spec, difficulty_percent=diff, expect=r, probe=dict(probe)))
print('tie words seen', words, file=sys.stderr)
json.dump(out, open(sys.argv[2], 'w'))
print('records', len(out), file=sys.stderr)
