"""Unit models for `figures.Scene`, one builder per unit family.

`frame(...)` is the single entry point used by the asset generator: it builds
the model for a unit name/kind/weapon, poses it for an animation slot and
frame, rotates it to one of the 8 facings and returns a palette image.
"""
import hashlib
import math

from PIL import Image

from . import figures as F

SKINS = [(242, 206, 168), (222, 172, 126), (178, 124, 86), (126, 86, 62)]
CLOTH = (206, 206, 210)               # base of team-coloured parts (luminance only)
STEEL = (170, 178, 190)
IRON = (118, 124, 136)
BRONZE = (196, 142, 62)
GOLD = (236, 196, 70)
LEATHER = (136, 92, 56)
WOOD = (150, 104, 60)
DARKWOOD = (100, 68, 42)
CANVAS = (236, 226, 196)
OLIVE = (98, 112, 70)
GREYSTEEL = (140, 150, 162)
BLACK = (52, 50, 58)
FIRE = (255, 186, 60)
HORSES = [(150, 98, 58), (110, 70, 44), (196, 160, 112), (70, 56, 50), (226, 220, 210), (168, 128, 78)]
TROUSERS = [(78, 66, 96), (104, 78, 56), (70, 86, 70), (96, 96, 108), (118, 62, 54)]


def _h(name, salt=""):
    return int.from_bytes(hashlib.sha256((name + salt).encode()).digest()[:4], "big")


class Look:
    def __init__(self, name):
        self.skin = SKINS[_h(name, "skin") % len(SKINS)]
        self.trousers = TROUSERS[_h(name, "trs") % len(TROUSERS)]
        self.horse = HORSES[_h(name, "horse") % len(HORSES)]
        hue = (_h(name, "acc") % 360) / 360.0
        import colorsys
        r, g, b = colorsys.hsv_to_rgb(hue, 0.65, 0.9)
        self.accent = (int(r * 255), int(g * 255), int(b * 255))
        self.low = name.lower()

    def has(self, *words):
        return any(w in self.low for w in words)


# ---------------------------------------------------------------- poses

def pose(slot, i, fpd):
    p = {"stride": 0.0, "arm": 0, "bob": 0.0, "crouch": 0.0, "flash": False, "guard": False,
         "flag": False, "pitch": 0.0, "work": None, "up": False, "phase": i}
    last = max(0, fpd - 1)
    if slot == "RUN":
        p["stride"] = 1.0 if i % 2 == 0 else -1.0
        p["bob"] = 0.0 if i % 2 else 0.8
    elif slot.startswith("ATTACK"):
        if slot == "ATTACK1":
            p["arm"] = 1 if i == 0 else 2
        else:
            p["arm"] = 2
        p["flash"] = (i == last) and slot != "ATTACK2"
        p["stride"] = 0.4
    elif slot == "DEATH":
        p["pitch"] = (0.9, 1.5)[min(i, 1)]
        p["arm"] = -1
    elif slot == "VICTORY":
        p["up"] = True
        p["bob"] = 1.0
    elif slot == "FORTIFY":
        p["crouch"] = 1.0 + 0.6 * i
        p["guard"] = True
    elif slot == "DEFEND":
        p["guard"] = True
        p["crouch"] = 0.6
    elif slot == "CAPTURE":
        p["flag"] = True
    elif slot == "FIDGET":
        p["bob"] = -0.6 if i == 0 else 0.0
    elif slot in ("BUILD", "ROAD", "MINE", "IRRIGATE", "FORTRESS", "JUNGLE", "FOREST", "PLANT"):
        p["work"] = slot
        p["arm"] = 2
    return p


# --------------------------------------------------------------- people

def _legs(sc, look, p, hipz, trousers, boots=DARKWOOD, rider=False):
    s = p["stride"]
    for side in (-1, 1):
        phase = s * side
        foot_f = 3.2 * phase
        foot_z = 1.2 + max(0.0, phase) * 1.6
        knee_f = 1.2 + 1.6 * phase + p["crouch"] * 1.6
        knee_z = hipz * 0.5 + 0.6 + (0.6 if phase > 0 else 0.0)
        hip = (side * 2.3, 0, hipz)
        knee = (side * 2.6, knee_f, knee_z)
        foot = (side * 2.5, foot_f, foot_z)
        sc.limb(hip, knee, 2.1, 1.8, trousers)
        sc.limb(knee, foot, 1.8, 1.6, trousers)
        sc.ball((side * 2.5, foot_f + 1.0, foot_z - 0.4), (1.8, 2.8, 1.5), boots)


def _head(sc, look, p, z, headgear):
    c = (0, 0.4, z)
    sc.ball(c, (3.3, 3.3, 3.5), look.skin)
    if headgear == "helm":
        sc.ball((0, 0.2, z + 1.5), (3.8, 3.8, 3.2), STEEL)
        sc.ball((0, -0.2, z + 3.6), (1.2, 2.2, 1.6), look.accent)          # crest
    elif headgear == "bronze":
        sc.ball((0, 0.2, z + 1.4), (3.7, 3.7, 3.0), BRONZE)
        sc.limb((0, -2.0, z + 3.6), (0, -3.5, z + 0.5), 1.2, 0.9, look.accent)
    elif headgear == "hood":
        sc.ball((0, -0.3, z + 1.0), (3.8, 3.8, 3.6), (84, 120, 70))
    elif headgear == "hat":                                                  # tricorn / brim
        sc.disc((0, 0.3, z + 2.6), 5.2, (0, 0, 1), BLACK, thick=0.8)
        sc.ball((0, 0.2, z + 3.4), (3.0, 3.0, 2.0), BLACK)
        sc.ball((0, 0.0, z + 3.2), (3.1, 3.1, 0.7), (230, 230, 230))
    elif headgear == "steel":                                                # modern helmet
        sc.ball((0, 0.2, z + 1.5), (3.9, 3.9, 2.9), OLIVE)
        sc.ball((0, 1.8, z + 3.0), (2.6, 1.5, 0.8), F.tone(OLIVE, 1.15))
    elif headgear == "straw":
        sc.disc((0, 0.3, z + 2.3), 6.0, (0, 0, 1), (230, 200, 120), thick=0.5)
        sc.ball((0, 0.2, z + 3.2), (3.0, 3.0, 1.8), (222, 190, 108))
    elif headgear == "band":
        sc.ball((0, -0.4, z + 1.2), (3.6, 3.7, 3.1), (66, 46, 34))           # hair
        sc.limb((-3.2, 0.5, z + 1.2), (3.2, 0.5, z + 1.2), 0.8, 0.8, look.accent)
    elif headgear == "plume":
        sc.ball((0, 0.2, z + 1.6), (3.8, 3.8, 3.2), GOLD)
        sc.limb((0, -0.5, z + 4.5), (0, -3.0, z + 7.5), 1.0, 0.4, look.accent)
    # eyes: two dark pixels' worth on the face side
    sc.ball((-1.2, 2.9, z + 0.2), (0.45, 0.45, 0.55), BLACK, bias=0.4)
    sc.ball((1.2, 2.9, z + 0.2), (0.45, 0.45, 0.55), BLACK, bias=0.4)


def _arm(sc, side, shoulder, elbow, hand, team=True, sleeve=CLOTH, skin=None):
    sc.limb(shoulder, elbow, 1.8, 1.6, sleeve, team=team)
    sc.limb(elbow, hand, 1.5, 1.3, sleeve if team else skin, team=team)
    sc.ball(hand, (1.4, 1.4, 1.4), skin or SKINS[0])


def _weapon_sword(sc, look, hand, p):
    r, f, z = hand
    a = p["arm"]
    if a == 1:
        tip = (r, f - 5, z + 14)
    elif a == 2:
        tip = (r, f + 12, z + 4)
    elif a == -1:
        tip = (r, f + 4, z - 4)
    else:
        tip = (r + 0.5, f + 3, z + 14)
    sc.rod(hand, tip, 1.5, STEEL)
    guard_a = (hand[0] - 2, hand[1], hand[2] + 0.5)
    guard_b = (hand[0] + 2, hand[1], hand[2] + 0.5)
    sc.rod(guard_a, guard_b, 1.2, GOLD)
    return tip


def _weapon_spear(sc, look, hand, p, length=1.0):
    r, f, z = hand
    a = p["arm"]
    if a == 1:
        a0, tip = (r, f - 8, z - 3), (r, f + 6, z + 15 * length)
    elif a == 2:
        a0, tip = (r, f - 8, z + 1), (r, f + 21, z + 3)
    elif a == -1:
        a0, tip = (r, f - 6, z - 6), (r, f + 14, z - 8)
    else:
        a0, tip = (r + 0.3, f + 0.5, z - 12 * length), (r + 0.5, f + 3, z + 26 * length)
    sc.rod(a0, tip, 1.1, DARKWOOD)
    sc.limb(tip, (tip[0], tip[1] + (tip[1] - a0[1]) * 0.06, tip[2] + (tip[2] - a0[2]) * 0.06 + 3), 1.3, 0.2, STEEL)
    return tip


def _weapon_club(sc, look, hand, p):
    r, f, z = hand
    a = p["arm"]
    tip = {1: (r, f - 3, z + 12), 2: (r, f + 10, z + 3), -1: (r, f + 4, z - 5)}.get(a, (r + 0.5, f + 2, z + 10))
    sc.rod(hand, tip, 1.8, DARKWOOD)
    sc.ball(tip, (2.6, 2.6, 2.6), WOOD)
    return tip


def _weapon_gun(sc, look, hand_r, hand_l, p, modern):
    col = BLACK if modern else DARKWOOD
    a = p["arm"]
    base = (hand_r[0] - 1.5, hand_r[1] - 5, hand_r[2] - 1)
    if a == 2:
        tip = (1.8, hand_r[1] + 13, hand_r[2] + 3)
    elif a == 1:
        tip = (3, hand_r[1] + 9, hand_r[2] + 7)
    else:
        tip = (4, hand_r[1] + 6, hand_r[2] + 11)
    sc.rod(base, tip, 1.5, col)
    sc.rod((base[0], base[1] + 1, base[2]), (tip[0], tip[1], tip[2]), 0.9, IRON, bias=0.1)
    return tip


def _weapon_bow(sc, look, hand, p):
    r, f, z = hand
    s = 9
    top, bot = (r, f + 1.5, z + s), (r, f + 1.5, z - s)
    mid = (r, f + 5.0 + (0 if p["arm"] != 1 else -1), z)
    sc.limb(top, mid, 0.9, 1.1, WOOD)
    sc.limb(bot, mid, 0.9, 1.1, WOOD)
    pull = -3.5 if p["arm"] == 1 else 0
    sc.rod(top, (r, f + 1.0 + pull, z), 0.45, (232, 232, 236))
    sc.rod(bot, (r, f + 1.0 + pull, z), 0.45, (232, 232, 236))
    if p["arm"] == 1:
        sc.rod((r, f + pull, z), (r, f + 11, z), 0.8, BRONZE)
    return mid


def human(sc, look, p, weapon, name, hat=None):
    modern = look.has("infantry", "marine", "paratrooper", "guerrilla", "partisan", "mech", "rifle", "sniper")
    hipz = 13.5 - p["crouch"] * 1.8 + p["bob"] * 0.6
    sc.ground_shadow(8.5, 3.6)
    trousers = OLIVE if modern else look.trousers
    _legs(sc, look, p, hipz, trousers)
    tz = hipz + 6.4
    # tunic skirt and torso (team colour)
    sc.ball((0, 0, hipz + 1.2), (4.7, 3.3, 3.6), CLOTH, team=True)
    sc.ball((0, 0, tz), (4.9, 3.3, 6.0), CLOTH, team=True)
    sc.limb((-4.4, 0.4, hipz + 3.0), (4.4, 0.4, hipz + 3.0), 0.9, 0.9, LEATHER)          # belt
    shz = hipz + 10.8
    up = p["up"]
    # arms
    sk = look.skin
    if up:
        for s in (-1, 1):
            _arm(sc, s, (s * 5, 0, shz), (s * 7.5, 0.5, shz + 4), (s * 8, 0.5, shz + 9), True, skin=sk)
    elif weapon == "gun":
        hr = (3.0, 4.5, hipz + 7.5)
        hl = (-1.5, 8, hipz + 8)
        _arm(sc, 1, (5, 0, shz), (5.8, 3, hipz + 6), hr, True, skin=sk)
        _arm(sc, -1, (-5, 0, shz), (-4, 4.5, hipz + 7), hl, True, skin=sk)
        tip = _weapon_gun(sc, look, hr, hl, p, modern)
        if p["flash"]:
            sc.ball((tip[0], tip[1] + 2.5, tip[2]), (2.4, 2.8, 2.4), FIRE, bias=0.5)
    elif weapon == "bow":
        hl = (-3.0, 5.5, hipz + 8.0)
        hr = (3.5, 4.0 if p["arm"] != 1 else 2.0, hipz + 8.0)
        _arm(sc, -1, (-5, 0, shz), (-5, 3, hipz + 7.5), hl, True, skin=sk)
        _arm(sc, 1, (5, 0, shz), (5.8, 3, hipz + 7.5), hr, True, skin=sk)
        _weapon_bow(sc, look, hl, p)
    else:
        a = p["arm"]
        if a == 1:
            hr = (5.5, 0, shz + 5)
            elbow = (6.5, -1.5, shz + 2)
        elif a == 2:
            hr = (5, 5, hipz + 8)
            elbow = (6.5, 2.5, hipz + 8.5)
        elif a == -1:
            hr = (6, 1, hipz + 2)
            elbow = (6, 0, hipz + 6)
        else:
            hr = (5.8, 2.5, hipz + 6.5)
            elbow = (6.3, 0.5, hipz + 7.5)
        _arm(sc, 1, (5, 0, shz), elbow, hr, True, skin=sk)
        if weapon == "sword":
            _weapon_sword(sc, look, hr, p)
        elif weapon == "spear":
            _weapon_spear(sc, look, hr, p)
        else:
            _weapon_club(sc, look, hr, p)
        guard = p["guard"]
        hl = (-5.5, 3.5 if guard else 1.5, hipz + (8.5 if guard else 6.0))
        _arm(sc, -1, (-5, 0, shz), (-6, 1.5, hipz + 7.5), hl, True, skin=sk)
        if weapon in ("sword", "spear") or guard:
            big = 6.4 if guard else 5.2
            sc.disc((hl[0] - 0.8, hl[1] + 1.0, hl[2] + 0.8), big, (-1, 0.15, 0), CLOTH, team=True,
                    thick=1.2, rim=STEEL if weapon != "club" else WOOD)
            sc.ball((hl[0] - 1.8, hl[1] + 1.0, hl[2] + 0.8), (0.8, 1.6, 1.6), GOLD, bias=0.2)
    # head
    if hat is None:
        if look.has("warrior"):
            hat = "band"
        elif weapon == "sword":
            hat = "helm"
        elif weapon == "spear":
            hat = "bronze"
        elif weapon == "bow":
            hat = "hood"
        elif weapon == "gun":
            hat = "steel" if modern else "hat"
        else:
            hat = "band"
    _head(sc, look, p, hipz + 17.0, hat)


def civilian(sc, look, p, name):
    hipz = 13.0 + p["bob"] * 0.5
    sc.ground_shadow(8.5, 3.6)
    _legs(sc, look, p, hipz, (96, 78, 58))
    tz = hipz + 6.2
    sc.ball((0, 0, hipz + 1.0), (4.6, 3.2, 3.6), CLOTH, team=True)
    sc.ball((0, 0, tz), (4.8, 3.2, 5.8), CLOTH, team=True)
    sc.limb((-4.3, 0.4, hipz + 3.0), (4.3, 0.4, hipz + 3.0), 0.9, 0.9, LEATHER)
    shz = hipz + 10.6
    settler = look.has("settler")
    if settler:
        sc.box((0, -4.2, hipz + 7.5), (3.6, 1.8, 4.6), LEATHER)               # pack
        sc.ball((0, -4.6, hipz + 13.0), (3.8, 1.9, 1.8), (216, 196, 150))       # bedroll
        sc.rod((6.3, 1.5, 0), (6.8, 2.5, hipz + 22), 1.0, DARKWOOD)             # staff
    sk = look.skin
    work = p["work"]
    tool = {"BUILD": "hammer", "ROAD": "shovel", "MINE": "pick", "IRRIGATE": "hoe", "FORTRESS": "shovel",
            "JUNGLE": "axe", "FOREST": "axe", "PLANT": "hoe"}.get(work)
    if not settler and tool is None:
        tool = "shovel"
    hr = (5.6, 2.8, hipz + 6.5)
    elbow = (6.3, 0.6, hipz + 7.6)
    if tool and work:
        hr = (5.0, 5.5, hipz + 9.0)
        elbow = (6.3, 2.4, hipz + 9.0)
    _arm(sc, 1, (5, 0, shz), elbow, hr, True, skin=sk)
    _arm(sc, -1, (-5, 0, shz), (-5.8, 1.5, hipz + 7.5), (-5.5, 3.0, hipz + 6.5), True, skin=sk)
    if tool:
        r, f, z = hr
        if work:
            butt, head = (r, f - 3, z + 11), (r, f + 6, z - 5)
        else:
            butt, head = (r + 0.2, f - 1.5, z - 4), (r + 1.0, f + 1.0, z + 17)
        sc.rod(butt, head, 1.2, DARKWOOD)
        tip = head
        if tool in ("shovel", "hoe"):
            sc.box((tip[0], tip[1] + 0.8, tip[2] - 0.6), (1.6, 1.0, 1.8) if tool == "shovel" else (1.8, 1.8, 0.5), STEEL)
        elif tool == "pick":
            sc.limb((tip[0] - 3.5, tip[1], tip[2] - 0.5), (tip[0] + 3.5, tip[1], tip[2] - 0.5), 0.9, 0.9, STEEL)
        elif tool == "hammer":
            sc.box(tip, (2.4, 1.6, 1.6), IRON)
        elif tool == "axe":
            sc.box((tip[0], tip[1] + 0.5, tip[2]), (0.8, 2.6, 2.0), STEEL)
    _head(sc, look, p, hipz + 16.6, "straw" if not settler else "hood" if look.has("worker") else "straw")


# ---------------------------------------------------------------- mounts

def _horse_legs(sc, p, col, hoof=DARKWOOD, big=1.0, hipz=9.5):
    s = p["stride"]
    for fo, side in ((6.2, -1), (6.2, 1), (-6.4, -1), (-6.4, 1)):
        phase = s * (1 if (fo > 0) == (side > 0) else -1)
        foot_f = fo + 3.5 * phase
        foot_z = max(0.0, phase) * 2.6
        knee = (side * 2.6, fo + 2.0 * phase + (1.0 if fo < 0 else 0.5), hipz * 0.45 + 0.8 * max(0, phase))
        sc.limb((side * 2.7, fo, hipz), knee, 1.7 * big, 1.3 * big, col)
        sc.limb(knee, (side * 2.6, foot_f, foot_z + 0.8), 1.3 * big, 1.1 * big, col)
        sc.ball((side * 2.6, foot_f + 0.3, foot_z + 0.4), (1.5 * big, 1.9 * big, 1.1), hoof)


def horse(sc, look, p, rider=True, kind="horse"):
    col = look.horse
    sc.ground_shadow(12, 4.8)
    if kind == "elephant":
        return elephant(sc, look, p)
    s = p["stride"]
    _horse_legs(sc, p, col, hipz=10.0)
    sc.ball((0, -5.8, 13.8), (4.4, 4.8, 5.4), col)
    sc.ball((0, 0, 13.6), (4.4, 6.0, 5.0), col)
    sc.ball((0, 5.8, 14.8), (4.2, 4.4, 5.6), col)
    neck_top = (0, 11.2, 24.0)
    sc.limb((0, 7.0, 17.0), neck_top, 2.9, 2.0, col)
    sc.limb((0.0, 7.6, 20.5), (0.0, 10.4, 25.0), 1.0, 0.9, F.tone(col, 0.55), bias=0.3)     # mane
    sc.ball((0, 13.8, 24.2), (1.9, 3.7, 2.2), F.tone(col, 1.05))
    sc.ball((0, 16.4, 23.4), (1.5, 1.4, 1.4), F.tone(col, 0.8))
    sc.limb((-0.9, 11.8, 26.2), (-1.1, 11.3, 28.4), 0.7, 0.2, col)
    sc.limb((0.9, 11.8, 26.2), (1.1, 11.3, 28.4), 0.7, 0.2, col)
    sc.limb((0, -10.0, 16.0), (0, -13.5 - s * 1.5, 8.5 + (3 if s else 0)), 1.5, 0.7, F.tone(col, 0.55))
    sc.ball((0, 0, 19.2), (4.9, 3.6, 0.9), CLOTH, team=True)                         # saddle cloth
    sc.ball((0, -1.2, 20.0), (3.2, 2.6, 1.1), LEATHER)
    if rider:
        return 20.0


def elephant(sc, look, p):
    col = (130, 126, 128)
    sc.ground_shadow(14, 5.6)
    s = p["stride"]
    for fo, side in ((6.5, -1), (6.5, 1), (-6.5, -1), (-6.5, 1)):
        ph = s * (1 if (fo > 0) == (side > 0) else -1)
        sc.limb((side * 4.2, fo, 10), (side * 4.2, fo + 2.5 * ph, max(0, ph) * 2.0 + 0.6), 3.0, 2.6, col)
        sc.ball((side * 4.2, fo + 2.5 * ph + 0.5, max(0, ph) * 2.0 + 0.4), (2.8, 3.2, 1.3), (96, 92, 96))
    sc.ball((0, -3.5, 17.0), (7.2, 7.5, 8.0), col)
    sc.ball((0, 4.0, 17.5), (7.0, 7.0, 8.4), F.tone(col, 1.05))
    sc.ball((0, 12.0, 20.0), (5.0, 4.6, 5.4), F.tone(col, 1.08))
    for side in (-1, 1):
        sc.ball((side * 6.4, 10.0, 21.0), (1.0, 3.6, 4.4), F.tone(col, 0.8))
        sc.limb((side * 2.4, 15.0, 16.5), (side * 3.0, 19.5, 15.0), 0.9, 0.5, (240, 236, 220))
    sc.limb((0, 15.0, 19.0), (0, 18.0, 13.0), 2.6, 1.9, col)
    sc.limb((0, 18.0, 13.0), (0, 20.0, 6.5 - (2 if p["arm"] == 2 else 0)), 1.9, 1.3, col)
    sc.limb((0, -10.0, 17.0), (0, -12.0, 9.0), 0.8, 0.5, F.tone(col, 0.6))
    sc.ball((0, 0, 25.4), (6.0, 5.0, 1.0), CLOTH, team=True)                          # howdah blanket
    sc.box((0, 0, 28.0), (3.6, 3.4, 1.8), WOOD)
    return 30.0


def rider(sc, look, p, weapon, seat, hat=None):
    """The mounted figure, seated: legs hang, torso, head, weapon."""
    zs = seat
    sc.limb((-3.2, 0, zs + 1.5), (-4.0, 2.0, zs - 5.0), 1.9, 1.5, look.trousers)
    sc.limb((3.2, 0, zs + 1.5), (4.0, 2.0, zs - 5.0), 1.9, 1.5, look.trousers)
    sc.ball((0, 0, zs + 6.0), (4.6, 3.2, 5.8), CLOTH, team=True)
    shz = zs + 10.6
    sk = look.skin
    if weapon == "spear":
        hr = (5.0, 4.5, zs + 6.5)
        _arm(sc, 1, (5, 0, shz), (6.0, 2.5, zs + 8), hr, True, skin=sk)
        tip = _weapon_spear(sc, look, (hr[0], hr[1] + 2, hr[2]), p, 0.5)
    elif weapon == "sword":
        hr = (5.2, 3.5, zs + 8.0)
        _arm(sc, 1, (5, 0, shz), (6.0, 1.5, zs + 8.5), hr, True, skin=sk)
        _weapon_sword(sc, look, hr, p)
    elif weapon == "gun":
        hr = (3.5, 4.5, zs + 7.5)
        _arm(sc, 1, (5, 0, shz), (5.5, 2.5, zs + 8), hr, True, skin=sk)
        tip = _weapon_gun(sc, look, hr, hr, p, False)
        if p["flash"]:
            sc.ball((tip[0], tip[1] + 2.5, tip[2]), (2.4, 2.8, 2.4), FIRE, bias=0.5)
    else:
        hr = (5.5, 3.5, zs + 7.5)
        _arm(sc, 1, (5, 0, shz), (6.0, 1.5, zs + 8.5), hr, True, skin=sk)
        _weapon_club(sc, look, hr, p)
    _arm(sc, -1, (-5, 0, shz), (-5.2, 2.0, zs + 7.5), (-3.5, 5.0, zs + 5.5), True, skin=sk)
    if weapon in ("sword", "spear"):
        sc.disc((-6.2, 2.0, zs + 6.5), 4.6, (-1, 0.1, 0), CLOTH, team=True, thick=1.0, rim=STEEL)
    _head(sc, look, p, zs + 17.0, hat or ("helm" if weapon == "sword" else "bronze" if weapon == "spear"
                                           else "hat" if weapon == "gun" else "band"))


def mounted(sc, look, p, weapon, name):
    if look.has("chariot"):
        return chariot(sc, look, p, weapon)
    if look.has("elephant"):
        seat = elephant(sc, look, p)
        rider(sc, look, p, weapon, seat - 1.0)
        return
    seat = horse(sc, look, p)
    if look.has("camel"):
        sc.ball((0, 0, 22.5), (3.4, 3.8, 4.0), look.horse)
    rider(sc, look, p, weapon, seat)


def chariot(sc, look, p, weapon):
    sc.ground_shadow(14, 5.0)
    col = look.horse
    s = p["stride"]
    # two small horses ahead, abreast
    for side in (-1, 1):
        ox = side * 3.6
        ph = s * side
        for fo, sd in ((15.0, -1), (15.0, 1), (7.5, -1), (7.5, 1)):
            pass
        sc.limb((ox - 1.5, 16, 9), (ox - 1.5, 16 + 2.5 * ph, max(0, ph) * 2.0), 1.4, 1.1, col)
        sc.limb((ox + 1.5, 16, 9), (ox + 1.5, 16 - 2.5 * ph, max(0, -ph) * 2.0), 1.4, 1.1, col)
        sc.limb((ox - 1.5, 9, 9), (ox - 1.5, 9 - 2.5 * ph, max(0, -ph) * 2.0), 1.4, 1.1, col)
        sc.limb((ox + 1.5, 9, 9), (ox + 1.5, 9 + 2.5 * ph, max(0, ph) * 2.0), 1.4, 1.1, col)
        sc.ball((ox, 12.5, 13), (2.4, 6.8, 3.8), col)
        sc.limb((ox, 17.0, 14.0), (ox, 20.0, 20.0), 2.0, 1.5, col)
        sc.ball((ox, 22.0, 20.0), (1.5, 2.8, 1.6), F.tone(col, 1.05))
    sc.limb((0, 9, 12.5), (0, -3, 9), 0.9, 0.9, DARKWOOD)                              # pole
    sc.box((0, -6, 10), (6.0, 4.2, 0.9), WOOD)
    sc.box((0, -9.8, 13), (6.0, 0.7, 3.0), CLOTH, team=True)
    for side in (-1, 1):
        sc.disc((side * 6.8, -6, 5.2), 5.2, (1, 0, 0), WOOD, thick=1.0, rim=DARKWOOD)
    sc.limb((-4.0, -6.0, 10.0), (-4.0, -6.0, 16.0), 1.9, 1.5, look.trousers)
    sc.ball((0, -6.0, 17.5), (4.2, 3.0, 5.2), CLOTH, team=True)
    sk = look.skin
    _arm(sc, 1, (4.5, -6, 21), (5.5, -3, 18), (5.0, 0, 18.5), True, skin=sk)
    _weapon_spear(sc, look, (5.0, 0, 18.5), p) if weapon == "spear" else None
    _head(sc, look, p, 25.5, "bronze")


# ----------------------------------------------------------------- siege

def siege(sc, look, p, name):
    sc.ground_shadow(13, 5.0)
    a = p["arm"]
    if look.has("catapult", "ballista", "trebuchet"):
        sc.box((0, 0, 5.2), (4.4, 8.4, 1.1), WOOD)
        sc.box((0, 0, 3.0), (3.2, 7.4, 0.8), DARKWOOD)
        for fo in (-5.0, 5.0):
            for side in (-1, 1):
                sc.disc((side * 5.4, fo, 3.4), 3.6, (1, 0, 0), WOOD, thick=1.0, rim=DARKWOOD)
        sc.box((0, -2, 8.6), (3.2, 1.2, 2.4), CLOTH, team=True)
        if look.has("trebuchet"):
            sc.limb((-3.5, 0, 6), (-0.4, 0, 22), 1.2, 1.2, DARKWOOD)
            sc.limb((3.5, 0, 6), (0.4, 0, 22), 1.2, 1.2, DARKWOOD)
            ang = {1: (0, -9, 24), 2: (0, 10, 14)}.get(a, (0, -8, 18))
            sc.limb((0, 0, 22), (0, -ang[1] * 0.45, 22 - (ang[2] - 22) * 0.45), 1.3, 1.1, WOOD)
            sc.limb((0, 0, 22), ang, 1.1, 0.8, WOOD)
            sc.box((0, -ang[1] * 0.45, 20 - (ang[2] - 22) * 0.45), (2.4, 2.4, 2.0), GREYSTEEL)
        elif look.has("ballista"):
            sc.limb((0, 1, 9.5), (0, 8, 11.5 if a != 2 else 10.5), 1.0, 0.8, DARKWOOD)
            sc.limb((-8, 5, 11.5), (0, 3.5, 11.5), 0.9, 1.0, WOOD)
            sc.limb((8, 5, 11.5), (0, 3.5, 11.5), 0.9, 1.0, WOOD)
            sc.rod((-8, 5, 11.5), (0, 0 if a == 1 else 4.5, 11.5), 0.4, (232, 232, 236))
            sc.rod((8, 5, 11.5), (0, 0 if a == 1 else 4.5, 11.5), 0.4, (232, 232, 236))
            sc.rod((0, -1 if a == 1 else 3, 11.8), (0, 13, 11.8), 0.9, BRONZE)
        else:
            tip = {1: (0, -6, 12), 2: (0, 9, 14)}.get(a, (0, 5, 17))
            sc.limb((0, -3.5, 7.0), tip, 1.5, 1.2, WOOD)
            sc.ball((tip[0], tip[1], tip[2] + 1.2), (2.6, 2.6, 1.4), DARKWOOD)
            if a == 2:
                sc.ball((tip[0], tip[1] + 3, tip[2] + 5), (1.6, 1.6, 1.6), (130, 126, 128))
            sc.limb((-3, 3.5, 6.5), (3, 3.5, 6.5), 0.8, 0.8, DARKWOOD)
            sc.limb((-3, -3.5, 7.0), (-3, 3.5, 7.0), 0.7, 0.7, DARKWOOD)
            sc.limb((3, -3.5, 7.0), (3, 3.5, 7.0), 0.7, 0.7, DARKWOOD)
        return
    modern = look.has("artillery", "howitzer")
    body = OLIVE if modern else DARKWOOD
    barrel = (88, 92, 100) if modern else (70, 72, 82)
    if modern:
        for side in (-1, 1):
            sc.box((side * 5.0, 0, 2.6), (1.9, 8.2, 2.4), (58, 56, 62))
        sc.box((0, 0, 6.5), (4.2, 7.0, 2.0), OLIVE)
        sc.box((0, 0, 6.5), (4.4, 1.5, 2.2), CLOTH, team=True)
        sc.limb((0, -3, 9.5), (0, 16 - (2 if a == 1 else 0), 11.0), 2.1, 1.7, barrel)
        sc.ball((0, -2, 9.5), (3.4, 3.8, 2.4), F.tone(OLIVE, 1.1))
    else:
        sc.box((0, -1.0, 5.6), (3.0, 6.2, 1.4), body)
        sc.box((0, -1.0, 7.4), (3.2, 2.0, 0.6), CLOTH, team=True)
        for side in (-1, 1):
            sc.disc((side * 4.9, 0.5, 4.3), 4.4, (1, 0, 0), WOOD, thick=1.1, rim=DARKWOOD)
        sc.limb((0, -5.0, 8.0), (0, 12 - (2.5 if a == 1 else 0), 10.0), 2.5, 2.0, barrel)
        sc.ball((0, 12.5 - (2.5 if a == 1 else 0), 10.0), (2.4, 1.0, 2.4), (50, 52, 60))
        sc.limb((0, -4.5, 5.8), (0, -9.5, 2.8), 1.0, 0.9, DARKWOOD)                      # trail
    if p["flash"]:
        sc.ball((0, 19, 10.5), (3.8, 4.4, 3.8), FIRE, bias=0.5)
        sc.ball((0, 22, 10.5), (2.4, 2.8, 2.4), (255, 240, 200), bias=0.6)


# ---------------------------------------------------------------- ships

def _hull(sc, length, beam, deck, col, deck_col, bow=1.1, stern=0.6, sheer=1.8):
    n = 11
    st = []
    for i in range(n):
        t = -1 + 2 * i / (n - 1)
        if t >= 0:
            w = beam * max(0.0, (1 - t ** (1.5 if bow > 1 else 2.4))) ** 0.6
        else:
            w = beam * max(0.0, (1 - abs(t) ** 3.2)) ** 0.5
        top = deck + sheer * abs(t) ** 2.5
        st.append((t * length, w, top))
    for (f0, w0, z0), (f1, w1, z1) in zip(st, st[1:]):
        for side in (-1, 1):
            pts = [(side * w0, f0, 0.0), (side * w1, f1, 0.0), (side * w1, f1, z1), (side * w0, f0, z0)]
            sc.facet(pts, (0, (f0 + f1) / 2, deck * 0.5), col)
        sc.quad([(-w0, f0, z0), (w0, f0, z0), (w1, f1, z1), (-w1, f1, z1)], (0, 0, 1), deck_col)
    return st


def ship(sc, look, p, name):
    sc.ground_foam(24, 8.5)
    sc.ground_shadow(21, 6.5, 0, 1.5)
    a = p["arm"]
    if look.has("submarine"):
        sc.ball((0, 0, 1.5), (3.8, 15, 3.4), (60, 66, 78))
        sc.ball((0, -1.0, 4.5), (2.0, 4.0, 2.6), (74, 80, 94))
        sc.rod((0, 0, 6.0), (0, 1.2, 11.5), 0.8, (60, 66, 78))
        sc.box((0, -5, 3.9), (3.9, 0.5, 0.5), CLOTH, team=True)
        return
    if look.has("carrier"):
        _hull(sc, 24, 6.8, 5.0, (118, 128, 142), (110, 116, 126), 1.0)
        sc.box((0, 0, 5.6), (6.4, 21, 0.5), (92, 98, 108))
        sc.box((0, 0, 6.2), (0.35, 18.5, 0.1), (230, 230, 230))
        sc.box((4.4, -4, 9.0), (1.5, 4.0, 3.0), (136, 144, 156))
        sc.box((4.4, -4, 12.4), (1.0, 2.0, 0.5), CLOTH, team=True)
        return
    modern = look.has("destroyer", "battleship", "cruiser", "transport", "ironclad", "carrier")
    steel = (126, 136, 150)
    if modern:
        L = {"battleship": 24, "cruiser": 21, "destroyer": 19, "transport": 20, "ironclad": 17}
        ln = next((v for k, v in L.items() if look.has(k)), 20)
        _hull(sc, ln, 5.4 if not look.has("transport") else 6.4, 4.2, steel, (96, 104, 116), 1.2, sheer=1.4)
        sc.box((0, -ln * 0.15, 7.0), (3.0, 5.0, 2.6), (150, 158, 170))
        sc.box((0, -ln * 0.15, 10.2), (2.0, 3.0, 1.0), (172, 180, 190))
        sc.box((0, -ln * 0.15, 11.4), (1.4, 2.0, 0.4), CLOTH, team=True)
        sc.limb((0, -ln * 0.3, 11), (0, -ln * 0.3, 18), 0.7, 0.7, IRON)
        if look.has("ironclad"):
            sc.limb((0, -3, 8), (0, -3, 16), 2.0, 1.6, (70, 70, 78))
        if look.has("battleship", "cruiser", "destroyer", "ironclad"):
            for fpos in ((ln * 0.5, 6.2), (ln * 0.15 if not look.has("destroyer") else -ln * 0.55, 6.0)):
                sc.ball((0, fpos[0], 5.8), (3.0, 3.0, 1.6), (112, 120, 134))
                sc.limb((0, fpos[0], 6.2), (0, fpos[0] + 8 - (2 if a == 1 else 0), 6.6), 0.9, 0.8, (60, 62, 70))
        if look.has("transport"):
            sc.box((0, 6, 6.2), (4.0, 3.5, 1.2), (176, 140, 90))
            sc.box((0, 11.5, 6.0), (3.6, 1.6, 1.0), (196, 156, 100))
        if p["flash"]:
            sc.ball((0, ln * 0.5 + 12, 7.2), (3.2, 3.6, 3.2), FIRE, bias=0.6)
        return
    # sailing ships
    big = look.has("galleon", "frigate", "man-o-war", "privateer", "caravel")
    ln = 20 if big else 15
    sail_n = 3 if look.has("galleon", "frigate", "man-o-war") else 2 if look.has("caravel", "privateer") else 1
    hull_col = (122, 80, 46)
    if look.has("canoe", "raft"):
        _hull(sc, 12, 3.0, 2.0, (168, 122, 74), (140, 98, 58), 1.1, sheer=1.2)
        sc.ball((0, -2.5, 3.4), (1.9, 1.9, 2.2), look.skin)
        sc.ball((0, -2.5, 6.2), (1.6, 1.6, 1.8), CLOTH, team=True)
        sc.rod((3, 1, 3), (6, 8, 0.5), 0.8, DARKWOOD)
        return
    _hull(sc, ln, 5.4, 4.0, hull_col, (170, 130, 84), 1.2)
    sc.limb((-5.0, -ln * 0.1, 4.0), (-5.0, ln * 0.7, 4.0), 0.5, 0.5, (210, 180, 90)) if big else None
    if look.has("galley", "dromon"):
        for fo in (-6, -2, 2, 6):
            for side in (-1, 1):
                sc.rod((side * 5.0, fo, 3.0), (side * 11.5, fo - 2.5 + (2 if p["phase"] % 2 else 0), 0.3), 0.7, DARKWOOD)
    for i in range(sail_n):
        fo = (i - (sail_n - 1) / 2.0) * 8.0
        h = 20 if i == (sail_n // 2) else 16
        sc.rod((0, fo, 4.0), (0, fo, 4.0 + h), 1.1, DARKWOOD)
        sw = 7.5 if h > 16 else 6.5
        z0, z1 = 7.0, 4.0 + h - 1.0
        sc.poly([(-sw, fo + 0.8, z0), (sw, fo + 0.8, z0), (sw, fo + 0.8, z1), (-sw, fo + 0.8, z1)], CANVAS)
        sc.poly([(-sw, fo + 0.8, z0 + (z1 - z0) * 0.42), (sw, fo + 0.8, z0 + (z1 - z0) * 0.42),
                 (sw, fo + 0.8, z0 + (z1 - z0) * 0.62), (-sw, fo + 0.8, z0 + (z1 - z0) * 0.62)], CLOTH, team=True, bias=0.2)
        sc.rod((-sw, fo + 0.4, z1), (sw, fo + 0.4, z1), 0.9, DARKWOOD)
    top = 4.0 + (20 if sail_n >= 1 else 16)
    sc.poly([(0, 0, top), (0, -5.0, top - 1.0), (0, 0, top - 2.5)], CLOTH, team=True, bias=0.3)
    if look.has("frigate", "man-o-war", "privateer", "galleon"):
        for fo in (-6, -2, 2, 6):
            for side in (-1, 1):
                sc.ball((side * 5.4, fo, 3.0), (0.9, 1.1, 0.9), (40, 40, 46))
    if p["flash"]:
        sc.ball((6.5, 3, 3.5), (3.0, 3.0, 3.0), FIRE, bias=0.6)


# ------------------------------------------------------------------ air

def air(sc, look, p, name):
    sc.ground_shadow(12, 4.2, 0, 0)
    sc.lift = 15.0 + math.sin(p["phase"] * 1.4) * 0.6
    if look.has("helicopter"):
        sc.ball((0, 0, 3), (3.8, 6.5, 3.8), (110, 120, 96))
        sc.limb((0, -5, 3.5), (0, -16, 5.5), 1.5, 0.8, (110, 120, 96))
        sc.limb((0, -16, 5.5), (0, -16.5, 9.5), 0.7, 0.7, (80, 84, 74))
        sc.ball((0, 3.5, 3.6), (2.8, 2.4, 2.4), (150, 190, 220))
        for side in (-1, 1):
            sc.limb((side * 3.3, -3, -1.5), (side * 3.3, 4, -1.5), 0.5, 0.5, IRON)
            sc.limb((side * 3.0, 0, 0.5), (side * 3.3, 0, -1.2), 0.4, 0.4, IRON)
        sc.limb((0, 0, 6.5), (0, 0, 8.0), 0.8, 0.8, IRON)
        ph = p["phase"] * 0.9
        for k in range(2):
            a = ph + k * math.pi
            sc.rod((-math.cos(a) * 11, -math.sin(a) * 11, 8.2), (math.cos(a) * 11, math.sin(a) * 11, 8.2), 0.8, (60, 60, 66))
        sc.box((0, -1, 6.6), (2.0, 2.0, 0.5), CLOTH, team=True)
        return
    if look.has("zeppelin"):
        sc.ball((0, 0, 4), (6.5, 17, 6.5), (210, 206, 190))
        sc.ball((0, 2, 11.5 - 7), (4.6, 12, 4.0), (232, 228, 212))
        sc.box((0, 3, -3.2), (1.8, 4.5, 1.4), DARKWOOD)
        sc.poly([(0, -14, 5), (0, -20, 11), (0, -12, 6)], CLOTH, team=True)
        sc.poly([(0, -14, 3), (0, -20, -3), (0, -12, 2)], (180, 176, 164))
        sc.poly([(-1, -14, 4), (-9, -19, 4), (-1, -11, 4)], (180, 176, 164))
        sc.poly([(1, -14, 4), (9, -19, 4), (1, -11, 4)], (180, 176, 164))
        return
    bomber = look.has("bomber")
    stealth = look.has("stealth")
    col = (60, 62, 70) if stealth else (150, 158, 170) if not bomber else (112, 122, 100)
    wing = 15 if bomber else 11
    sweep = 5 if not bomber else 1
    sc.limb((0, -12, 1), (0, 9, 1), 1.9 if not bomber else 2.6, 1.0 if not bomber else 1.7, col)
    sc.limb((0, 8, 1), (0, 15, 0.6), 1.4 if not bomber else 1.7, 0.2, F.tone(col, 1.1))
    sc.ball((0, 4.5, 2.6), (1.3, 2.4, 1.2), (150, 200, 230))
    if stealth:
        sc.poly([(0, 12, 0.8), (-14, -9, 0.8), (-5, -9, 0.8), (0, -6, 0.8), (5, -9, 0.8), (14, -9, 0.8)], col)
    else:
        sc.poly([(-1.5, 4, 0.8), (-wing, 4 - sweep * 1.4, 0.4), (-wing, -2 - sweep, 0.4), (-1.5, -4, 0.8)], F.tone(col, 1.05))
        sc.poly([(1.5, 4, 0.8), (wing, 4 - sweep * 1.4, 0.4), (wing, -2 - sweep, 0.4), (1.5, -4, 0.8)], F.tone(col, 1.05))
        sc.poly([(-1, -9, 1), (-5, -13, 1), (-5, -11, 1), (-1, -6, 1)], F.tone(col, 0.9))
        sc.poly([(1, -9, 1), (5, -13, 1), (5, -11, 1), (1, -6, 1)], F.tone(col, 0.9))
    sc.poly([(0, -6, 1.5), (0, -13, 8.0), (0, -13, 5.0), (0, -10, 1.5)], CLOTH, team=True)
    if bomber:
        for side in (-1, 1):
            for x in (5, 10):
                sc.ball((side * x, 2, -0.5), (1.4, 2.8, 1.4), (72, 76, 70))
    sc.box((wing * 0.62, -1, 0.6), (3.0, 1.8, 0.2), CLOTH, team=True)
    if p["flash"]:
        sc.ball((0, 19, 0.6), (2.0, 3.2, 2.0), FIRE, bias=0.5)


# --------------------------------------------------------------- ground

def vehicle(sc, look, p, name):
    sc.ground_shadow(13, 5.0)
    a = p["arm"]
    if look.has("wagon"):
        sc.box((0, -1, 8), (4.6, 7.0, 1.0), WOOD)
        sc.box((0, -1, 11), (4.4, 0.6, 3.0), DARKWOOD)
        sc.box((0, -1, 11.4), (4.4, 6.0, 2.6), CANVAS)
        sc.box((0, -1, 14.2), (4.5, 6.2, 0.3), CLOTH, team=True)
        for fo in (-4.5, 3.5):
            for side in (-1, 1):
                sc.disc((side * 5.4, fo, 4.2), 4.2, (1, 0, 0), WOOD, thick=0.8, rim=DARKWOOD)
        sc.limb((0, 6, 8.5), (0, 12, 6.5), 0.8, 0.8, DARKWOOD)
        return
    big = look.has("armor", "panzer")
    s = 1.0 if big else 0.9
    col = (96, 108, 74)
    for side in (-1, 1):
        sc.box((side * 5.6 * s, 0, 3.0), (2.0, 8.8 * s, 2.5), (54, 54, 60))
        sc.box((side * 5.6 * s, 0, 5.6), (2.2, 8.9 * s, 0.4), (84, 84, 92))
        for fo in (-6, -2, 2, 6):
            sc.disc((side * 7.7 * s, fo * s, 3.0), 1.8, (side, 0, 0), (96, 98, 106), thick=0.0)
    sc.box((0, 0, 6.6), (4.4 * s, 8.0 * s, 1.8), col)
    sc.poly([(-4.4 * s, 8.0 * s, 8.4), (4.4 * s, 8.0 * s, 8.4), (4.4 * s, 10.5 * s, 6.2), (-4.4 * s, 10.5 * s, 6.2)], F.tone(col, 1.1))
    for side in (-1, 1):
        sc.box((side * 4.5 * s, 0, 6.8), (0.2, 6.0, 1.0), CLOTH, team=True)
    sc.ball((0, -0.8, 10.4), (4.0 * s, 4.8 * s, 2.0), F.tone(col, 1.05))
    rec = -2.5 if a == 1 else 0
    sc.limb((0, 3.0, 11.0), (0, 15.5 * s + rec, 11.3), 1.2, 1.0, (70, 72, 62))
    sc.ball((0, 15.8 * s + rec, 11.3), (1.3, 1.0, 1.3), (50, 52, 46))
    sc.box((0.8, -1.5, 12.8), (1.1, 1.1, 0.35), CLOTH, team=True)
    if p["flash"]:
        sc.ball((0, 19 * s, 11.6), (3.4, 3.8, 3.4), FIRE, bias=0.6)


def mech(sc, look, p, name):
    sc.ground_shadow(11, 4.4)
    s = p["stride"]
    col = (98, 108, 86)
    for side in (-1, 1):
        ph = s * side
        knee = (side * 4.0, 3.0 + 2.5 * ph, 11.0)
        foot = (side * 4.4, 4.0 * ph, 1.2 + max(0, ph) * 2.0)
        sc.limb((side * 3.8, 0, 20), knee, 2.6, 2.1, col)
        sc.limb(knee, foot, 2.1, 1.7, F.tone(col, 0.9))
        sc.box((foot[0], foot[1] + 1.4, foot[2] - 0.2), (2.4, 3.6, 0.9), (60, 62, 56))
    sc.box((0, 0, 24), (5.4, 3.6, 4.0), col)
    sc.box((0, 3.4, 25), (3.8, 0.3, 2.6), CLOTH, team=True)
    sc.ball((0, 1.2, 30.5), (3.0, 2.8, 2.4), (150, 200, 220))
    for side in (-1, 1):
        sc.limb((side * 6.8, 0, 26), (side * 7.4, 4, 21), 2.0, 1.7, F.tone(col, 0.9))
        sc.limb((side * 7.4, 4, 21), (side * 7.4, 12 + (3 if p["arm"] == 2 else 0), 21.5), 1.4, 1.2, (66, 68, 62))
    if p["flash"]:
        sc.ball((7.4, 17.5, 21.5), (2.6, 3.0, 2.6), FIRE, bias=0.6)


def missile(sc, look, p, name):
    sc.ground_shadow(8, 3.4)
    cruise = look.has("cruise", "tactical")
    body = (214, 216, 222)
    if cruise:
        sc.lift = 7.0
        sc.limb((0, -11, 6), (0, 8, 6), 2.2, 2.2, body)
        sc.limb((0, 8, 6), (0, 15, 6), 2.2, 0.3, (190, 60, 52))
        sc.limb((0, -3.6, 6), (0, -1.6, 6), 2.4, 2.4, CLOTH, team=True)
        sc.poly([(-1, -3, 6), (-9, -7, 6), (-9, -4, 6), (-1, 1, 6)], (150, 156, 166))
        sc.poly([(1, -3, 6), (9, -7, 6), (9, -4, 6), (1, 1, 6)], (150, 156, 166))
        sc.poly([(0, -10, 6), (0, -13, 11), (0, -8, 6)], (150, 156, 166))
        if p["flash"]:
            sc.ball((0, -15, 6), (2.4, 3.6, 2.4), FIRE, bias=0.5)
        return
    sc.box((0, 0, 1.6), (5.4, 5.4, 1.4), (88, 92, 100))
    sc.limb((0, 0, 3), (0, 0, 28), 2.9, 2.9, body)
    sc.limb((0, 0, 28), (0, 0, 38), 2.9, 0.4, (190, 60, 52))
    sc.limb((0, 0, 12), (0, 0, 16), 3.1, 3.1, CLOTH, team=True)
    for k in range(3):
        a = math.radians(90 + 120 * k)
        sc.poly([(math.cos(a) * 2.8, math.sin(a) * 2.8, 3), (math.cos(a) * 7.8, math.sin(a) * 7.8, 1.5),
                 (math.cos(a) * 7.8, math.sin(a) * 7.8, 7.5), (math.cos(a) * 2.8, math.sin(a) * 2.8, 11)], (150, 156, 166))
    if p["flash"]:
        sc.ball((0, 0, 0.5), (5.0, 5.0, 3.0), FIRE, bias=0.6)


def leader(sc, look, p, name):
    human(sc, look, dict(p, flag=True), "sword", name, hat="plume")
    sc.rod((-8.0, 2.0, 0), (-8.0, 2.0, 46), 1.1, DARKWOOD)
    sc.poly([(-8, 2, 45), (-8, -9, 41), (-8, -9, 31), (-8, 2, 33)], CLOTH, team=True)
    sc.ball((-8, 2, 46.5), (1.2, 1.2, 1.2), GOLD)


# Canvas (w, h) of each family's FLC frames; the model scale inside it.
SIZES = {"foot": (88, 88), "civilian": (88, 88), "mounted": (104, 88), "siege": (104, 76),
         "ship": (128, 96), "air": (112, 80), "vehicle": (104, 76), "mech": (96, 88),
         "missile": (72, 96), "leader": (96, 104)}
KINDS = {"foot": 1.5, "civilian": 1.5, "mounted": 1.3, "siege": 1.45, "ship": 1.55, "air": 1.6,
         "vehicle": 1.7, "mech": 1.4, "missile": 1.4, "leader": 1.25}


# Rows kept free under the feet for the shadow (and a ship's foam ring).
MARGIN = {"foot": 8, "civilian": 8, "mounted": 17, "siege": 9, "ship": 24, "air": 9,
          "vehicle": 10, "mech": 8, "missile": 7, "leader": 14}


def frame(quant, name, kind, weapon, w, h, d, slot, i, fpd, plain=False, zoom=1.0):
    look = Look(name)
    p = pose(slot, i, fpd)
    sc = F.Scene(w, h, d, gy=h - MARGIN.get(kind, 8), k=KINDS.get(kind, 1.2) * zoom)
    wreck = 0
    if p["pitch"]:
        if kind in ("foot", "civilian", "mounted", "leader"):
            sc.pitch = p["pitch"]
            sc.gy -= 12 * min(1.0, sc.pitch)
            sc.k *= 0.92
        else:
            wreck = 1 if p["pitch"] < 1.0 else 2
            if kind == "ship":
                sc.pitch = -0.10 * wreck
                sc.lift = -3.0 * wreck
                sc.gy -= 10
            elif kind == "air":
                sc.pitch = -0.35 * wreck
                sc.lift = 14 - 6 * wreck
    if kind == "foot":
        human(sc, look, p, weapon, name)
    elif kind == "civilian":
        civilian(sc, look, p, name)
    elif kind == "mounted":
        mounted(sc, look, p, weapon, name)
    elif kind == "siege":
        siege(sc, look, p, name)
    elif kind == "ship":
        ship(sc, look, p, name)
    elif kind == "air":
        air(sc, look, p, name)
    elif kind == "vehicle":
        vehicle(sc, look, p, name)
    elif kind == "mech":
        mech(sc, look, p, name)
    elif kind == "missile":
        missile(sc, look, p, name)
    else:
        leader(sc, look, p, name)
    if wreck:
        _wreck(sc, kind, wreck)
    if plain:
        sc.shadows, sc.foam = [], []
    return sc.finish(quant)


def icon(quant, name, kind, weapon, size=32):
    """A `size` x `size` portrait of the unit for the unit-icon sheet: the
    figure in three-quarter view, scaled to fill the cell."""
    w, h = SIZES[kind]
    d = 0 if kind == "missile" else 1

    def box(im):
        mask = Image.frombytes("L", im.size, im.tobytes()).point(lambda v: 0 if v in (254, 255) else 255)
        return mask.getbbox()
    first = frame(quant, name, kind, weapon, w, h, d, "DEFAULT", 0, 1, plain=True)
    bb = box(first)
    zoom = (size - 2) / float(max(bb[2] - bb[0], bb[3] - bb[1]))
    big = frame(quant, name, kind, weapon, w, h, d, "DEFAULT", 0, 1, plain=True, zoom=zoom)
    bb = box(big)
    crop = big.crop(bb)
    out = Image.new("P", (size, size), quant.BG)
    out.putpalette(big.getpalette())
    out.paste(crop, ((size - crop.width) // 2, size - 1 - crop.height))
    return out


def _wreck(sc, kind, level):
    """Fire and smoke over a destroyed machine."""
    z = {"ship": 8, "air": 6, "missile": 10}.get(kind, 12)
    sc.ball((0, 0, z), (3.0 + level, 3.0 + level, 2.4 + 0.6 * level), (40, 38, 44), bias=1.0)
    sc.ball((-1, 0.5, z + 1.5), (2.2 + 0.6 * level, 2.2 + 0.6 * level, 2.0 + 0.5 * level), FIRE, bias=1.2)
    sc.ball((1.5, -1, z + 4.0), (1.6 + 0.5 * level, 1.6 + 0.5 * level, 1.6), (255, 232, 140), bias=1.3)
    if level > 1:
        sc.ball((2, 1, z + 9), (3.6, 3.6, 3.2), (66, 64, 70), bias=1.4)
        sc.ball((-1, 0, z + 13), (2.6, 2.6, 2.4), (92, 90, 96), bias=1.5)
