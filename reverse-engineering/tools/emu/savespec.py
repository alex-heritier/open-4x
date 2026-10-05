"""Python reference spec of the CIV3 save stream (format 24, sub-versions 2..10).

Mirrors the exe's loaders chunk for chunk and produces the list of segments
(offset, kind, name, size). `biq/src/sav/` is the Rust twin; this one is what
was checked against the emulator traces (`trace.py`). See
`reverse-engineering/savegame.md`, "How the grammar was verified".
"""
import struct, sys, pickle, glob, os

U32 = struct.Struct('<I')


def u32(b, o):
    return U32.unpack_from(b, o)[0]


class Fail(Exception):
    pass


class Dec:
    def __init__(self, s, ver, sub, counts):
        self.s = s
        self.p = 0
        self.ver = ver
        self.sub = sub
        self.c = counts
        self.seg = []  # (off, kind, name, size)

    # primitives -------------------------------------------------
    def chunk(self, tag, size=None):
        s, p = self.s, self.p
        if p + 8 > len(s):
            raise Fail('eof chunk %s at %#x' % (tag, p))
        t = s[p:p + 4]
        n = u32(s, p + 4)
        if t != tag.encode():
            raise Fail('tag mismatch at %#x: want %s got %r' % (p, tag, t))
        if size is not None and n != size:
            raise Fail('size mismatch at %#x %s: want %d got %d' % (p, tag, size, n))
        self.seg.append((p, 'C', tag, n))
        self.p = p + 8 + n
        return s[p + 8:p + 8 + n]

    def raw(self, n, name=''):
        if n:
            self.seg.append((self.p, 'R', name, n))
        if self.p + n > len(self.s):
            raise Fail('eof raw %s at %#x +%d' % (name, self.p, n))
        b = self.s[self.p:self.p + n]
        self.p += n
        return b

    def dw(self, name=''):
        return u32(self.raw(4, name), 0)

    # classes ----------------------------------------------------
    def run(self):
        s = self.s
        c = self.c
        # scenario
        bic = self.chunk('BIC ', 524)
        biqlen = u32(bic, 0)
        self.raw(biqlen, 'biq')
        # GAME
        g = self.chunk('GAME', 848)
        self.game_body = g
        self.game_arrays(g)
        self.chunk('CNSL', 228)
        if self.sub >= 9:
            self.map()
        self.players()
        if self.sub < 9:
            self.map()
        self.units(u32(g, 0x18))
        self.cities(u32(g, 0x1C))
        self.colonies(u32(g, 0x20))
        self.raw(256, 'g256')
        if self.sub < 4:
            self.raw(8, 'sub4skip')
        for i in range(32):
            self.chunk('PALV', 148)
        self.hist(g)
        if self.ver > 3:
            self.chunk('TUTR', 92)
        if self.ver >= 10:
            self.chunk('FAXX', 88)
        if self.ver >= 12:
            self.replay()
        if self.sub >= 1:
            self.fnet()
            self.chunk('PEER', 24)
        for off, tag, size in ((0x12C, 'AIBS', 20), (0x130, 'VLOC', 16), (0x134, 'RADT', 16), (0x138, 'OUTP', 16)):
            for i in range(u32(g, off)):
                self.chunk(tag, size)
        return self.p

    def game_arrays(self, g):
        c = self.c
        A = u32(g, 0x124)
        if A:
            self.raw(A * 4, 'game.A')
        if c['tech']:
            self.raw(c['tech'] * 4, 'game.tech4')
        if c['bldg']:
            self.raw(c['bldg'] * 4, 'game.bldg4a')
            self.raw(c['bldg'], 'game.bldg1')
            self.raw(c['bldg'] * 4, 'game.bldg4b')
            self.raw(c['bldg'] * 4, 'game.bldg4c')
        if c['prto']:
            self.raw(c['prto'] * 4, 'game.prto4a')
            self.raw(c['prto'] * 4, 'game.prto4b')
        if c['tech']:
            self.raw(c['tech'] * 4, 'game.tech4b')
        gv = u32(g, 0x320)
        if gv == 1:
            raise Fail('game block v1 unsupported')
        if gv >= 2:
            self.chunk('DATE', 84)
        if gv >= 3:
            self.chunk('PLGI', 4)
            self.chunk('PLGI', 8)
            self.chunk('DATE', 84)
            self.chunk('DATE', 84)
        if gv >= 4:
            self.raw(4, 'game.v4')
        if gv >= 5:
            self.raw(4, 'game.v5')

    def map(self):
        c = self.c
        w2 = self.chunk('WRLD', 2)
        w164 = self.chunk('WRLD', 164)
        if self.ver >= 13:
            self.chunk('WRLD', 52)
        ncont = struct.unpack_from('<H', w2, 0)[0]
        if ncont == 0:
            raise Fail('zero continents: fallback count unsupported')
        height = u32(w164, 4)
        width = u32(w164, 0x18)
        ntile = (width // 2) * height
        for i in range(ntile):
            t36 = self.chunk('TILE', 36)
            self.chunk('TILE', 12)
            self.chunk('TILE', 4)
            if self.sub < 8 and struct.unpack_from('<b', t36, 0x1C)[0] >= 6:
                self.raw(12, 'tile.legacy12')
            self.chunk('TILE', 128)
        for i in range(ncont):
            self.chunk('CONT', 8)
        self.raw(c['good'] * 4, 'map.good')
        self.map_dims = (width, height, ntile, ncont)

    def players(self):
        for i in range(32):
            self.player(i)

    def player(self, idx):
        c = self.c
        lead = self.chunk('LEAD', 5532)
        for k in range(32):
            n = self.dw('pl.list0')
            self.raw(n * 12, 'pl.list0.items')
        active = lead[0x11b4 - 0x1c]
        pv = u32(lead, 0x15b0 - 0x1c)
        if active:
            B = c['bldg']
            if B:
                self.raw(B * 2, 'pl.bldg2a')
                self.raw(B * 2, 'pl.bldg2b')
                self.raw(B * 2, 'pl.bldg2c')
                self.raw(B * 4, 'pl.bldg4')
                self.raw(B, 'pl.bldg1')
            P = c['prto']
            if P:
                self.raw(P * 2, 'pl.prto2a')
                self.raw(P * 2, 'pl.prto2b')
                self.raw(P * 2, 'pl.prto2c')
            if c['space']:
                self.raw(c['space'] * 2, 'pl.space2')
            G = c['good']
            if G:
                self.raw(G * 96, 'pl.good96')
                self.raw(G, 'pl.good1')
        n19c = u32(lead, 0x180)
        if n19c:
            for k in range(4):
                self.raw(n19c * 4, 'pl.arr%d' % k)
            self.raw(n19c * 4, 'pl.arr4')
        self.chunk('CULT', 16)
        self.chunk('ESPN', 32)
        self.chunk('ESPN', 32)
        n = self.dw('pl.ringn')
        self.raw(n * 4, 'pl.ring')
        if self.sub > 2:
            for k in range(32):
                n = self.dw('pl.listA')
                self.raw(n * 12, 'pl.listA.items')
        if self.sub >= 6:
            for k in range(32):
                n = self.dw('pl.listB')
                self.raw(n * 12, 'pl.listB.items')
        tail = 0
        if pv >= 1:
            tail += 4
        if pv >= 2:
            tail += 2
        if pv >= 3:
            tail += 2
        if pv >= 4:
            tail += 1
        self.raw(tail * 4, 'pl.tail')

    def units(self, n):
        for i in range(n):
            u = self.chunk('UNIT', 472)
            ver = u32(u, 464)
            if ver >= 2:
                idls = self.chunk('IDLS', 8)
                cnt = u32(idls, 4)
                self.raw(cnt * 4, 'unit.idls')

    def cities(self, n):
        for i in range(n):
            self.city()

    def city(self):
        self.chunk('CITY', 136)
        self.chunk('CITY', 16)
        self.chunk('CITY', 36)
        self.chunk('CITY', 164)
        self.chunk('CITY', 148)
        popd = self.chunk('POPD', 8)
        nct = u32(popd, 4)
        for i in range(nct):
            self.chunk('CTZN', 300)
        self.chunk('BINF', 4)
        self.raw(self.c['bldg'] * 12, 'city.bldg12')
        self.chunk('BITM', 40)
        if self.sub >= 4:
            self.chunk('DATE', 84)
        ver = None
        if self.ver >= 20:
            cv = self.chunk('CITY', 8)
            ver = u32(cv, 4)
        else:
            ver = 0
        if ver >= 2:
            cn = self.chunk('CITY', 4)
            n = u32(cn, 0)
            for i in range(n):
                self.chunk('CITY', 4)
        if ver >= 3:
            self.chunk('CTPG', 4)
            self.chunk('CTPG', 16)
        if ver >= 4:
            self.chunk('CITY', 4)

    def colonies(self, n):
        for i in range(n):
            self.chunk('CLNY', 16)

    def hist(self, g):
        self.raw(4, 'hist.tag')
        n = self.dw('hist.n')
        self.raw(4, 'hist.x')
        flags = u32(g, 0x08)
        for i in range(n):
            a = self.dw('hist.a')
            b = self.dw('hist.b')
            m = self.dw('hist.m')
            narr = 4 + (1 if flags & 0x26000 else 0)
            for k in range(narr):
                self.raw(m * 4, 'hist.arr')

    def replay(self):
        self.raw(4, 'rpl.tag')
        n = self.dw('rpl.n')
        for i in range(n):
            self.chunk('RPLT', 5)
            k = self.dw('rpl.k')
            for j in range(k):
                self.chunk('RPLE', 10)
                b = self.s[self.p]
                if b == 0:
                    self.raw(1, 'rple.s0')
                else:
                    e = self.s.index(b'\0', self.p)
                    self.raw(e - self.p + 1, 'rple.str')

    def fnet(self):
        n = self.dw('fnet.n')
        for i in range(n):
            l = self.dw('fnet.len')
            self.raw(l, 'fnet.data')


def counts_from_biq(biq, defaults):
    """Rule counts from an embedded BIQ stream (None -> default)."""
    out = dict(defaults)
    p = 4
    secs = {}
    while p + 8 <= len(biq):
        tag = biq[p:p + 4]
        cnt = u32(biq, p + 4)
        p += 8
        rows = []
        if tag == b'FLAV':
            n = u32(biq, p); p += 4
            for i in range(n):
                nrel = u32(biq, p + 4 + 256)
                p += 4 + 256 + 4 + nrel * 4
            secs[tag] = (cnt, [])
            continue
        for i in range(cnt):
            l = u32(biq, p)
            rows.append((p + 4, l))
            p += 4 + l
        secs[tag] = (cnt, rows)
    for tag, key in ((b'BLDG', 'bldg'), (b'PRTO', 'prto'), (b'TECH', 'tech'), (b'GOOD', 'good')):
        if tag in secs and secs[tag][0] > 0:
            out[key] = secs[tag][0]
    if b'RULE' in secs and secs[b'RULE'][0] > 0:
        off, l = secs[b'RULE'][1][0]
        out['space'] = u32(biq, off + 0x60)
    return out


DEFAULT = dict(bldg=83, prto=141, tech=83, good=26, space=10)


def decode_file(path):
    d = open(path, 'rb').read()
    ver, sub = struct.unpack_from('<II', d, 6)
    hdr = 6 + 8 + (16 if sub >= 7 else 0)
    s = d[hdr:]
    # embedded BIQ
    biqlen = u32(s, 8)
    biq = s[8 + 524:8 + 524 + biqlen]
    counts = counts_from_biq(biq, DEFAULT)
    dec = Dec(s, ver, sub, counts)
    end = dec.run()
    return dec, end, len(s), counts


def check(path, trace):
    dec, end, n, counts = decode_file(path)
    tr = pickle.load(open(trace, 'rb'))
    hdr = tr['hdr']
    tchunks = [(e[2] - hdr, e[3].decode(), e[4]) for e in tr['ev'] if e[0] == 'C']
    dchunks = [(o, t, n_) for (o, k, t, n_) in dec.seg if k == 'C']
    ok = tchunks == dchunks
    if not ok:
        for i, (a, b) in enumerate(zip(tchunks, dchunks)):
            if a != b:
                print('  first diff at chunk #%d: trace=%s dec=%s' % (i, a, b))
                break
        print('  trace chunks', len(tchunks), 'dec chunks', len(dchunks))
    return ok, end, n, counts


if __name__ == '__main__':
    # usage: savespec.py STREAM.raw [TRACE.pkl] ...   (pairs; trace optional)
    # Decodes each decoded save stream with the spec above, then, when a trace
    # from trace.py is given, requires the exe's chunk list to be identical.
    import traceback
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    i = 0
    while i < len(args):
        f = args[i]
        tr = None
        if i + 1 < len(args) and args[i + 1].endswith('.pkl'):
            tr = args[i + 1]
            i += 1
        i += 1
        name = os.path.basename(f)
        try:
            if tr:
                ok, end, n, counts = check(f, tr)
            else:
                dec, end, n, counts = decode_file(f)
                ok = None
            print(name[:40].ljust(42), 'chunks', 'OK' if ok else ('?' if ok is None else 'DIFF'),
                  'end %#x of %#x' % (end, n), 'EOF OK' if end == n else 'EOF MISMATCH', counts)
        except Fail as e:
            print(name[:40].ljust(42), 'FAIL', e)
        except Exception:
            print(name[:40].ljust(42), 'ERR')
            traceback.print_exc()
