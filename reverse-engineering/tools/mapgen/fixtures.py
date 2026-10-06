#!/usr/bin/env python3
"""Write oracle fixtures for the Rust tests (`rust/tests/data/oracle/*.txt`).

    python3 fixtures.py                 # every scenario in SCENARIOS
    python3 fixtures.py tiny_pangaea    # one scenario
    python3 fixtures.py --out DIR --seed 7 --size 1 --landmass 1 --name probe

A fixture holds, for each `generateMap` stage entry (= the previous stage's
output), the cell planes that changed since the stage before; planes not
listed are unchanged. Planes are run-length encoded: `value` or `value*count`,
space separated, hex. Format (`rust/src/oracle.rs` reads it):

    civ3-mapgen-oracle 1
    scenario NAME / template SAVE / width W / height H / wrap F / seed S / civs N / size I
    opt NAME VALUE ...                  (the derived option values after the run)
    raw NAME VALUE ...                  (the values the run was configured with)
    radius R                            (Map+0x158, the WSIZ row's civ distance)
    ret N                               (Map::generate's second argument: the seafaring civ count)
    multiplayer 0|1                     (Map::generate's network-game flag, generateMap's second argument)
    goods CLASS:FREQ ...                (the GOOD rows: class 0 bonus, 1 luxury, 2 strategic; frequency)
    terr HEX ...                        (the TERR rows' resource allow-mask bytes, hex)
    stage NAME                          (state at the entry of stage NAME; "end" = final)
    plane NAME RLE ...
    continents N IS_LAND SIZE ...
    slots V0 ... V31                    (Map+0x16C, the start cell index of each civ slot; when it changed)

The `final_pass` entry also lists the scratch planes `value` (what `Map` vtable
slot 8 returns for each tile: the city-site score `finalPass` ranks by) and
`shore` (the `0x5EEDB0` water-body id next to each tile, or -1).
"""
import argparse
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import oracle  # noqa: E402

REPO = oracle.REPO
OUT = os.path.join(REPO, 'reverse-engineering/rust/tests/data/oracle')

# (name, offset, struct format)
PLANES = [
    ('river', 0x04, '<B'),
    ('resource', 0x08, '<I'),
    ('image', 0x10, '<B'),
    ('file', 0x11, '<B'),
    ('continent', 0x1E, '<H'),
    ('overlay', 0x28, '<I'),
    ('terrain', 0x2C, '<I'),
    ('feature', 0x30, '<I'),
]

# Option sets chosen to cover every landmass style, ocean row, climate,
# temperature and age value, two sizes and a few wrap settings.
SCENARIOS = {
    'tiny_archipelago': dict(size=0, landmass=0, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=101, wrap=5, ret=2),
    'tiny_continents': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=202, wrap=5, ret=1),
    'tiny_pangaea': dict(size=0, landmass=2, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=303, wrap=5, ret=3),
    'tiny_dry_cool': dict(size=0, landmass=1, ocean=2, temperature=2, climate=0, age=0, barbarians=1, seed=404, wrap=5),
    'tiny_wet_warm': dict(size=0, landmass=1, ocean=0, temperature=0, climate=2, age=2, barbarians=1, seed=505, wrap=5, ret=8),
    'tiny_flat': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=606, wrap=0),
    'small_continents': dict(size=1, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=4242, wrap=5, ret=2),
    # Every wrap-flag value the map can carry (bit 0 x, bit 1 y, bit 2 unused by the generator).
    'tiny_wrap_x': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=1111, wrap=1, ret=2),
    'tiny_wrap_y': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=2222, wrap=2, ret=1),
    'tiny_wrap_xy': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=3333, wrap=3, ret=3),
    'tiny_wrap_7': dict(size=0, landmass=2, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=3334, wrap=7),
    # Every option slot random: rollRandomOptions draws for each of them.
    'tiny_random': dict(size=0, landmass=3, ocean=3, temperature=3, climate=3, age=3, barbarians=4, seed=4444, wrap=5),
    'small_random': dict(size=1, landmass=3, ocean=3, temperature=3, climate=3, age=3, barbarians=4, seed=4445, wrap=5),
    # Civilisation counts (the start placement depends on them).
    'tiny_civs3': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=5555, wrap=5, civs=3, ret=1),
    'tiny_civs16': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=5556, wrap=5, civs=16, ret=4),
    # A network game: the first civ's start is not moved to the two-thirds mark and the slots are not grouped by continent.
    'tiny_multiplayer': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=6007, wrap=5, ret=2, multiplayer=True),
    'small_multiplayer': dict(size=1, landmass=0, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=6008, wrap=5, ret=0, multiplayer=True, civs=12),
    # Barbarian settings (-1 = none, which leaves out the goody huts) and more civ counts.
    'tiny_no_barbs': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=-1, seed=6001, wrap=5),
    'tiny_raging': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=3, seed=6002, wrap=5, ret=1),
    'tiny_civs5': dict(size=0, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=0, seed=6003, wrap=5, civs=5),
    'small_civs12': dict(size=1, landmass=0, ocean=1, temperature=1, climate=1, age=1, barbarians=2, seed=6004, wrap=3, civs=12, ret=5),
    # Larger worlds (the size slider changes the number of continents, the river quota, ...).
    'standard_continents': dict(size=2, landmass=1, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=7777, wrap=5, ret=3),
    'standard_archipelago': dict(size=2, landmass=0, ocean=3, temperature=2, climate=2, age=0, barbarians=1, seed=8888, wrap=5, ret=2),
    'large_pangaea': dict(size=3, landmass=2, ocean=2, temperature=0, climate=1, age=2, barbarians=1, seed=9999, wrap=5),
    'huge_archipelago': dict(size=4, landmass=0, ocean=1, temperature=1, climate=1, age=1, barbarians=1, seed=6005, wrap=5, ret=2),
    # A custom size (the editor's): the resource spacing grows with the map up to a limit.
    'custom_220x200': dict(size=2, dims=(220, 200), landmass=1, ocean=1, temperature=1, climate=1, age=1,
                           barbarians=1, seed=6006, wrap=5, ret=3),
}


def rle(values):
    out, i = [], 0
    while i < len(values):
        j = i
        while j < len(values) and values[j] == values[i]:
            j += 1
        out.append(f'{values[i]:x}' + (f'*{j - i}' if j - i > 1 else ''))
        i = j
    return ' '.join(out)


def plane(cells, off, fmt):
    lo = off - oracle.CELL_LO
    n = struct.calcsize(fmt)
    return [struct.unpack_from(fmt, c, lo)[0] for c in cells]


def u32(values):
    return [v & 0xFFFFFFFF for v in values]


def write_fixture(o, name, path, ret=0, scores=None, multiplayer=False):
    d = o.dims()
    snaps = [s for s in o.snaps if s[0] != 'start']
    with open(path, 'w') as f:
        f.write('civ3-mapgen-oracle 1\n')
        f.write(f'scenario {name}\ntemplate {o.template}\nwidth {d["width"]}\nheight {d["height"]}\n')
        f.write(f'wrap {d["wrap"]}\nseed {o.seed}\ncivs {d["civs"]}\nsize {d["size"]}\n')
        f.write(f'radius {d["radius"]}\nret {ret}\nmultiplayer {int(multiplayer)}\n')
        opts = o.options()
        f.write('opt ' + ' '.join(f'{k} {v}' for k, v in opts.items() if k != 'raw') + '\n')
        f.write('raw ' + ' '.join(f'{k} {v}' for k, v in opts['raw'].items()) + '\n')
        rules = o.rules()
        f.write('goods ' + ' '.join(f'{c}:{q}' for c, q in rules['goods']) + '\n')
        f.write('terr ' + ' '.join(m.hex() for m in rules['terr']) + '\n')
        f.write('goodfx ' + ' '.join(':'.join(map(str, r)) for r in rules['good_fx']) + '\n')
        f.write('terrfx ' + ' '.join(':'.join(map(str, r)) for r in rules['terr_fx']) + '\n')
        prev, prev_slots = None, None
        for sname, cells, header, conts, extras in snaps:
            f.write(f'stage {sname.replace("before:", "")}\n')
            if scores and sname.replace('before:', '') == 'final_pass':
                extras = dict(extras, **{k: u32(v) for k, v in scores.items()})
            slots = o.slots(header)
            if slots != prev_slots:
                f.write('slots ' + ' '.join(map(str, slots)) + '\n')
                prev_slots = slots
            cur = {p: plane(cells, off, fmt) for p, off, fmt in PLANES}
            for p, _, _ in PLANES:
                if prev is None or cur[p] != prev[p]:
                    f.write(f'plane {p} {rle(cur[p])}\n')
            prev = cur
            # Scratch arrays are listed whenever they are alive; they are not
            # carried forward by the reader.
            for k, v in extras.items():
                f.write(f'plane {k} {rle(v)}\n')
            # The Continent records are stale until the land/sea stage has
            # numbered the continents, so they are not written before then.
            if sname.replace('before:', '') not in ('roll_options', 'landmass'):
                f.write('continents %d %s\n' % (len(conts), ' '.join(f'{a} {b}' for a, b in conts)))
            f.write('header150 %d\n' % struct.unpack_from('<I', header, 0x150)[0])


def boot(params, seed):
    o = oracle.Oracle()
    params = dict(params)
    params.pop('seed', None)
    dims = params.pop('dims', None)
    params.pop('ret', None)
    o.configure(**params)
    if dims:
        o.set_dimensions(*dims)
    o.seed = seed
    return o


def run(name, params, out):
    seed, ret = params['seed'], params.get('ret', 0)
    o = boot(params, seed)
    o.generate(seed, ret)
    scores = boot(params, seed).start_scores(seed, ret)
    path = os.path.join(out, name + '.txt')
    write_fixture(o, name, path, ret, scores, bool(params.get('multiplayer')))
    print(f'{name}: {os.path.getsize(path) // 1024} KiB, {len(o.snaps)} snapshots, {o.seconds:.1f}s')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('scenarios', nargs='*')
    ap.add_argument('--out', default=OUT)
    ap.add_argument('--name')
    for k in ('seed', 'size', 'landmass', 'ocean', 'temperature', 'climate', 'age', 'barbarians', 'civs', 'wrap', 'ret', 'multiplayer'):
        ap.add_argument('--' + k, type=int)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    if a.name:
        params = {k: getattr(a, k) for k in ('seed', 'size', 'landmass', 'ocean', 'temperature', 'climate',
                                             'age', 'barbarians', 'civs', 'wrap', 'ret', 'multiplayer') if getattr(a, k) is not None}
        run(a.name, params, a.out)
        return
    for n in a.scenarios or SCENARIOS:
        run(n, SCENARIOS[n], a.out)


if __name__ == '__main__':
    main()
