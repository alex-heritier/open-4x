"""Differential test: the Rust `Fractal::sample` / `Fractal::percentile` against the exe.

Runs `sampleHeight` (0x5E2180) and `percentileLookup` (0x5E2280) from the PE image under
Unicorn, on fractals the exe itself generated (0x5E1B60), and compares with the port
(`cargo run --example fractal_probe`) on the same inputs.

    /tmp/emu/bin/python diff_sample.py [seed] [cases] [fpcw]
    FIXTURES=reverse-engineering/rust/tests/data/fractal_probe.txt \\
        /tmp/emu/bin/python diff_sample.py 5 14      # also write the exe's answers out

`fpcw` is the x87 control word to run the exe under; the default 0x27F is the C
runtime's (53-bit mantissa, round to nearest), 0x37F is the hardware reset value
(64-bit mantissa). The port's f64 arithmetic only equals the first. The sample points
include the grid-aligned ones (x * 128/W within an ulp of an integer), which is where the
two differ.
"""
import random
import subprocess
import sys

from unicorn.x86_const import UC_X86_REG_FPCW

from emu import Emu

RUST_DIR = __import__("os").path.abspath(
    __import__("os").path.join(__import__("os").path.dirname(__file__), "../../../../../reverse-engineering/rust"))


def rust(lines):
    p = subprocess.run(["cargo", "run", "--release", "-q", "--example", "fractal_probe"], cwd=RUST_DIR,
                       input="\n".join(lines) + "\n", capture_output=True, text=True)
    if p.returncode != 0:
        print(p.stderr)
        sys.exit(1)
    return p.stdout.strip().split("\n")


def main():
    rnd = random.Random(int(sys.argv[1]) if len(sys.argv) > 1 else 5)
    cases = int(sys.argv[2]) if len(sys.argv) > 2 else 60
    fpcw = int(sys.argv[3], 0) if len(sys.argv) > 3 else 0x27F
    e = Emu()
    fm = e.alloc(0x20E4)
    lines, want = [], []
    for _ in range(cases):
        w = rnd.choice([40, 50, 60, 64, 80, 96, 100, 110, 120, 128, 140, 160, 200, 256])
        h = rnd.choice([40, 50, 60, 64, 80, 96, 100, 120, 128, 132, 160, 256])
        level = rnd.choice([2, 3])
        flags = rnd.choice([0, 1, 8, 9, 3]) | rnd.choice([0, 4])
        seed = rnd.getrandbits(32) or 1
        e.write(fm, bytes(0x20E4))
        e.thiscall(0x5E1B60, fm, w, h, level, flags, seed, 0)
        base = f"{w} {h} {level} {flags} {seed}"

        pts = [(rnd.randrange(w), rnd.randrange(h)) for _ in range(150)]
        # grid-aligned columns/rows: where x*128/w is (nearly) an integer
        for k in range(1, 20):
            gx = (k * w) // 128 if (k * w) % 128 == 0 else round(k * w / 128)
            gy = round(k * h / 64)
            pts.append((min(gx, w - 1), rnd.randrange(h)))
            pts.append((rnd.randrange(w), min(gy, h - 1)))
            pts.append((min(gx, w - 1), min(gy, h - 1)))
        e.mu.reg_write(UC_X86_REG_FPCW, fpcw)
        got = [e.thiscall(0x5E2180, fm, x, y) & 0xFF for x, y in pts]
        lines.append(f"{base} S " + " ".join(f"{x} {y}" for x, y in pts))
        want.append(" ".join(map(str, got)))

        pcts = list(range(0, 101)) + [rnd.randrange(101) for _ in range(20)]
        got = [e.thiscall(0x5E2280, fm, p) & 0xFF for p in pcts]
        lines.append(f"{base} P " + " ".join(map(str, pcts)))
        want.append(" ".join(map(str, got)))

    fixtures = __import__("os").environ.get("FIXTURES")
    if fixtures:
        with open(fixtures, "w") as f:
            for l, a in zip(lines, want):
                f.write(l + " = " + a + "\n")
        print("wrote", fixtures)

    have = rust(lines)
    bad = 0
    for l, a, b in zip(lines, want, have):
        if a != b:
            bad += 1
            xs, ys = a.split(), b.split()
            d = [(i, x, y) for i, (x, y) in enumerate(zip(xs, ys)) if x != y]
            print("MISMATCH", l[:60], f"{len(d)} of {len(xs)} differ, first {d[:4]}")
    print(f"{len(lines) - bad}/{len(lines)} probe lines identical (fpcw {fpcw:#x})")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
