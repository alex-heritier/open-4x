#!/usr/bin/env python3
"""The map-generator oracle: the exe's own `Map::generate`, run under Unicorn.

Loads a saved game with the game's own loader (so the rules tables, players
and the `Map` singleton exist), then calls `Map::generate(seed, ret)`
(`0x5D16F0`, the call the New Game screen makes) and reads the resulting
`Cell` array back. One generation takes about two seconds, so any seed, option
set and map size can be generated and compared against the Rust port; the state
at every stage boundary of `generateMap` (`0x5EB580`) is captured on the way.

    python3 oracle.py --seed 12345 --landmass 1 --ocean 1 --size 2 --dump out.txt

Setup: `reverse-engineering/tools/mapgen/README.md`. The harness under
`reverse-engineering/tools/emu` does the emulation; this module only drives it.
"""
import argparse
import hashlib
import os
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(REPO, 'reverse-engineering', 'tools', 'emu'))

import dcl  # noqa: E402

CACHE = os.environ.get('CIV3_ORACLE_CACHE', '/tmp/civ3_oracle_cache')

MAP = 0x9C736C          # the Map singleton (static); `Map::generate`'s `this`
CELL_SIZE = 0xDC        # sizeof(Cell)
CELL_LO, CELL_HI = 0x04, 0x58   # the bytes of a Cell captured (state lives in 0x04..0x38; 0x38..0x58 is scratch)
HEADER_LEN = 0x240      # the part of the Map object captured per stage

# Map object fields (offsets from MAP).
F_CLIMATE_RAW, F_CLIMATE = 0x04, 0x08
F_BARB_RAW, F_BARB = 0x0C, 0x10
F_LAND_RAW, F_LAND = 0x14, 0x18
F_OCEAN_RAW, F_OCEAN = 0x1C, 0x20
F_TEMP_RAW, F_TEMP = 0x24, 0x28
F_AGE_RAW, F_AGE = 0x2C, 0x30
F_SIZE = 0x34
F_CELLS_PTR, F_COUNT = 0x148, 0x40
F_HEIGHT, F_RADIUS, F_CIVS, F_WIDTH = 0x154, 0x158, 0x15C, 0x168
F_SEED, F_WRAP = 0x1EC, 0x1F0
F_CONT_LO, F_CONT_HI, F_CONT_PTR, F_CONT_COUNT = 0x224, 0x228, 0x214, 0x230
WSIZ_TABLE = 0x9C7330   # pointer to the WSIZ rows (stride 0x54)

# The `generateMap` stages, by entry address. A snapshot named `before:<stage>`
# is the state the stage starts from, i.e. the output of the stage before it.
STAGES = [
    ('roll_options', 0x5F1F50),
    ('landmass', 0x5ECEB0),
    ('landmass_fix', 0x5ED440),
    ('coast', 0x5EEB00),
    ('relief', 0x5EDB70),
    ('paint_continents', 0x5EDDB0),
    ('biomes', 0x5F1480),
    ('lakes', 0x5ED5D0),
    ('rivers', 0x5F07D0),
    ('post_process', 0x5EBE80),
    ('resources', 0x5F22A0),
    ('huts', 0x5F21B0),
    ('bonus_grassland', 0x5F2090),
    ('final_pass', 0x5EEEE0),
    ('contour', 0x5D3100),
]


def find_exe():
    for p in (os.environ.get('CIV3_EXE'),
              os.path.join(REPO, '../civ3/civ3-gog/app/Conquests/Civ3Conquests.exe'),
              os.path.join(REPO, '../civ3/re/Civ3Conquests.exe')):
        if p and os.path.exists(p):
            return p
    sys.exit('Civ3Conquests.exe not found; set CIV3_EXE')


def find_template_save():
    p = os.environ.get('CIV3_SAVE')
    if p:
        return p
    for d in ('../civ3/civ3-gog/app/Conquests/Saves', '../civ3/civ3-complete/Conquests/Saves'):
        d = os.path.join(REPO, d)
        for name in sorted(os.listdir(d)) if os.path.isdir(d) else []:
            if name.upper() == 'EGYPT.SAV':
                return os.path.join(d, name)
    sys.exit('no template save found; pass --save or set CIV3_SAVE')


def inflate(path):
    raw = open(path, 'rb').read()
    return dcl.decompress(raw) if dcl.looks_compressed(raw) else raw


def cached(name, data):
    os.makedirs(CACHE, exist_ok=True)
    p = os.path.join(CACHE, f'{name}-{hashlib.sha1(data).hexdigest()[:12]}.raw')
    if not os.path.exists(p):
        open(p, 'wb').write(data)
    return p


class Oracle:
    """A booted emulator with a loaded game and the `Map` singleton at `MAP`."""

    def __init__(self, save=None, verbose=False):
        exe = find_exe()
        os.environ['CIV3_EXE'] = exe
        conquests = os.path.join(os.path.dirname(exe), 'conquests.biq')
        os.environ['CIV3_DEFAULT_BIQ'] = cached('conquests', inflate(conquests))
        # imported late: emu.py reads CIV3_EXE at import time
        import emu as emu_mod
        import loadsave as ls
        from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_FPCW
        self._esp, self._fpcw = UC_X86_REG_ESP, UC_X86_REG_FPCW
        self.STACK_TOP = emu_mod.STACK_TOP
        save = save or find_template_save()
        stream = inflate(save)
        self.template = os.path.basename(save)
        self.e = ls.setup(cached('save', stream), verbose)
        e = self.e
        e.uc.reg_write(self._esp, self.STACK_TOP - 0x1000)
        e.uc.reg_write(self._fpcw, 0x27F)   # the CRT's 53-bit precision control
        # post-load rebuild of trade networks and visibility: minutes of
        # emulation, and irrelevant to map generation
        e.hook_func(0x5D2150, lambda em: 0, nargs=0)
        r = e.call(0x590030, args=(ls.BUF_BASE + e.hdr, 0))
        assert r == len(stream) - e.hdr, f'game_data returned {r:#x}'
        self.snaps = []
        self._recording = False
        self._stop_at = None
        self._install_stage_hooks()

    # ------------------------------------------------------------ memory
    def u16(self, a):
        return struct.unpack('<H', self.e.rd(a, 2))[0]

    def u32(self, a):
        return self.e.u32(a)

    def i32(self, a):
        return struct.unpack('<i', self.e.rd(a, 4))[0]

    def w32(self, a, v):
        self.e.w32(a, v)

    def dims(self):
        return dict(count=self.u16(MAP + F_COUNT), width=self.u32(MAP + F_WIDTH),
                    height=self.u32(MAP + F_HEIGHT), seed=self.u32(MAP + F_SEED),
                    wrap=self.u32(MAP + F_WRAP), size=self.u32(MAP + F_SIZE),
                    civs=self.u32(MAP + F_CIVS), radius=self.u32(MAP + F_RADIUS))

    def options(self):
        g = self.i32
        return dict(climate=g(MAP + F_CLIMATE), barbarians=g(MAP + F_BARB), landmass=g(MAP + F_LAND),
                    ocean=g(MAP + F_OCEAN), temperature=g(MAP + F_TEMP), age=g(MAP + F_AGE),
                    raw=dict(climate=g(MAP + F_CLIMATE_RAW), barbarians=g(MAP + F_BARB_RAW),
                             landmass=g(MAP + F_LAND_RAW), ocean=g(MAP + F_OCEAN_RAW),
                             temperature=g(MAP + F_TEMP_RAW), age=g(MAP + F_AGE_RAW)))

    def cells(self):
        """Bytes `[0x04, 0x38)` of every cell, in cell-index order."""
        d = self.dims()
        ptrs = struct.unpack(f'<{d["count"]}I', self.e.rd(self.u32(MAP + F_CELLS_PTR), 4 * d['count']))
        rd = self.e.rd
        return [rd(p + CELL_LO, CELL_HI - CELL_LO) for p in ptrs]

    def header(self):
        return self.e.rd(MAP, HEADER_LEN)

    def rules(self):
        """The rules tables the resource stage reads, through the `Map`'s own
        list accessor (`getListEntry(tag, index, &out)`, vtable slot `0x8C`;
        index -1 returns the count): `goods` is `(class, frequency)` per `GOOD`
        row (row `+0x3C`, `+0x40`) and `terr` the resource allow-mask bytes of
        every `TERR` row (the array row `+0x08` points to)."""
        e, tag = self.e, lambda s: struct.unpack('<I', s.encode())[0]
        fn = self.u32(self.u32(MAP) + 0x8C)
        scratch = self.STACK_TOP - 0x3000

        def entries(name, size):
            n = e.call(fn, args=(tag(name), 0xFFFFFFFF, 0), ecx=MAP)
            for i in range(n):
                e.w32(scratch, 0)
                e.call(fn, args=(tag(name), i, scratch), ecx=MAP)
                yield e.rd(self.u32(scratch), size)

        goods, good_fx = [], []
        for row in entries('GOOD', 0x5C):
            goods.append(struct.unpack_from('<Ii', row, 0x3C))
            # prerequisite tech, food / shield / commerce bonus (row +0x4C..+0x58)
            good_fx.append(struct.unpack_from('<4i', row, 0x4C))
        masks, terr_fx = [], []
        for row in entries('TERR', 0xF0):
            ptr, count = struct.unpack_from('<I', row, 8)[0], struct.unpack_from('<I', row, 0x60)[0]
            masks.append(e.rd(ptr, (count + 7) // 8))
            # irrigation, mining, road bonus (+0x4C..+0x54), food / shields / commerce
            # (+0x64..+0x6C), worker job (+0x70), the "allow cities" flag byte (+0x78)
            irr, mine, road = struct.unpack_from('<3i', row, 0x4C)
            food, shields, commerce, job = struct.unpack_from('<4i', row, 0x64)
            terr_fx.append((food, shields, commerce, irr, mine, road, job, row[0x78]))
        return dict(goods=goods, terr=masks, good_fx=good_fx, terr_fx=terr_fx)

    def continents(self):
        """`(is_land, size)` for every `Continent` record, the way the exe's
        `getNumContinents` (`0x5DC360`) and `getContinent` (`0x437A50`) read
        them: count = `word[Map+0x230]`, else `Map+0x228 - Map+0x224 + 1`;
        record `i` at `[Map+0x214] + 0x28 * i` (`+0x20` land flag, `+0x24` size).
        Memory reads only, so it is safe inside a code hook."""
        n = self.u16(MAP + F_CONT_COUNT) or (self.u32(MAP + F_CONT_HI) - self.u32(MAP + F_CONT_LO) + 1)
        base = self.u32(MAP + F_CONT_PTR)
        out = []
        for i in range(min(n, 4096)):
            rec = self.e.rd(base + 0x28 * i, 0x28)
            out.append((struct.unpack_from('<I', rec, 0x20)[0], struct.unpack_from('<I', rec, 0x24)[0]))
        return out

    # ------------------------------------------------------------ setup
    def configure(self, size=None, landmass=None, ocean=None, temperature=None, climate=None,
                  age=None, barbarians=None, civs=None, wrap=None, multiplayer=None):
        """Set the world options the New Game screen would, as raw *and* derived
        values (a raw value of 3, or 4 for barbarians, asks `rollRandomOptions`
        to draw one). `size` is a WSIZ row; the cell array is rebuilt with the
        exe's own `setCellCount` (`0x5F3CC0`) when the size changes."""
        raw = dict(climate=F_CLIMATE_RAW, barbarians=F_BARB_RAW, landmass=F_LAND_RAW,
                   ocean=F_OCEAN_RAW, temperature=F_TEMP_RAW, age=F_AGE_RAW)
        for k, off in raw.items():
            v = locals()[k]
            if v is not None:
                self.w32(MAP + off, v & 0xFFFFFFFF)
                self.w32(MAP + off + 4, v & 0xFFFFFFFF)
        if multiplayer is not None:
            # `Map::generate` (`0x5D1717`..`0x5D1752`) tells `generateMap` whether this is a
            # network game from `0x47B530()` or the byte at 0xA52991.
            self.e.wr(0xA52991, bytes([1 if multiplayer else 0]))
        if civs is not None:
            self.w32(MAP + F_CIVS, civs)
        if wrap is not None:
            self.w32(MAP + F_WRAP, wrap)
        if size is not None:
            row = self.u32(WSIZ_TABLE) + size * 0x54
            clamp = lambda v: min(max(v, 16), 362) & ~1
            h, w = clamp(self.u32(row + 0x44)), clamp(self.u32(row + 0x50))
            dist = min(max(self.i32(row + 0x48), 1), 362)   # `0x48E242`: the row's +0x48, clamped to 1..362
            self.w32(MAP + F_SIZE, size)
            self.w32(MAP + F_HEIGHT, h)
            self.w32(MAP + F_WIDTH, w)
            self.w32(MAP + F_RADIUS, dist)
            self.e.call(0x5F3CC0, args=(w // 2 * h,), ecx=MAP)

    def set_dimensions(self, width, height):
        """Non-preset dimensions (the editor's custom sizes)."""
        self.w32(MAP + F_HEIGHT, height)
        self.w32(MAP + F_WIDTH, width)
        self.e.call(0x5F3CC0, args=(width // 2 * height,), ecx=MAP)

    # ------------------------------------------------------------ generate
    def _install_stage_hooks(self):
        from unicorn import UC_HOOK_CODE
        for name, addr in STAGES:
            self.e.uc.hook_add(UC_HOOK_CODE, self._on_stage, begin=addr, end=addr, user_data=name)

    def _on_stage(self, uc, addr, size, name):
        if self._recording:
            self.snap('before:' + name)
        if name == self._stop_at:
            uc.emu_stop()

    def extras(self):
        """Generator scratch arrays that are alive at this point, by name.

        `region` is the `u16[cell count]` region map `paintContinents` builds
        at `Map+0x3C` (freed again at the end of the biome stage)."""
        out = {}
        n = self.u16(MAP + F_COUNT)
        p = self.u32(MAP + 0x3C)
        if p and n:
            try:
                out['region'] = list(struct.unpack(f'<{n}H', self.e.rd(p, 2 * n)))
            except Exception:
                pass
        return out

    def snap(self, name):
        self.snaps.append((name, self.cells(), self.header(), self.continents(), self.extras()))

    def start_scores(self, seed, ret=0):
        """The two per-tile numbers `finalPass` (`0x5EEEE0`) ranks tiles by,
        asked of the exe itself at the entry of that stage: `value` is
        `Map` vtable slot 8 (`0x5D3830`, which calls the city-site evaluator
        `0x442480` with player -1), `shore` is `0x5EEDB0` (the largest water
        body among the 8 neighbours, -1 for none). Runs a second generation
        that stops at `finalPass`, because a stage cannot be paused and asked
        questions in the middle of a call."""
        self._stop_at = 'final_pass'
        self.generate(seed, ret, stages=False)
        self._stop_at = None
        d = self.dims()
        half, signed = d['width'] // 2, lambda v: v - (1 << 32) if v >> 31 else v
        value, shore = [], []
        for i in range(d['count']):
            y = i // half
            x = 2 * (i % half) + (y & 1)
            value.append(signed(self.e.call(0x5D3830, args=(x, y), ecx=MAP)))
            shore.append(signed(self.e.call(0x5EEDB0, args=(x, y), ecx=MAP)))
        return dict(value=value, shore=shore)

    def slots(self, header=None):
        """`Map+0x16C`: 32 dwords, the start location (cell index) of civ 1..31
        in slots 1..31, -1 for none (slot 0 is -1 too)."""
        h = header if header is not None else self.header()
        return list(struct.unpack_from('<32i', h, 0x16C))

    def generate(self, seed, ret=0, stages=True):
        """Run `Map::generate(seed, ret)`; returns the final cells. With
        `stages`, `self.snaps` holds `(name, cells, header, continents, extras)` for
        the start, each stage entry and the end."""
        self.snaps = []
        self._recording = stages
        if stages:
            self.snap('start')
        t = time.time()
        self.e.call(0x5D16F0, args=(seed, ret), ecx=MAP)
        self.seconds = time.time() - t
        if stages:
            self.snap('end')
        self._recording = False
        return self.snaps[-1][1] if stages else self.cells()


# ---------------------------------------------------------------- decoding
def terrain_word(c):
    return struct.unpack_from('<I', c, 0x2C - CELL_LO)[0]


def decode(c):
    """A cell's `[0x04, 0x38)` bytes as a dict of the named fields."""
    u8 = lambda o: c[o - CELL_LO]
    u16 = lambda o: struct.unpack_from('<H', c, o - CELL_LO)[0]
    u32 = lambda o: struct.unpack_from('<I', c, o - CELL_LO)[0]
    return dict(river=u8(0x04), owner=u8(0x05), resource=struct.unpack_from('<i', c, 0x08 - CELL_LO)[0],
                image=u8(0x10), file=u8(0x11), packed=u32(0x14), barbarian=u16(0x18), city=u16(0x1A),
                colony=u16(0x1C), continent=u16(0x1E), depth=u8(0x20), victory=u16(0x22), ruin=u32(0x24),
                overlay=u32(0x28), terrain=u32(0x2C), feature=u32(0x30), flags=u32(0x34))


def cell_xy(index, width):
    half = width // 2
    y = index // half
    return 2 * (index % half) + (y & 1), y


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--save', help='template save (default: the EGYPT save in civ3/)')
    ap.add_argument('--seed', type=lambda s: int(s, 0), required=True)
    ap.add_argument('--size', type=int, help='WSIZ row (0 tiny .. 5 huge)')
    ap.add_argument('--landmass', type=int)
    ap.add_argument('--ocean', type=int)
    ap.add_argument('--temperature', type=int)
    ap.add_argument('--climate', type=int)
    ap.add_argument('--age', type=int)
    ap.add_argument('--barbarians', type=int)
    ap.add_argument('--civs', type=int)
    ap.add_argument('--wrap', type=int)
    ap.add_argument('--dump', help='write the final terrain plane as text')
    a = ap.parse_args()
    o = Oracle(a.save)
    o.configure(size=a.size, landmass=a.landmass, ocean=a.ocean, temperature=a.temperature,
                climate=a.climate, age=a.age, barbarians=a.barbarians, civs=a.civs, wrap=a.wrap)
    cells = o.generate(a.seed)
    d = o.dims()
    print(f'template {o.template}: {d["width"]}x{d["height"]} wrap {d["wrap"]} civs {d["civs"]} '
          f'options {o.options()} generated in {o.seconds:.1f}s')
    print('stages:', ' '.join(n for n, *_ in o.snaps))
    if a.dump:
        with open(a.dump, 'w') as f:
            for i, c in enumerate(cells):
                f.write(f'{terrain_word(c):08x}\n')


if __name__ == '__main__':
    main()
