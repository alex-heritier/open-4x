"""Animated land units: line infantry, pioneers, workers, dragoon cavalry and field artillery.

Each figure is a 3D puppet (see `rig.py`) posed by a clip function `pose(clip, t)` and drawn from any of the eight
facings.  Sprites are 160 x 160 with the ground point of the tile at (80, 108), as before.  Clips:

    idle     loops: breathing, a glance around, a horse's tail and head
    run      loops: one stride cycle, drawn in place (the game slides the sprite between squares)
    attack   a volley, a sabre charge or a gun firing; the shot comes 30% of the way in, a charge connects at 50%
    victory  a cheer that ends back at rest
    death    hit, crumple and fall; the last frame lies on the ground

Lit from the upper left, dark ink outlines, colours from the original 2D figures.
"""

import math

from .rig import Scene, Xf, add, ease, facing_frame, ground_shadow, oval_shadow, keys, mix, mul, smoke, span, sub, two_bone, unit
from .svgkit import shade

U = 160
ORIGIN = (80, 108)
SCALE = 0.86
INK = "#17130f"
SKIN = "#dcaa80"
NAVY = "#33465f"
SLATE = "#59667a"
LEATHER = "#2a1d14"
BRASS = "#cda95c"
CREAM = "#e4dcc3"
RED = "#a8322a"
STEEL = "#cfd5d8"
WOOD = "#7a5532"

CLIPS = {
    # name: (frames, milliseconds per frame, loops)
    "idle": (8, 160, True),
    "run": (8, 60, True),
    "attack": (10, 80, False),
    "victory": (8, 90, False),
    "death": (10, 90, False),
}
FIRE_AT = 0.30
STRIKE_AT = 0.50


# ---- a person -----------------------------------------------------------------------------------
class Kit:
    """What a figure wears and carries."""

    def __init__(self, coat=NAVY, trousers=SLATE, cap=NAVY, band=RED, hat=None, pack="knapsack", tool="rifle", plume=None):
        self.coat, self.trousers, self.cap, self.band, self.hat = coat, trousers, cap, band, hat
        self.pack, self.tool, self.plume = pack, tool, plume


SOLDIER = Kit()
PIONEER = Kit(coat="#8a6a43", trousers="#55483a", hat="#5d4a32", pack="rucksack", tool="pick")
WORKER = Kit(coat="#4f6a82", trousers="#4a4036", hat="#7a6a4a", pack="rucksack", tool="shovel")
GUNNER = Kit(trousers="#4f5b6d", pack=None, tool=None)
TROOPER = Kit(trousers="#d8d2c0", pack=None, tool="sabre", plume="#e9e2cf")


class Pose:
    """A figure's posture in its own frame (x forward, y left, z up); feet on the ground at the origin."""

    def __init__(self):
        self.hip = (0.0, 0.0, 42.0)
        self.lean = 0.0  # torso pitch forward, degrees
        self.side = 0.0  # torso lean to the left, degrees
        self.twist = 0.0  # torso yaw, degrees
        self.head = (0.0, 0.0)  # yaw, pitch (down)
        self.feet = [(0.5, 5.5, 0.0), (0.5, -5.5, 0.0)]  # left, right
        self.hands = [None, None]  # left, right; None hangs the arm
        self.tool = None  # (grip, direction) for the carried tool, or None to sling it
        self.seated = False  # a rider: thighs forward over the saddle
        self.fall = 0.0  # topple backward around the feet, degrees
        self.fall_pivot = (0.0, 0.0, 0.0)
        self.lift = 0.0
        self.smoke = None  # (point in the figure frame, radius, opacity)


def _torso_axes(p):
    lean, side, twist = math.radians(p.lean), math.radians(p.side), math.radians(p.twist)
    up = unit((math.sin(lean), math.sin(side), math.cos(lean) * math.cos(side)))
    fwd = unit((math.cos(lean) * math.cos(twist), math.sin(twist), -math.sin(lean)))
    left = unit((-math.sin(twist), math.cos(twist), 0.0))
    return up, fwd, left


def person(sc: Scene, xf: Xf, p: Pose, kit: Kit, layer=0):
    """Draw one person posed by `p` through the frame `xf`."""
    if p.fall:
        piv = p.fall_pivot
        xf = xf.at(piv).at((0, 0, p.lift)).pitch(-p.fall).at(mul(piv, -1))
    W = xf  # figure -> world
    up, fwd, left = _torso_axes(p)
    hip = p.hip
    neck = add(hip, mul(up, 35.0))
    shoulders = [add(add(neck, mul(left, 11.5)), mul(up, -3.0)), add(add(neck, mul(left, -11.5)), mul(up, -3.0))]
    world_fwd = W.vec(fwd)

    def L(a, b, ra, rb, col, **kw):
        sc.limb(W(a), W(b), ra, rb, col, layer=layer, **kw)

    # legs
    for i, side in enumerate((1, -1)):
        h = add(hip, (0, 5.0 * side, -2.0))
        foot = p.feet[i]
        ankle = add(foot, (-1.0, 0, 4.5))
        pole = W.vec((1.0, 0.0, 0.0)) if not p.seated else W.vec((1.0, 0.25 * side, 0.6))
        knee = two_bone(W(h), W(ankle), 18.6, 18.0, pole)
        sc.limb(W(h), knee, 5.4, 4.2, kit.trousers, layer=layer)
        sc.limb(knee, W(ankle), 4.2, 3.3, kit.trousers, layer=layer)
        toe = add(ankle, (7.0, 0, -3.0))
        heel = add(ankle, (-3.0, 0, -3.5))
        sc.limb(W(heel), W(toe), 3.2, 2.6, "#1f1812", layer=layer, bias=-0.3)
    # torso: coat as an elliptical tube from the tails to the shoulders
    base = add(hip, mul(up, -6.0))
    top = add(hip, mul(up, 33.0))
    sc.tube(W(base), W(top), 13.0, 11.8, kit.coat, side=W.vec(left), squash=0.55, sides=14, layer=layer)
    sc.tube(W(add(hip, mul(up, 3.2))), W(add(hip, mul(up, 6.6))), 12.4, 12.3, "#211a14", side=W.vec(left), squash=0.6, sides=14, caps=False, layer=layer, bias=-0.4)
    front = mul(fwd, 6.6)
    back = mul(fwd, -6.8)
    facing_viewer = W.vec(fwd)[1] < 0.2  # chest toward the camera
    if kit is not WORKER and kit is not PIONEER:
        # white cross belts on the chest and the back
        for sgn in (1, -1):
            a = add(add(neck, mul(left, 9.0 * sgn)), mul(up, -4.0))
            b = add(add(hip, mul(left, -10.0 * sgn)), mul(up, 6.0))
            sc.poly([W(add(a, front)), W(add(add(a, front), mul(left, -2.8 * sgn))), W(add(add(b, front), mul(left, 2.8 * sgn))), W(add(b, front))], CREAM, None, layer=layer, bias=-0.6, lit=False, flat=shade(CREAM, 0.95))
            sc.poly([W(add(b, back)), W(add(add(b, back), mul(left, 2.8 * sgn))), W(add(add(a, back), mul(left, -2.8 * sgn))), W(add(a, back))], CREAM, None, layer=layer, bias=-0.6, lit=False, flat=shade(CREAM, 0.8))
        if facing_viewer:
            for k in range(5):
                c = W(add(add(hip, mul(up, 10 + k * 5.2)), mul(fwd, 6.9)))
                sc.draw(c, lambda svg, x, y: svg.circle(x, y, 0.95, BRASS), layer=layer, bias=-0.8)
    else:
        # shoulder strap and a tool pouch
        a = add(add(neck, mul(left, -9.0)), mul(up, -3.0))
        b = add(add(hip, mul(left, 9.0)), mul(up, 5.0))
        sc.poly([W(add(a, front)), W(add(add(a, front), mul(left, 2.6))), W(add(add(b, front), mul(left, -2.6))), W(add(b, front))], "#4a3a28", None, layer=layer, bias=-0.6)
        sc.box(W.at(add(hip, add(mul(fwd, 5.0), mul(left, 7.0)))), -1.5, 1.5, -3, 3, -3, 4, "#6a5236", layer=layer, bias=-0.5)
    # collar and epaulettes
    sc.tube(W(add(neck, mul(up, -3.6))), W(add(neck, mul(up, -0.4))), 5.8, 5.2, kit.band if kit.tool != "pick" and kit.tool != "shovel" else kit.coat, side=W.vec(left), sides=10, layer=layer, bias=-0.2)
    if kit.tool in ("rifle", "sabre", None):
        for s in shoulders:
            sc.ball(W(add(s, mul(up, 1.5))), 3.4, shade(BRASS, 0.9), squash=0.6, layer=layer, bias=-0.5)
    # pack on the back
    if kit.pack == "knapsack":
        bx = W.at(add(add(hip, mul(up, 22.0)), mul(fwd, -9.5))).yaw(p.twist).pitch(p.lean)
        sc.box(bx, -3.0, 3.0, -9.0, 9.0, -9.0, 8.0, "#3a2c20", layer=layer, bias=0.5)
        roll_a = W(add(add(add(hip, mul(up, 31.5)), mul(fwd, -9.5)), mul(left, 10.0)))
        roll_b = W(add(add(add(hip, mul(up, 31.5)), mul(fwd, -9.5)), mul(left, -10.0)))
        sc.tube(roll_a, roll_b, 3.4, 3.4, "#a58f67", sides=10, layer=layer, bias=0.4)
    elif kit.pack == "rucksack":
        bx = W.at(add(add(hip, mul(up, 20.0)), mul(fwd, -10.0))).yaw(p.twist).pitch(p.lean)
        sc.box(bx, -3.5, 3.5, -7.5, 7.5, -12.0, 9.0, "#6e5a3a", layer=layer, bias=0.5)
        roll_a = W(add(add(add(hip, mul(up, 30.5)), mul(fwd, -10.0)), mul(left, 8.0)))
        roll_b = W(add(add(add(hip, mul(up, 30.5)), mul(fwd, -10.0)), mul(left, -8.0)))
        sc.tube(roll_a, roll_b, 3.0, 3.0, "#c4b38a", sides=10, layer=layer, bias=0.4)
    # arms
    for i, s in enumerate(shoulders):
        sgn = 1 if i == 0 else -1
        hand = p.hands[i]
        if hand is None:
            hand = add(add(s, mul(up, -29.0)), add(mul(left, 2.5 * sgn), mul(fwd, 1.0)))
        pole = W.vec(add(mul(fwd, -1.0), mul(left, 0.6 * sgn)))
        elbow = two_bone(W(s), W(hand), 16.0, 15.0, pole)
        sc.limb(W(s), elbow, 4.0, 3.4, kit.coat, layer=layer)
        sc.limb(elbow, W(hand), 3.4, 2.9, kit.coat, layer=layer)
        sc.ball(W(hand), 2.9, SKIN, sw=0.6, layer=layer, bias=-0.2)
    # head
    hy, hp = math.radians(p.head[0] + p.twist), math.radians(p.head[1])
    hf = unit((math.cos(hy) * math.cos(hp), math.sin(hy) * math.cos(hp), -math.sin(hp)))
    hl = unit((-math.sin(hy), math.cos(hy), 0.0))
    hu = unit((-hf[0] * hf[2], -hf[1] * hf[2], 1 - hf[2] * hf[2])) if abs(hf[2]) > 1e-6 else (0.0, 0.0, 1.0)
    hu = add(mul(hu, 0.85), mul(up, 0.15))
    sc.limb(W(neck), W(add(neck, mul(up, 5.0))), 2.6, 2.6, shade(SKIN, 0.85), layer=layer)
    hc = add(neck, add(mul(up, 9.5), mul(hf, 0.5)))
    sc.ball(W(hc), 6.3, SKIN, squash=1.08, layer=layer)
    face = W.vec(hf)
    seen = -face[1] * 0.9 + face[2] * 0.45  # how much of the face looks at the camera
    if seen > 0.05:
        op = min(1.0, seen * 2.5)
        for e in (1, -1):
            c = W(add(add(add(hc, mul(hf, 5.6)), mul(hl, 2.1 * e)), mul(hu, 0.6)))
            sc.draw(c, lambda svg, x, y, op=op: svg.ellipse(x, y, 0.8 * SCALE, 1.0 * SCALE, "#3a2418", op=op), layer=layer, bias=-0.7)
        m = W(add(add(hc, mul(hf, 5.8)), mul(hu, -2.6)))
        sc.draw(m, lambda svg, x, y, op=op: svg.rect(x - 2.6 * SCALE, y - 0.7, 5.2 * SCALE, 1.4, "#3b2a1d", rx=0.5, op=op * 0.85), layer=layer, bias=-0.7)
    if kit.hat:
        # brimmed hat
        brim_c = add(hc, mul(hu, 5.2))
        sc.disk(W(brim_c), W.vec(hu), 11.5, shade(kit.hat, 0.9), sides=18, layer=layer, bias=-0.3)
        sc.tube(W(brim_c), W(add(brim_c, mul(hu, 8.0))), 5.6, 4.9, kit.hat, sides=12, layer=layer, bias=-0.4)
    else:
        # kepi: a drum tilted forward, a band, a visor and a brass badge
        k0 = add(hc, add(mul(hu, 3.0), mul(hf, -0.3)))
        k1 = add(hc, add(mul(hu, 11.5), mul(hf, 1.6)))
        sc.tube(W(k0), W(k1), 6.6, 5.8, kit.cap, side=W.vec(hl), sides=14, layer=layer, bias=-0.3)
        sc.tube(W(k0), W(add(k0, mul(hu, 2.6))), 6.7, 6.6, kit.band, side=W.vec(hl), sides=14, caps=False, layer=layer, bias=-0.4)
        v = [add(k0, add(mul(hf, 5.0), mul(hl, 4.5))), add(k0, add(mul(hf, 5.0), mul(hl, -4.5))), add(k0, add(mul(hf, 9.5), add(mul(hl, -3.5), mul(hu, -1.2)))), add(k0, add(mul(hf, 9.5), add(mul(hl, 3.5), mul(hu, -1.2))))]
        sc.poly([W(q) for q in v], "#1a1511", two=True, layer=layer, bias=-0.5)
        if seen > 0.1:
            sc.draw(W(add(k0, add(mul(hf, 6.7), mul(hu, 2.0)))), lambda svg, x, y: svg.circle(x, y, 1.2, BRASS), layer=layer, bias=-0.9)
        if kit.plume:
            a = W(add(k1, mul(hf, 0.5)))
            b = W(add(add(k1, mul(hu, 8.0)), mul(hf, -4.0)))
            sc.limb(a, b, 1.6, 2.4, kit.plume, layer=layer, bias=-0.6)
    # the tool: a rifle, a pick, a shovel or a sabre
    tool = p.tool
    if kit.tool and tool is not None:
        grip, d = tool
        draw_tool(sc, W, kit.tool, grip, d, layer)
    elif kit.tool in ("rifle",) and tool is None:
        # slung across the back
        a = add(add(hip, mul(up, 6.0)), add(mul(fwd, -9.0), mul(left, -9.0)))
        draw_tool(sc, W, kit.tool, a, unit(add(mul(up, 1.0), mul(left, 0.7))), layer, bias=0.6)
    if p.smoke:
        at, r, op = p.smoke
        sc.draw(W(at), lambda svg, x, y: smoke(svg, x, y, r * SCALE, op), layer=layer + 1)


def draw_tool(sc, W, kind, grip, d, layer, bias=0.0):
    """A rifle, pick, shovel or sabre held at `grip` pointing along `d` (figure frame)."""
    d = unit(d)
    if kind == "rifle":
        butt = add(grip, mul(d, -14.0))
        muzzle = add(grip, mul(d, 48.0))
        sc.limb(W(butt), W(add(grip, mul(d, 4.0))), 2.0, 1.5, "#7e5632", sw=0.6, layer=layer, bias=bias - 0.1)
        sc.limb(W(add(grip, mul(d, 2.0))), W(muzzle), 1.5, 1.1, "#6a4a2c", sw=0.5, layer=layer, bias=bias - 0.1)
        sc.line(W(add(grip, mul(d, 10.0))), W(muzzle), "#4b5054", 0.9, layer=layer, bias=bias - 0.15)
        sc.line(W(muzzle), W(add(muzzle, mul(d, 17.0))), STEEL, 1.3, layer=layer, bias=bias - 0.15)
    elif kind in ("pick", "shovel"):
        end = add(grip, mul(d, 30.0))
        start = add(grip, mul(d, -40.0))
        sc.limb(W(start), W(end), 1.5, 1.5, "#7a5634", sw=0.6, layer=layer, bias=bias - 0.1)
        side = unit((d[1], -d[0], 0.0)) if abs(d[2]) < 0.95 else (0.0, 1.0, 0.0)
        if kind == "pick":
            perp = unit(add(mul(side, 0.2), (0.0, 0.0, 1.0))) if abs(d[2]) < 0.6 else (1.0, 0.0, 0.0)
            a = add(end, mul(perp, 11.0))
            b = add(end, mul(perp, -11.0))
            sc.limb(W(a), W(end), 0.8, 2.2, "#8c949a", sw=0.6, layer=layer, bias=bias - 0.2)
            sc.limb(W(end), W(b), 2.2, 0.8, "#8c949a", sw=0.6, layer=layer, bias=bias - 0.2)
        else:
            blade_w = mul(side, 5.5)
            t0 = add(end, mul(d, 2.0))
            t1 = add(end, mul(d, 15.0))
            sc.poly([W(add(t0, blade_w)), W(add(t1, mul(blade_w, 0.8))), W(add(t1, mul(blade_w, -0.8))), W(add(t0, mul(blade_w, -1)))], "#a8b0b6", two=True, layer=layer, bias=bias - 0.2)
            g = add(start, mul(d, -2.0))
            sc.line(W(add(g, mul(side, -3.5))), W(add(g, mul(side, 3.5))), "#6a4a2c", 2.0, layer=layer, bias=bias - 0.2)
    elif kind == "sabre":
        tip = add(grip, mul(d, 30.0))
        sc.line(W(grip), W(tip), "#d7dde0", 2.0, layer=layer, bias=bias - 0.3)
        sc.line(W(grip), W(tip), INK, 0.5, op=0.6, layer=layer, bias=bias - 0.35)
        side = unit((d[1], -d[0], 0.0)) if abs(d[2]) < 0.95 else (0.0, 1.0, 0.0)
        sc.line(W(add(grip, mul(side, -3.0))), W(add(grip, mul(side, 3.0))), BRASS, 2.0, layer=layer, bias=bias - 0.3)


# ---- what a soldier does ------------------------------------------------------------------------
def _rest_rifle(p, breathe=0.0):
    """Order arms: the rifle stands by the right foot, the right hand at the barrel."""
    p.hands[1] = (2.5, -14.0, 54.0 + breathe)
    p.tool = ((2.0, -14.5, 52.0 + breathe), (0.03, 0.0, 1.0))
    p.hands[0] = None


def soldier_pose(clip, t, kit=SOLDIER, phase=0.0):
    p = Pose()
    if clip == "idle":
        a = math.tau * (t + phase)
        breathe = 0.6 * math.sin(a)
        p.hip = (0.0, 0.8 * math.sin(a), 42.0 + 0.4 * math.sin(2 * a))
        p.head = (14.0 * math.sin(a + 0.6), 2.0)
        p.lean = 1.0 + 1.0 * math.sin(a)
        if kit.tool == "rifle":
            _rest_rifle(p, breathe)
        else:
            _shoulder_tool(p, kit)
    elif clip == "run":
        a = math.tau * (t + phase)
        for i, off in enumerate((0.0, math.pi)):
            s = 1 if i == 0 else -1
            x = 10.0 * math.cos(a + off)
            z = 5.0 * max(0.0, -math.sin(a + off))
            p.feet[i] = (x, 5.0 * s, z)
        p.hip = (1.0, 0.0, 41.0 + 1.3 * math.cos(2 * a))
        p.lean = 7.0
        p.head = (0.0, 3.0)
        # the free arm swings against its leg
        p.hands[0] = (2.0 - 9.0 * math.cos(a), 13.0, 47.0 + 2.0 * abs(math.sin(a)))
        if kit.tool == "rifle":
            # shoulder arms: the right hand under the butt, the barrel up against the shoulder
            p.hands[1] = (6.0, -10.5, 58.0)
            p.tool = ((6.0, -10.0, 60.0), (-0.42, 0.08, 0.90))
        else:
            _shoulder_tool(p, kit)
    elif clip == "attack":
        if kit.tool == "rifle":
            _fire(p, t)
        else:
            _swing(p, t, kit)
    elif clip == "victory":
        k = keys(t, [(0.0, 0.0), (0.3, 1.0), (0.75, 1.0), (1.0, 0.0)])
        hop = 4.0 * math.sin(math.pi * span(t, 0.3, 0.6))
        p.hip = (0.0, 0.0, 42.0 + hop - 1.5 * k)
        p.feet = [(0.5, 5.5, hop * 0.7), (0.5, -5.5, hop * 0.7)]
        p.head = (0.0, -12.0 * k)
        p.lean = -5.0 * k
        if kit.tool == "rifle":
            rest_r = (2.5, -14.0, 54.0)
            up_r = (3.0, -8.0, 92.0 + hop)
            up_l = (3.0, 8.0, 92.0 + hop)
            p.hands[1] = mix(rest_r, up_r, k)
            p.hands[0] = mix((1.0, 15.5, 47.0), up_l, k) if k > 0.02 else None
            grip = mix((2.0, -14.5, 52.0), (3.0, -8.0, 92.0 + hop), k)
            d = unit(mix((0.03, 0.0, 1.0), (0.05, 1.0, 0.25), k))
            p.tool = (grip, d)
        else:
            up_r = (4.0, -8.0, 92.0 + hop)
            p.hands[1] = mix((7.0, -13.0, 64.0), up_r, k)
            p.hands[0] = mix((1.0, 15.5, 47.0), (4.0, 13.0, 86.0 + hop), k) if k > 0.02 else None
            d = unit(mix((-0.55, -0.15, 0.82), (0.2, 0.0, 1.0), k))
            p.tool = (p.hands[1], d)
    elif clip == "death":
        _die(p, t, kit)
    return p


def _shoulder_tool(p, kit):
    """A pick or spade on the right shoulder, as the labourers carry it."""
    p.hands[1] = (7.0, -13.0, 64.0)
    p.tool = ((7.0, -13.0, 64.0), (-0.55, -0.15, 0.82))


def _fire(p, t):
    """Present, aim, fire at FIRE_AT, recover, order arms."""
    raise_ = keys(t, [(0.0, 0.0), (0.2, 1.0), (0.62, 1.0), (0.9, 0.0)])
    kick = 0.0
    if t >= FIRE_AT:
        x = span(t, FIRE_AT, FIRE_AT + 0.22)
        kick = 6.75 * x * (1 - x) * (1 - x)
    step = keys(t, [(0.0, 0.0), (0.18, 1.0), (0.7, 1.0), (0.95, 0.0)])
    p.feet = [(0.5 + 9.0 * step, 5.5 + 1.0 * step, 0.0), (0.5 - 3.0 * step, -5.5, 0.0)]
    p.hip = (1.0 * step - 1.6 * kick, 0.0, 42.0 - 2.0 * step)
    p.lean = 4.0 * step - 6.0 * kick
    p.twist = -18.0 * raise_
    p.head = (14.0 * raise_, 4.0 * raise_)
    # rifle at the shoulder, pointing forward, muzzle climbing with the recoil
    aim_d = unit((1.0, 0.04, 0.05 + 0.32 * kick))
    aim_grip = (4.0 - 3.0 * kick, -7.5, 69.0 + 1.0 * kick)
    rest_grip, rest_d = (2.0, -14.5, 52.0), (0.03, 0.0, 1.0)
    grip = mix(rest_grip, aim_grip, raise_)
    d = unit(mix(rest_d, aim_d, raise_))
    p.tool = (grip, d)
    p.hands[1] = add(grip, mul(d, 1.0))
    p.hands[0] = add(grip, mul(d, 22.0)) if raise_ > 0.25 else None
    if t >= FIRE_AT:
        x = span(t, FIRE_AT, 1.0)
        muzzle = add(aim_grip, mul(aim_d, 62.0 + 8.0 * x))
        p.smoke = (add(muzzle, (2.0 * x, 0.0, 6.0 * x)), 5.0 + 11.0 * x, 0.85 * (1.0 - x) ** 1.2)


def _swing(p, t, kit):
    """A labourer's blow: the tool goes up over the head and comes down in front at the strike."""
    up = keys(t, [(0.0, 0.0), (0.3, 1.0), (STRIKE_AT, -0.4), (0.7, -0.4), (1.0, 0.0)])
    p.lean = 10.0 * max(0.0, -up) - 4.0 * max(0.0, up)
    p.feet = [(6.0, 5.5, 0.0), (-2.0, -5.5, 0.0)]
    rest = (7.0, -13.0, 64.0)
    high = (-2.0, -4.0, 94.0)
    low = (16.0, -3.0, 48.0)
    if up >= 0:
        hand = mix(rest, high, up)
        d = unit(mix((-0.55, -0.15, 0.82), (-0.6, 0.0, 0.8), up))
    else:
        hand = mix(rest, low, -up / 0.4)
        d = unit(mix((-0.55, -0.15, 0.82), (0.85, 0.0, -0.5), -up / 0.4))
    p.hands[1] = hand
    p.hands[0] = add(hand, mul(d, -12.0)) if abs(up) > 0.1 else None
    p.tool = (hand, d)


def _die(p, t, kit):
    """Struck: a jolt back, the knees go, a fall backward, still on the ground."""
    jolt = keys(t, [(0.0, 0.0), (0.12, 1.0), (0.3, 0.6)])
    buckle = keys(t, [(0.12, 0.0), (0.45, 1.0)])
    fall = keys(t, [(0.4, 0.0), (0.82, 88.0), (0.9, 84.0), (1.0, 86.0)])
    p.lean = -16.0 * jolt + 22.0 * buckle * (1 - span(t, 0.45, 0.8))
    p.head = (0.0, -20.0 * jolt)
    p.hip = (-3.0 * buckle, 0.0, 42.0 - 11.0 * buckle)
    p.feet = [(2.0 + 4.0 * buckle, 6.0, 0.0), (0.0 + 3.0 * buckle, -6.5, 0.0)]
    p.fall = fall
    # over the folded knees, so he ends up lying close to where he stood
    p.fall_pivot = (4.0, 0.0, 9.0)
    p.lift = -4.0 * span(t, 0.5, 0.85)
    fling = keys(t, [(0.0, 0.0), (0.15, 1.0)])
    p.hands = [(-4.0 * fling + 1.0, 17.0, 50.0 + 14.0 * fling), (-5.0 * fling + 2.5, -17.0, 50.0 + 14.0 * fling)]
    if kit.tool in ("rifle", "pick", "shovel"):
        # the rifle or tool drops and lies on the ground beside him
        held = (2.0, -14.5, 52.0) if kit.tool == "rifle" else (7.0, -13.0, 64.0)
        held_d = (0.03, 0.0, 1.0) if kit.tool == "rifle" else (-0.55, -0.15, 0.82)
        drop = span(t, 0.1, 0.55)
        e = ease(drop)
        grip = mix(held, (-6.0, -6.0, 2.0 + 14.0 * (1 - e) * (1 - e)), e)
        d = unit(mix(held_d, (0.35, -0.94, 0.0), e))
        p.tool = None
        p.dropped = (grip, d)


def figure(sc, xf, kit, clip, t, phase=0.0, layer=0):
    p = soldier_pose(clip, t, kit, phase)
    person(sc, xf, p, kit, layer)
    dropped = getattr(p, "dropped", None)
    if dropped:
        draw_tool(sc, xf, kit.tool, dropped[0], dropped[1], layer)


def infantry(k, clip, t):
    # two riflemen: one ahead on the right, one behind on the left
    lag = 0.04 if clip in ("attack", "death", "victory") else 0.0
    sc = Scene(U, U, ORIGIN, SCALE)
    xf = facing_frame(k)
    slots = [((-8.0, 13.0), 0.5, lag), ((8.0, -13.0), 0.0, 0.0)]
    for (sx, sy), _, _ in slots:
        ground_shadow(sc, xf((sx, sy, 0.0)), 16, 5.0)
    for (sx, sy), phase, delay in slots:
        tt = t if CLIPS[clip][2] else max(0.0, t - delay)
        figure(sc, xf.at((sx, sy, 0.0)), SOLDIER, clip, tt, phase)
    return sc.render()


def pioneer(k, clip, t):
    lag = 0.05 if clip in ("attack", "death", "victory") else 0.0
    sc = Scene(U, U, ORIGIN, SCALE)
    xf = facing_frame(k)
    slots = [((-8.0, 13.0), 0.45, lag), ((8.0, -13.0), 0.0, 0.0)]
    for (sx, sy), _, _ in slots:
        ground_shadow(sc, xf((sx, sy, 0.0)), 15, 5.0)
    for (sx, sy), phase, delay in slots:
        tt = t if CLIPS[clip][2] else max(0.0, t - delay)
        figure(sc, xf.at((sx, sy, 0.0)), PIONEER, clip, tt, phase)
    return sc.render()


def worker(k, clip, t):
    sc = Scene(U, U, ORIGIN, SCALE * 1.06)
    xf = facing_frame(k)
    ground_shadow(sc, xf((0.0, 0.0, 0.0)), 16, 5.0)
    figure(sc, xf, WORKER, clip, t)
    return sc.render()


# ---- horse and rider ----------------------------------------------------------------------------
COAT = "#7a4524"
MANE = "#2a1a10"
HOOF = "#1a140f"


class Steed:
    def __init__(self):
        self.pitch = 0.0  # positive tips the nose down
        self.pivot = (0.0, 0.0, 0.0)  # what the pitch turns about
        self.roll = 0.0  # positive falls onto its right side
        self.drop = 0.0
        self.bob = 0.0
        self.legs = {}  # name -> (hip swing degrees, knee fold degrees)
        self.neck = 0.0  # neck raise, degrees
        self.tail = 0.0


LEGS = {"fl": (30.0, 6.5, True), "fr": (30.0, -6.5, True), "hl": (-27.0, 6.5, False), "hr": (-27.0, -6.5, False)}


def steed_xf(xf, h: Steed):
    w = xf
    if h.roll:
        w = w.at((0.0, -9.0, 0.0)).roll(h.roll).at((0.0, 9.0, 0.0))
    if h.pitch:
        w = w.at(h.pivot).pitch(h.pitch).at(mul(h.pivot, -1))
    return w.at((0.0, 0.0, h.bob - h.drop))


def horse(sc, xf, h: Steed, layer=0):
    W = steed_xf(xf, h)
    dark = shade(COAT, 0.62)
    # legs: far side first is handled by depth sorting
    for name, (x, y, front) in LEGS.items():
        swing, fold = h.legs.get(name, (0.0, 0.0))
        top = (x, y, 47.0)
        a = math.radians(swing)
        knee = add(top, (math.sin(a) * 22.0, 0.0, -math.cos(a) * 22.0))
        b = math.radians(swing - fold if front else swing + fold)
        hoof = add(knee, (math.sin(b) * 21.0, 0.0, -math.cos(b) * 21.0))
        col = COAT
        sc.limb(W(top), W(knee), 5.6, 3.6, col, layer=layer)
        sc.limb(W(knee), W(hoof), 3.4, 2.8, col, layer=layer)
        sc.limb(W(add(hoof, (0.0, 0.0, 2.6))), W(hoof), 3.4, 3.6, HOOF, layer=layer, bias=-0.1)
        sc.ball(W(add(hoof, (0.0, 0.0, 4.6))), 2.6, "#e6ddc6", sw=0.4, squash=0.7, layer=layer, bias=-0.15)
    # body: barrel, chest, haunch
    sc.tube(W((-30.0, 0.0, 56.0)), W((30.0, 0.0, 57.0)), 11.0, 11.5, COAT, side=W.vec((0, 1, 0)), squash=1.4, sides=16, caps=False, layer=layer)
    sc.ball(W((-31.0, 0.0, 58.0)), 14.5, COAT, squash=1.05, layer=layer, bias=0.2)
    sc.ball(W((29.0, 0.0, 58.0)), 14.0, COAT, squash=1.1, layer=layer, bias=0.2)
    # tail
    sw_ = h.tail
    t0 = W((-43.0, 0.0, 64.0))
    t1 = W((-52.0, 4.0 * sw_, 46.0))
    t2 = W((-50.0, 6.0 * sw_, 28.0))
    sc.limb(t0, t1, 3.6, 4.4, MANE, layer=layer, bias=0.3)
    sc.limb(t1, t2, 4.4, 2.0, MANE, layer=layer, bias=0.3)
    # neck and head
    ra = math.radians(h.neck)
    base = (33.0, 0.0, 64.0)
    top = add(base, (math.cos(math.radians(58) + ra) * 26.0, 0.0, math.sin(math.radians(58) + ra) * 26.0))
    nose = add(top, (math.cos(math.radians(-38) + ra) * 17.0, 0.0, math.sin(math.radians(-38) + ra) * 17.0))
    sc.tube(W(base), W(top), 8.5, 5.4, COAT, side=W.vec((0, 1, 0)), squash=1.25, sides=12, layer=layer, bias=-0.1)
    sc.tube(W(top), W(nose), 5.6, 3.8, COAT, side=W.vec((0, 1, 0)), squash=1.15, sides=12, layer=layer, bias=-0.2)
    sc.ball(W(nose), 3.9, shade(COAT, 0.75), layer=layer, bias=-0.25)
    mane_a = add(base, (-3.0, 0.0, 9.0))
    sc.limb(W(mane_a), W(add(top, (-2.0, 0.0, 4.0))), 2.6, 2.0, MANE, layer=layer, bias=-0.15)
    for e in (1, -1):
        ear = add(top, (0.0, 2.4 * e, 5.0))
        sc.limb(W(ear), W(add(ear, (-1.0, 0.6 * e, 4.5))), 1.4, 0.6, shade(COAT, 0.8), layer=layer, bias=-0.3)
        eye = add(add(top, mul(unit(sub(nose, top)), 4.0)), (0.0, 4.6 * e, 1.4))
        sc.draw(W(eye), lambda svg, x, y: svg.circle(x, y, 1.0, "#0f0b08"), layer=layer, bias=-0.35)
    # blaze
    sc.line(W(add(top, (1.5, 0, 3.0))), W(add(nose, (1.5, 0, 2.5))), "#e8dcc0", 1.2, op=0.7, layer=layer, bias=-0.4)
    # saddle blanket and saddle
    for e in (1, -1):
        y = 11.2 * e
        sc.poly([W((-12.0, y, 70.0)), W((12.0, y, 70.0)), W((12.5, y * 1.02, 52.0)), W((-12.5, y * 1.02, 52.0))], "#7a2f28", INK, 0.7, layer=layer, bias=-0.3)
        sc.poly([W((-12.5, y * 1.03, 54.5)), W((12.5, y * 1.03, 54.5)), W((12.5, y * 1.03, 52.0)), W((-12.5, y * 1.03, 52.0))], BRASS, None, layer=layer, bias=-0.35)
    sc.box(W.at((0.0, 0.0, 69.0)), -12.0, 12.0, -10.0, 10.0, 0.0, 3.5, "#2a1d14", layer=layer, bias=-0.3)
    return W


def steed_pose(clip, t):
    h = Steed()
    if clip == "idle":
        a = math.tau * t
        h.neck = 4.0 * math.sin(a) - 2.0
        h.tail = math.sin(2 * a)
        h.legs = {"fl": (2.0, 0.0), "fr": (-2.0, 0.0), "hl": (-3.0, 0.0), "hr": (2.0, 6.0 * max(0.0, math.sin(a)))}
        h.bob = 0.5 * math.sin(2 * a)
    elif clip == "run":
        a = math.tau * t
        # a canter: the legs swing in a rolling order, the body rocks
        offs = {"hl": 0.0, "hr": 0.12, "fl": 0.42, "fr": 0.56}
        for name, o in offs.items():
            ph = a - math.tau * o
            swing = 28.0 * math.cos(ph)
            fold = 55.0 * max(0.0, math.sin(ph))
            h.legs[name] = (swing, fold)
        h.pitch = 4.0 * math.sin(a + 0.6)
        h.pivot = (0.0, 0.0, 50.0)
        h.bob = 3.0 * math.cos(2 * a)
        h.neck = -6.0 + 6.0 * math.sin(a + 1.0)
        h.tail = 0.6 * math.sin(a)
    elif clip == "attack":
        # rear back a little on the wind-up, lunge into the blow at STRIKE_AT, recover
        rear = keys(t, [(0.0, 0.0), (0.28, 1.0), (STRIKE_AT, -0.5), (0.75, 0.0), (1.0, 0.0)])
        h.pitch = -12.0 * rear
        h.pivot = (-27.0, 0.0, 0.0)
        a = math.tau * t * 2
        for name, o in {"hl": 0.0, "hr": 0.12, "fl": 0.42, "fr": 0.56}.items():
            ph = a - math.tau * o
            g = 0.6 * (1 - span(t, 0.75, 1.0))
            h.legs[name] = (24.0 * math.cos(ph) * g, 45.0 * max(0.0, math.sin(ph)) * g)
        if rear > 0:
            h.legs["fl"] = (h.legs["fl"][0] + 25 * rear, 60 * rear)
            h.legs["fr"] = (h.legs["fr"][0] + 15 * rear, 70 * rear)
        h.neck = 10.0 * max(0.0, rear) - 8.0 * max(0.0, -rear)
        h.tail = math.sin(a)
    elif clip == "victory":
        rear = keys(t, [(0.0, 0.0), (0.35, 1.0), (0.65, 1.0), (1.0, 0.0)])
        h.pitch = -30.0 * rear
        h.pivot = (-27.0, 0.0, 0.0)
        kick = math.sin(math.tau * 2 * t) * rear
        h.legs = {"fl": (30 * rear + 12 * kick, 80 * rear), "fr": (20 * rear - 12 * kick, 90 * rear), "hl": (8 * rear, 0.0), "hr": (4 * rear, 0.0)}
        h.neck = 18.0 * rear
        h.tail = 0.8 * math.sin(math.tau * t)
    elif clip == "death":
        fold = keys(t, [(0.05, 0.0), (0.4, 1.0)])
        h.legs = {n: (10.0 * fold if n[0] == "f" else -10.0 * fold, 95.0 * fold if n[0] == "f" else 70.0 * fold) for n in LEGS}
        h.drop = 22.0 * ease(span(t, 0.1, 0.5))
        h.roll = keys(t, [(0.35, 0.0), (0.8, 82.0), (0.9, 78.0), (1.0, 80.0)])
        h.neck = keys(t, [(0.0, 0.0), (0.15, 20.0), (0.6, -30.0)])
        h.tail = 0.0
        h.pitch = keys(t, [(0.0, 0.0), (0.12, -6.0), (0.4, 4.0)])
        h.pivot = (0.0, 0.0, 0.0)
    return h


def rider_pose(clip, t, steed: Steed):
    p = Pose()
    p.seated = True
    p.hip = (0.0, 0.0, 73.0)
    p.feet = [(8.0, 11.5, 44.0), (8.0, -11.5, 44.0)]
    p.hands[0] = (16.0, 4.0, 82.0)  # reins
    p.lean = 4.0
    rest_grip, rest_d = (12.0, -10.0, 84.0), unit((-0.15, -0.1, 1.0))  # sabre at the carry
    if clip == "idle":
        a = math.tau * t
        p.head = (16.0 * math.sin(a), 0.0)
        p.lean = 3.0 + 1.0 * math.sin(a)
        grip, d = rest_grip, rest_d
    elif clip == "run":
        a = math.tau * t
        p.lean = 14.0
        p.hip = (0.0, 0.0, 73.0 + 1.5 * math.sin(2 * a))
        p.hands[0] = (20.0, 4.0, 80.0)
        grip, d = (16.0, -10.0, 88.0), unit((0.75, -0.1, 0.65))
    elif clip == "attack":
        up = keys(t, [(0.0, 0.0), (0.3, 1.0), (STRIKE_AT, -1.0), (0.72, -1.0), (1.0, 0.0)])
        p.lean = 6.0 + 18.0 * max(0.0, -up)
        p.twist = -20.0 * max(0.0, up) + 18.0 * max(0.0, -up)
        if up >= 0:
            grip = mix(rest_grip, (2.0, -12.0, 108.0), up)
            d = unit(mix(rest_d, (-0.7, -0.2, 0.6), up))
        else:
            grip = mix(rest_grip, (28.0, -14.0, 70.0), -up)
            d = unit(mix(rest_d, (0.9, -0.2, -0.4), -up))
    elif clip == "victory":
        k = keys(t, [(0.0, 0.0), (0.3, 1.0), (0.7, 1.0), (1.0, 0.0)])
        p.lean = 4.0 - 18.0 * k
        grip = mix(rest_grip, (8.0, -9.0, 116.0), k)
        d = unit(mix(rest_d, (0.25, 0.0, 1.0), k))
        p.head = (0.0, -15.0 * k)
    else:  # death: thrown off the far side as the horse goes over
        p.lean = keys(t, [(0.0, 4.0), (0.12, -20.0), (0.5, 10.0)])
        p.side = keys(t, [(0.3, 0.0), (0.8, -30.0)])
        p.head = (0.0, -20.0 * span(t, 0.0, 0.2))
        grip, d = mix(rest_grip, (6.0, -24.0, 60.0), span(t, 0.1, 0.5)), unit(mix(rest_d, (0.6, -0.8, -0.3), span(t, 0.1, 0.5)))
        p.hands[0] = mix((16.0, 4.0, 82.0), (0.0, 20.0, 100.0), span(t, 0.05, 0.2))
    p.hands[1] = grip
    p.tool = (grip, d)
    return p


def cavalry(k, clip, t):
    sc = Scene(U, U, ORIGIN, SCALE * 0.84)
    xf = facing_frame(k).at((-2.0, 0.0, 0.0))
    h = steed_pose(clip, t)
    shadow_op = 0.5 * (1.0 - 0.5 * span(t, 0.5, 1.0)) if clip == "death" else 0.5
    oval_shadow(sc, xf, 46, 14, op=shadow_op)
    W = horse(sc, xf, h)
    rp = rider_pose(clip, t, h)
    person(sc, W, rp, TROOPER)
    return sc.render()


# ---- field gun ----------------------------------------------------------------------------------
BRONZE = "#8d6a30"


def wheel(sc, W, c, spin, k, layer=0, r=17.0):
    """A spoked wheel in the gun's x-z plane at `c`, turned by `spin` degrees."""
    r *= k
    n = W.vec((0.0, 1.0, 0.0))
    out = 0.6 * (1 if c[1] > 0 else -1)
    sc.disk(W(c), n, r + 2.2 * k, "#1c1b19", sides=24, layer=layer)
    sc.disk(W(add(c, (0.0, out * 0.7, 0.0))), n, r * 0.92, "#5a3f24", sides=24, layer=layer, bias=-0.05)
    for i in range(10):
        a = math.radians(spin + i * 36.0)
        e = add(c, (math.cos(a) * r * 0.9, out, math.sin(a) * r * 0.9))
        sc.line(W(add(c, (0, out, 0))), W(e), "#2e2013", 1.7, layer=layer, bias=-0.1)
        sc.line(W(add(c, (0, out, 0))), W(e), "#b78e55", 0.6, op=0.6, layer=layer, bias=-0.12)
    sc.ball(W(add(c, (0.0, out * 2, 0.0))), 3.6 * k, "#2b2927", layer=layer, bias=-0.15)


def gun(sc, xf, recoil=0.0, elev=8.0, spin=0.0, tilt=0.0, drop=0.0, layer=0, smoke_at=None, k=1.3):
    """The field gun: wheels on an axle at the origin, the trail running back to the ground, the barrel forward.
    Authored at the old sprite's size and drawn `k` times larger, so the piece reads beside its gunner."""
    W = xf
    if tilt:
        W = W.at((0.0, -12.0 * k, 0.0)).roll(tilt).at((0.0, 12.0 * k, 0.0))
    W = W.at((-recoil * 0.5, 0.0, -drop))
    G = lambda p: mul(p, k)
    wood = "#7c5530"
    # trail: two cheeks from the axle back to the spade on the ground
    for e in (1, -1):
        sc.limb(W(G((4.0, 5.0 * e, 24.0))), W(G((-57.0, 2.2 * e, 2.5))), 3.6 * k, 2.6 * k, wood, layer=layer)
    sc.box(W.at(G((-58.0, 0.0, 0.0))), -3.0 * k, 3.0 * k, -4.0 * k, 4.0 * k, 0.0, 4.5 * k, "#241c14", layer=layer)
    sc.limb(W(G((0.0, 13.0, 17.0))), W(G((0.0, -13.0, 17.0))), 2.2 * k, 2.2 * k, "#2c2a28", layer=layer)
    # barrel: breech, tube and muzzle swell, slid back by the recoil
    e = math.radians(elev)
    d = (math.cos(e), 0.0, math.sin(e))
    breech = G((-14.0 - recoil, 0.0, 30.0))
    muzzle = add(breech, mul(d, 60.0 * k))
    sc.tube(W(breech), W(muzzle), 5.8 * k, 3.8 * k, BRONZE, sides=14, layer=layer, bias=-0.2)
    sc.tube(W(add(breech, mul(d, 52.0 * k))), W(add(breech, mul(d, 60.5 * k))), 4.8 * k, 4.8 * k, shade(BRONZE, 0.95), sides=14, layer=layer, bias=-0.25)
    sc.ball(W(add(breech, mul(d, -1.5 * k))), 5.4 * k, shade(BRONZE, 0.9), layer=layer, bias=-0.2)
    for x in (16.0, 30.0):
        sc.tube(W(add(breech, mul(d, x * k))), W(add(breech, mul(d, (x + 2.5) * k))), (5.6 - x * 0.04) * k, (5.5 - x * 0.04) * k, shade(BRONZE, 0.7), sides=14, caps=False, layer=layer, bias=-0.25)
    for e2 in (1, -1):
        wheel(sc, W, G((0.0, 13.5 * e2, 17.0)), spin, k, layer)
    if smoke_at is not None:
        x, r, op = smoke_at
        tip = W(add(muzzle, mul(d, (6.0 + x * 14.0) * k)))
        sc.draw(tip, lambda svg, px, py: (smoke(svg, px, py - 6 * x, r * SCALE, op), smoke(svg, px - 9, py - 2, r * 0.7 * SCALE, op * 0.8, seed=3)), layer=layer + 1)
    return W


def artillery(k, clip, t):
    sc = Scene(U, U, ORIGIN, SCALE * 0.84)
    xf = facing_frame(k).at((10.0, 0.0, 0.0))
    oval_shadow(sc, xf.at((-26.0, 0.0, 0.0)), 52, 18, op=0.45)
    crew_at = (-30.0, 30.0, 0.0)
    if clip == "idle":
        gun(sc, xf)
        figure(sc, xf.at(crew_at).yaw(-20), GUNNER, "idle", t)
    elif clip == "run":
        gun(sc, xf, spin=-72.0 * t)  # two spokes a stride, so the loop joins
        figure(sc, xf.at(crew_at), GUNNER, "run", t)
    elif clip == "attack":
        r = 0.0
        if t >= FIRE_AT:
            x = span(t, FIRE_AT, FIRE_AT + 0.25)
            r = 10.0 * (6.75 * x * (1 - x) * (1 - x)) if t < FIRE_AT + 0.25 else 0.0
            back = keys(t, [(FIRE_AT, 0.0), (FIRE_AT + 0.06, 8.0), (1.0, 0.0)])
        else:
            back = 0.0
        sm = None
        if t >= FIRE_AT:
            x = span(t, FIRE_AT, 1.0)
            sm = (x, 7.0 + 16.0 * x, 0.95 * (1 - x) ** 1.1)
        gun(sc, xf.at((-back, 0.0, 0.0)), recoil=r, spin=back * 6.0, smoke_at=sm)
        # the gunner jerks the lanyard at the shot
        p = soldier_pose("idle", 0.0, GUNNER)
        pull = keys(t, [(0.0, 0.0), (0.2, 0.6), (FIRE_AT, 1.0), (0.6, 1.0), (1.0, 0.0)])
        p.hands[1] = mix((2.0, -13.0, 47.0), (-10.0, -12.0, 52.0), pull)
        p.lean = -6.0 * pull
        p.head = (25.0 * pull, 0.0)
        person(sc, xf.at(crew_at).yaw(-20), p, GUNNER)
    elif clip == "victory":
        gun(sc, xf)
        figure(sc, xf.at(crew_at).yaw(-30), GUNNER, "victory", t)
    else:
        tilt = keys(t, [(0.1, 0.0), (0.5, 26.0), (0.6, 22.0), (1.0, 24.0)])
        drop = keys(t, [(0.1, 0.0), (0.5, 5.0)])
        sm = (span(t, 0.0, 1.0), 9.0 + 10.0 * t, 0.6 * (1 - t))
        gun(sc, xf, tilt=tilt, drop=drop, elev=8.0 - 10 * span(t, 0.1, 0.5))
        figure(sc, xf.at(crew_at).yaw(-20), GUNNER, "death", t)
    return sc.render()


BUILDERS = {
    "infantry": infantry,
    "pioneer": pioneer,
    "worker": worker,
    "cavalry": cavalry,
    "artillery": artillery,
}
