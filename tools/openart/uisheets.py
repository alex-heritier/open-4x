"""The interface sheets: each function returns the RGBA image of one file.

Sizes and cell grids follow what `tools/prep_assets.py` and the game crop
(see the comments there); everything drawn is original. Transparent pixels
become the PCX key colour when `screens.indexed` quantises a sheet.
"""
import math

from PIL import Image, ImageDraw, ImageFilter

from . import figures as F
from . import pictos, portraits, screens
from .screens import blank, frame, parchment, plate, rule, well

ERA_RAMPS = [screens.PARCH,
             [(0.0, (160, 166, 182)), (0.45, (198, 204, 218)), (1.0, (234, 236, 244))],
             [(0.0, (170, 140, 116)), (0.45, (210, 184, 156)), (1.0, (240, 224, 204))],
             [(0.0, (150, 168, 164)), (0.45, (190, 208, 206)), (1.0, (230, 240, 240))]]
INK = (46, 28, 20)
GOLD = (236, 190, 66)


def _round_mask(size, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, size[0] - 1, size[1] - 1), radius=radius, fill=255)
    return m


def _put(sheet, tile, xy):
    sheet.alpha_composite(tile, xy)


def _glyph(fn, size):
    return pictos.rgba(fn, size)


def _fit(tile, w, h):
    """A glyph centred in a w x h transparent box."""
    out = blank((w, h))
    out.alpha_composite(tile, ((w - tile.width) // 2, (h - tile.height) // 2))
    return out


# ------------------------------------------------------------- city screen

def city_background():
    """1024x768 parchment with the city view cut out of rows 92..507."""
    W, H = 1024, 768
    img = parchment((W, H), 11)
    rule(img, 82, 10)
    rule(img, 508, 10)
    for x in (6, W - 7):                                           # studs at the rule ends
        d = ImageDraw.Draw(img)
        d.ellipse((x - 4, 83, x + 4, 91), fill=(236, 200, 120), outline=(54, 36, 22))
        d.ellipse((x - 4, 509, x + 4, 517), fill=(236, 200, 120), outline=(54, 36, 22))
    well(img, (714, 31, 831, 55), 0.78)                            # culture bar
    # The engine prints each heading just above its bar and the garrison under
    # the commerce rows, so the wells start below the text and keep clear of it.
    well(img, (288, 524, 836, 548))                                # production
    well(img, (288, 573, 772, 593))                                # food
    well(img, (794, 566, 880, 668))                                # growth box
    for row in range(3):
        y = 621 + row * 46
        well(img, (424, y, 716, y + 22))                           # commerce rows
    well(img, (902, 624, 1022, 738))                               # shield grid
    return screens.from_rgb(img, hole=(0, 92, W - 1, 507))


def city_xandview():
    """282x280: three round close buttons (idle, rollover, pressed)."""
    sheet = blank((282, 280))
    for i in range(3):
        def draw(d, s, box, i=i):
            screens.glyph_x(d, s, box, INK)
        _put(sheet, plate(30, 30, "bronze", i, 14, draw), (5 + i * 43, 58))
    return sheet


def city_buildings(entries):
    """448x3000: 32 px icons on a 33 px grid; row i + 1 is `BLDG` row i, and
    the ancient column (1) onward repeats the picture."""
    sheet = blank((448, 3000))
    for i, entry in enumerate(entries):
        tile = _glyph(pictos.for_building(entry), 32)
        for c in range(1, 13):
            _put(sheet, tile, (1 + c * 33, 1 + (i + 1) * 33))
    return sheet


def city_icons():
    """776x32: 25 icons of 30 px on a 31 px stride."""
    sheet = blank((776, 32))
    for i, fn in enumerate(pictos.CITY_ICONS):
        _put(sheet, _glyph(fn, 30), (1 + i * 31, 1))
    return sheet


def prod_button():
    """400x95: the production button in three 115x95 states (116 px stride)."""
    sheet = blank((400, 95))
    for i in range(3):
        def draw(d, s, box):
            x0, y0, x1, y1 = box
            d.rounded_rectangle((x0 + 8 * s, y0 + 8 * s, x1 - 8 * s, y1 - 8 * s), radius=4 * s,
                                fill=(226, 208, 166, 255), outline=(54, 36, 22, 255), width=s)
        _put(sheet, plate(115, 95, "bronze", i, 8, draw), (i * 116, 0))
    return sheet


def hurry_button():
    """200x28: three 28x28 cells at x = 1 + 29 i holding a coin and an arrow."""
    sheet = blank((200, 28))
    coin = _glyph(pictos.i_coin, 18)
    for i in range(3):
        base = plate(28, 28, "bronze", i, 14)
        base.alpha_composite(coin, (5, 5 + (1 if i == 2 else 0)))
        _put(sheet, base, (1 + i * 29, 0))
    return sheet


def mgmt_buttons():
    """197x238: previous / next / (unused) / close, three states in rows of 48."""
    sheet = blank((197, 238))
    cols = {"prev": (1, 42), "next": (44, 42), "x": (154, 39)}
    for row in range(3):
        for name, (x0, w) in cols.items():
            def draw(d, s, box, name=name):
                if name == "x":
                    screens.glyph_x(d, s, box, INK, 3.0, 0.3)
                else:
                    screens.glyph_tri(d, s, box, "left" if name == "prev" else "right", INK, 0.3)
            _put(sheet, plate(w, 46, "bronze", row, 7, draw), (x0, row * 48 + 1))
    return sheet


def fade_bar(h, top, alpha):
    """1024 x h: a vertical fade (the colour bar, or its alpha sheet)."""
    img = Image.new("RGB", (1024, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        t = 1 - t if top else t
        v = int(255 * t) if alpha else 0
        col = (v, v, v) if alpha else (66, 46, 28)
        d.line([(0, y), (1023, y)], fill=col)
    return screens.from_rgb(img)


def queue_box():
    """203x360: the production queue panel (frame, title bar, list well)."""
    img = parchment((203, 360), 21)
    frame(img, (0, 0, 202, 359), "bronze", 4)
    rule(img, 6, 24, "dark", 6, 196)
    well(img, (6, 36, 196, 353), 0.86)
    return screens.from_rgb(img)


# ----------------------------------------------------------------- advisors

def advisor_panel(era, seed):
    """1024x768: only the panel (60,40)-(964,700) is drawn."""
    sheet = blank((1024, 768))
    panel = parchment((905, 661), seed, ERA_RAMPS[era])
    rule(panel, 0, 6)
    rule(panel, 655, 6)
    frame(panel, (0, 0, 904, 660), "bronze", 5)
    rule(panel, 62, 6, "bronze", 5, 899)
    sheet.alpha_composite(screens.from_rgb(panel), (60, 40))
    return sheet


def dialog_box():
    img = parchment((207, 132), 31)
    frame(img, (0, 0, 206, 131), "bronze", 4)
    return screens.from_rgb(img)


def non_required():
    sheet = blank((45, 45))
    def draw(d, s, box):
        x0, y0, x1, y1 = box
        cy = (y0 + y1) / 2
        d.line([(x0 + 0.3 * (x1 - x0), cy), (x1 - 0.3 * (x1 - x0), cy)], fill=(244, 240, 226, 255), width=3 * s)
    _put(sheet, plate(27, 27, "blue", 0, 13, draw), (0, 0))
    return sheet


def exit_button():
    sheet = blank((100, 50))
    for i in range(3):
        _put(sheet, plate(26, 30, "bronze", i, 5, lambda d, s, b: screens.glyph_x(d, s, b, INK, 2.2, 0.27)), (26 * i, 0))
    return sheet


def govt_button():
    sheet = blank((250, 100))
    for i in range(3):
        _put(sheet, plate(146, 26, "bronze", i, 6), (0, 26 * i))
    return sheet


def plusminus_small():
    sheet = blank((102, 50))
    for i in range(3):
        _put(sheet, plate(12, 8, "bronze", i, 2, lambda d, s, b: screens.glyph_sign(d, s, b, False, INK, 1.0, .27)), (12 * i, 0))
        _put(sheet, plate(12, 13, "bronze", i, 2, lambda d, s, b: screens.glyph_sign(d, s, b, True, INK, 1.0, .27)), (12 * i, 8))
    return sheet


def plusminus_aux():
    sheet = blank((200, 125))
    for i in range(3):
        for x, plus in ((50, False), (74, True)):
            _put(sheet, plate(24, 24, "bronze", i, 5, lambda d, s, b, plus=plus: screens.glyph_sign(d, s, b, plus, INK, 2.4, .26)),
                 (x, 24 * i))
    return sheet


def domestic_icons():
    """1024x768 icon cells; the game reads the flask, coins and smiley."""
    sheet = blank((1024, 768))
    for fn, (x, y) in ((pictos.i_flask, (138, 256)), (pictos.i_coin, (172, 256)), (pictos.i_happy, (250, 250))):
        tile = _fit(_glyph(fn, 21), 21, 27)
        _put(sheet, tile, (x + 1, y + 1))
    return sheet


def techboxes():
    """1000x1483: 16 rows (4 eras x 4 sizes) of 4 states, 181x81 each."""
    sheet = blank((1000, 1483))
    mask = _round_mask((181, 81), 7)
    for i in range(16):
        era = i // 4
        for state in range(4):
            ramp = ERA_RAMPS[era]
            if state == 3:
                ramp = screens.SLATE
            elif state == 1:
                ramp = [(0.0, (222, 172, 84)), (0.5, (244, 208, 124)), (1.0, (252, 236, 178))]
            img = parchment((181, 81), 100 + i * 4 + state, ramp, vignette=0.12)
            frame(img, (0, 0, 180, 80), "bronze" if state != 3 else "dark", 3)
            if state == 0:
                d = ImageDraw.Draw(img)
                d.rectangle((3, 3, 177, 77), outline=(96, 156, 82))
            tile = img.convert("RGBA")
            tile.putalpha(mask)
            _put(sheet, tile, (state * 189, i * 93))
    return sheet


def wonders_window():
    img = Image.new("RGB", (1024, 768))
    img.paste(screens.stone((1024, 768), 41, screens.LEATHER), (0, 0))
    inner = parchment((914, 645), 42)
    frame(inner, (0, 0, 913, 644), "bronze", 5)
    img.paste(inner, (56, 69))
    return screens.from_rgb(img)


def wonders_card(hidden):
    """370x200: parchment card with a 190x132 picture well at (162,47) and a
    66x47 notch at its top right for the eye button."""
    well_box = (162, 47, 351, 178)
    notch = (285, 47, 351, 93)
    if hidden:
        sheet = blank((370, 200))
        art = screens.stone((190, 132), 43, screens.SLATE)
        d = ImageDraw.Draw(art)
        for k in range(-132, 190, 14):
            d.line([(k, 132), (k + 132, 0)], fill=(86, 94, 110))
        tile = art.convert("RGBA")
        ImageDraw.Draw(tile).rectangle((notch[0] - 162, 0, 189, notch[3] - 47), fill=(0, 0, 0, 0))
        sheet.alpha_composite(tile, (162, 47))
        return sheet
    img = parchment((370, 200), 44)
    frame(img, (0, 0, 369, 199), "bronze", 4)
    well(img, well_box, 0.5)
    img.paste(parchment((notch[2] - notch[0] + 1, notch[3] - notch[1] + 1), 45), (notch[0], notch[1]))
    return screens.from_rgb(img)


def wonders_eye():
    sheet = blank((128, 145))
    for i in range(3):
        base = plate(66, 47, "stone" if i < 2 else "bronze", i, 14)
        d = ImageDraw.Draw(base)
        pts = [(33 + 20 * math.cos(math.radians(a)), 23.5 + 10 * math.sin(math.radians(a))) for a in range(0, 360, 12)]
        d.polygon(pts, fill=(246, 244, 236), outline=(40, 30, 28))
        d.ellipse((26, 17, 40, 30), fill=(70, 110, 170), outline=(30, 24, 26))
        d.ellipse((31, 21, 36, 26), fill=(20, 18, 24))
        _put(sheet, base, (1, 1 + 48 * i))
    return sheet


def science_nav():
    sheet = blank((198, 200))
    for i in range(3):
        _put(sheet, plate(129, 34, "bronze", i, 7), (0, 34 * i))
    for x, direction in ((0, "left"), (45, "right")):
        tile = blank((45, 10))
        d = ImageDraw.Draw(tile)
        pts = [(36, 1), (43, 5), (36, 9)] if direction == "right" else [(8, 1), (1, 5), (8, 9)]
        d.line([(3, 5), (41, 5)] if direction == "right" else [(4, 5), (42, 5)], fill=INK + (255,), width=2)
        d.polygon(pts, fill=INK + (255,))
        _put(sheet, tile, (x, 102))
    return sheet


def exitbox():
    sheet = blank((216, 48))
    for i in range(3):
        _put(sheet, plate(72, 48, "bronze", i, 10, lambda d, s, b: screens.glyph_x(d, s, b, INK, 3.0, 0.32)), (72 * i, 0))
    return sheet


def popup_borders():
    """500x300: the 187x136 parchment popup panel at (250, 0)."""
    sheet = blank((500, 300))
    img = parchment((187, 136), 51)
    d = ImageDraw.Draw(img)
    green = (38, 74, 46)
    d.rectangle((5, 5, 181, 130), outline=green)
    d.rectangle((8, 8, 178, 127), outline=green)
    for cx, cy in ((11, 11), (175, 11), (11, 124), (175, 124)):
        d.ellipse((cx - 3, cy - 3, cx + 3, cy + 3), fill=(190, 150, 70), outline=green)
    sheet.alpha_composite(screens.from_rgb(img), (250, 0))
    return sheet


def bullets():
    sheet = blank((109, 21))
    for i in range(3):
        tile = plate(19, 20, "stone" if i == 0 else "bronze", 1 if i == 1 else 0, 9)
        d = ImageDraw.Draw(tile)
        d.ellipse((4, 5, 14, 15), fill=(70, 50, 34, 255) if i != 2 else (210, 60, 44, 255))
        _put(sheet, tile, (36 * i + 1, 1))
    return sheet


def pulldown():
    sheet = blank((22, 43))
    for y, state in ((0, 1), (22, 0)):
        _put(sheet, plate(21, 21, "bronze", state, 4, lambda d, s, b: screens.glyph_tri(d, s, b, "down", INK, 0.3)), (1, y))
    return sheet


def scroll_parts():
    sheet = blank((540, 455))
    for (x, y), direction, tone in (((112, 14), "up", "green"), ((128, 14), "up", "red"),
                                    ((112, 0), "down", "green"), ((128, 0), "down", "red")):
        _put(sheet, plate(18, 16, tone, 0, 4, lambda d, s, b, dr=direction: screens.glyph_tri(d, s, b, dr, (240, 244, 236), 0.3)), (x, y))
    track = Image.new("RGB", (14, 20), (70, 54, 40))
    d = ImageDraw.Draw(track)
    for y in range(0, 20, 4):
        d.line([(1, y), (12, y)], fill=(112, 88, 60))
        d.line([(1, y + 1), (12, y + 1)], fill=(40, 30, 22))
    d.line([(0, 0), (0, 19)], fill=(34, 24, 18))
    d.line([(13, 0), (13, 19)], fill=(34, 24, 18))
    sheet.alpha_composite(screens.from_rgb(track), (14, 138))
    return sheet


def advisor_tabs():
    """300x500: six advisors (domestic, trade, military, foreign, culture,
    science) x idle, rollover, active."""
    sheet = blank((300, 500))
    glyphs = (pictos.palace, pictos.i_coin, pictos.barracks, pictos.globe, pictos.masks, pictos.flask)
    for r, fn in enumerate(glyphs):
        g = _glyph(fn, 32)
        for s in range(3):
            base = plate(52, 52, "stone" if s == 0 else "bronze", s, 9)
            base.alpha_composite(g, (10, 10 + (1 if s == 2 else 0)))
            _put(sheet, base, (s * 56 + 2, r * 56 + 2))
    return sheet


# --------------------------------------------------------------- portraits

def pop_heads():
    """500x1000: 10 x 20 cells of 50 px (49 px drawn at +1,+1). Rows 0-15 are
    four eras of (smiling, content, unhappy, angry); 16-19 are the idle
    citizens (jester, tax collector, scientist, worker)."""
    sheet = blank((500, 1000))
    moods = ("happy", "content", "unhappy", "angry")
    spec = {16: ("jester", "content"), 17: ("visor", "content"), 18: ("cap", "content"), 19: ("none", "content")}
    for r in range(20):
        for c in range(10):
            if r < 16:
                era, mood = r // 4, moods[r % 4]
                face = portraits.Face(f"head{c}", gear=("none", "cap", "none", "none")[c % 4] if era else "none", era=era)
            else:
                gear, mood = spec[r]
                face = portraits.Face(f"head{c}", gear=gear, era=2, glasses=(r == 18))
            tile = portraits.render(face, 49, 49, 1.5, mood=mood, gy=47, outline=(34, 24, 24))
            _put(sheet, tile, (c * 50 + 1, r * 50 + 1))
    return sheet


def advisor_portraits(who):
    """800x800: 150 px cells, a row an era, a column a mood."""
    sheet = blank((800, 800))
    moods = ("happy", "content", "unhappy", "angry", "content")
    for era in range(4):
        for c, mood in enumerate(moods):
            face = portraits.Face(f"advisor-{who}", gear=("laurel", "cap", "hat", "none")[era] if who == "SCIENCE" else
                                  ("crown", "turban", "hat", "none")[era], era=era,
                                  glasses=(who == "SCIENCE" and era >= 2), long_hair=(who == "DOMESTIC"),
                                  beard=(who == "SCIENCE" and era in (0, 1)))
            bg = portraits.backdrop((150, 150), f"{who}{era}").convert("RGBA")
            bust = portraits.render(face, 150, 150, 5.6, mood=mood, gy=148)
            bg.alpha_composite(bust)
            ImageDraw.Draw(bg).rectangle((0, 0, 149, 149), outline=(54, 36, 22, 255))
            _put(sheet, bg, (c * 150, era * 150))
    return sheet


ARCHETYPES = 16
ARCH_GEAR = ("crown", "laurel", "turban", "helmet", "hat", "plume", "feathers", "cap")
LEADER_ERA = {"": 0, "A": 0, "B": 1, "C": 2, "D": 3}


def leader_frames(arch, era, n):
    """`n` idle frames (RGB 200x240) of one leader archetype: a blink and a
    slow nod of the head, so a frame differs from the last only around it."""
    face = portraits.Face(f"leader{arch}", gear=ARCH_GEAR[arch % 8], era=era, beard=bool(arch // 8))
    bg = portraits.backdrop((200, 240), f"arch{arch}")
    out = []
    for i in range(n):
        dz = (0.0, 0.2, 0.35, 0.2, 0.0, -0.1)[i % 6]
        dx = (0.0, 0.1, 0.25, 0.3, 0.15, 0.0)[i % 6]
        bust = portraits.render(face, 200, 240, 8.2, mood="happy" if i == n - 1 else "content",
                                blink=(i == 3), dx=dx, dz=dz, gy=238)
        frame_img = bg.copy().convert("RGBA")
        frame_img.alpha_composite(bust)
        out.append(frame_img.convert("RGB"))
    return out


def palette_for(frames):
    """One adaptive 256-colour palette shared by every frame of a clip."""
    n = len(frames)
    strip = Image.new("RGB", (frames[0].width * n, frames[0].height))
    for i, f in enumerate(frames):
        strip.paste(f, (i * f.width, 0))
    return strip.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)


# ------------------------------------------------------------ diplomacy etc.

def diplomacy_screen(seed):
    """1024x768 frame around the 200x240 leader hole at (411, 59)."""
    img = Image.new("RGB", (1024, 768))
    img.paste(screens.stone((1024, 768), seed, screens.LEATHER), (0, 0))
    inner = parchment((944, 90), seed + 1)
    frame(inner, (0, 0, 943, 89), "bronze", 4)
    img.paste(inner, (40, 620))
    frame(img, (405, 53, 616, 304), "bronze", 6)
    text = parchment((540, 300), seed + 2)
    frame(text, (0, 0, 539, 299), "bronze", 4)
    img.paste(text, (242, 318))
    return screens.from_rgb(img, hole=(411, 59, 610, 298))


def diplomacy_arrow(up):
    return plate(26, 24, "bronze", 0, 6, lambda d, s, b: screens.glyph_tri(d, s, b, "up" if up else "down", INK, 0.3))


def wonder_frame():
    """1024x768 splash: a 320x320 well at (351,109) and a text panel."""
    img = Image.new("RGB", (1024, 768))
    img.paste(screens.stone((1024, 768), 61, screens.LEATHER), (0, 0))
    frame(img, (345, 103, 676, 434), "bronze", 6)
    text = parchment((544, 241), 62)
    frame(text, (0, 0, 543, 240), "bronze", 4)
    img.paste(text, (240, 460))
    return screens.from_rgb(img, hole=(351, 109, 670, 428))


# ----------------------------------------------------------------- icons

def tech_icon(entry):
    return pictos.rgba(pictos.for_tech(entry), 32)


def wonder_splash(entry, size=320):
    """A 320x320 scene: sky, hills, ground and the monument's glyph, drawn at
    10x the icon scale so its outline reads as an ink line."""
    top = portraits._hue(entry, "sky", 0.35, 0.95)
    bot = F.tone(portraits._hue(entry, "sky2", 0.18, 0.98), 1.12)
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    horizon = int(size * 0.64)
    for y in range(size):
        t = min(1.0, y / horizon)
        d.line([(0, y), (size, y)], fill=tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)))
    sun = (int(size * (0.2 + 0.6 * ((portraits._h(entry, "sun") % 100) / 100))), int(size * 0.2))
    d.ellipse((sun[0] - 22, sun[1] - 22, sun[0] + 22, sun[1] + 22), fill=(255, 238, 170))
    for k, (hx, hw, hh, tone) in enumerate(((0.2, 0.7, 0.14, 0.78), (0.78, 0.8, 0.18, 0.66), (0.5, 0.9, 0.08, 0.9))):
        col = F.tone(portraits._hue(entry, "hill", 0.35, 0.66), tone)
        d.ellipse((size * hx - size * hw / 2, horizon - size * hh, size * hx + size * hw / 2, horizon + size * hh), fill=col)
    ground = F.tone(portraits._hue(entry, "ground", 0.38, 0.62), 0.9)
    for y in range(horizon, size):
        t = (y - horizon) / (size - horizon)
        d.line([(0, y), (size, y)], fill=tuple(int(ground[i] * (1.1 - 0.35 * t)) for i in range(3)))
    big = pictos.rgba(pictos.for_building(entry), int(size * 0.74))
    shadow = Image.new("L", img.size, 0)
    ImageDraw.Draw(shadow).ellipse((size * 0.17, size * 0.84, size * 0.83, size * 0.95), fill=110)
    img.paste((20, 24, 20), mask=shadow.filter(ImageFilter.GaussianBlur(5)))
    x, y = (size - big.width) // 2, int(size * 0.9) - big.height
    rgba = img.convert("RGBA")
    rgba.alpha_composite(big, (x, y))
    frame(rgba, (0, 0, size - 1, size - 1), "bronze", 3)
    return rgba.convert("RGB")
