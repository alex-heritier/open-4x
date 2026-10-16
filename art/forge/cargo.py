"""A directional wooden cargo cart, using the same lit rig and nation accents as troops."""
import math
from .rig import Scene, ease, facing_frame, oval_shadow, span
from .troops import CLIPS, wheel
from .svgkit import shade


def frame(facing=0, clip='idle', t=0.0):
    sc = Scene(160, 160, (80, 108), 0.9)
    xf = facing_frame(facing)
    a = math.tau * t
    bounce = 0.8 * math.sin(2 * a) if clip == 'run' else 0.0
    fall = ease(span(t, 0.18, 0.8)) if clip == 'death' else 0.0
    oval_shadow(sc, xf, 38, 24, op=0.35)
    rock = 2.0 * math.sin(math.pi * t) if clip == 'victory' else 0.0
    recoil = -2.0 * math.sin(math.pi * t) if clip == 'attack' else 0.0
    W = xf.at((recoil, 0, bounce)).roll(68 * fall + rock).at((0, 0, -7 * fall))
    # Iron tyres, wooden spokes, axle and the two shafts all turn with the cart.
    sc.limb(W((0, -26, 12)), W((0, 26, 12)), 2.2, 2.2, '#393b3b')
    for side in (-1, 1):
        wheel(sc, W, (0, side * 25, 12), -360 * t if clip == 'run' else 0, 0.65)
        sc.limb(W((-27, side * 17, 19)), W((57, side * 17, 12)), 1.8, 1.3, '#77532f')
    sc.box(W, -28, 28, -23, 23, 16, 22, '#65462e', top='#a48051')
    for z0, z1, x0, x1, y0, y1 in [(22, 45, -24, 24, -20, 20), (45, 64, -20, 17, -18, 18)]:
        wood = '#ab8050' if z0 == 22 else '#bc955f'
        sc.box(W, x0, x1, y0, y1, z0, z1, wood, top=shade(wood, 1.16), sw=0.6)
        for side in (-1, 1):
            y = (y1 + 0.2) if side > 0 else (y0 - 0.2)
            for z in range(z0 + 5, z1, 5):
                sc.line(W((x0, y, z)), W((x1, y, z)), shade(wood, 0.75), 0.5, bias=-0.2)
            # Metal corner nails; cloth binding is the only national paint.
            for x in (x0 + 2, x1 - 2):
                for z in (z0 + 2, z1 - 2):
                    sc.ball(W((x, y, z)), 0.6, '#45403a', ink=None, bias=-0.25)
            for x in (-10, 9):
                sc.poly([W((x-1.6,y,z0)),W((x+1.6,y,z0)),W((x+1.6,y,z1)),W((x-1.6,y,z1))], '#8b3930', None, two=True, bias=-0.3, accent=True)
        for x in (-10, 9):
            sc.poly([W((x-1.6,y0,z1+0.2)),W((x+1.6,y0,z1+0.2)),W((x+1.6,y1,z1+0.2)),W((x-1.6,y1,z1+0.2))], '#8b3930', None, two=True, bias=-0.3, accent=True)
    return sc.render()


BUILDERS = {'crate': frame}


def build():
    # Keep the standalone build group on the shared unit-sheet pipeline.
    from build import build_units
    build_units(['crate'])
