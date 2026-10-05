#!/usr/bin/env python3
"""Deterministic original rectangles, indexed flat colours and silence, never install art.

Run from the repo root with Pillow installed. --biq FILE refreshes the reference
manifest using the existing Rust BIQ reader (no install or PediaIcons required).
Without --biq, uses the committed manifest. Output is ready to convert at checkout.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import wave
from PIL import Image, ImageDraw
import prep_assets as prep

ROOT = Path(__file__).resolve().parent.parent


def flc(w, h, frames):
    """An indexed FLC: one palette, flat BRUN rows, valid frame headers."""
    pal = bytes([1, 0, 0, 0]) + bytes(v for i in range(256) for v in ((60, 160, 100) if i != 255 else (255, 0, 255)))
    chunks = []
    for n in range(frames):
        row = bytearray([0])  # decoder counts pixels, not this obsolete packet count
        left = w
        while left:
            count = min(left, 127)
            row.extend([count, 64])
            left -= count
        brun = struct.pack('<IH', 6 + len(row) * h, 15) + row * h
        palette = struct.pack('<IH', 6 + len(pal), 4) + pal if n == 0 else b''
        body = palette + brun
        chunks.append(struct.pack('<IHH8x', 16 + len(body), 0xF1FA, 2 if n == 0 else 1) + body)
    head = bytearray(128)
    struct.pack_into('<IHHHHHHI', head, 0, 128 + sum(map(len, chunks)), 0xAF12, frames, w, h, 8, 3, 100)
    return bytes(head) + b''.join(chunks)


def block_font():
    """A tiny original TrueType font: ASCII maps to a rectangular outline.

    No font files or glyph outlines are copied. This tests font loading, not typography.
    """
    pack = lambda fmt, *v: struct.pack('>' + fmt, *v)
    glyph = pack('hhhhhHH', 1, 0, 0, 400, 700, 3, 0) + bytes([1]*4)
    glyph += pack('hhhhhhhh', 0, 400, 0, -400, 0, 0, 700, 0)
    glyph += b'\0' * (-len(glyph) % 4)
    head = pack('IIIIHHQQhhhhHHhhh', 0x10000, 0x10000, 0, 0x5F0F3CF5, 0, 1000, 0, 0, 0, 0, 400, 700, 0, 8, 2, 1, 0)
    hhea = pack('IhhhHhhhhhhhhhhhH', 0x10000, 800, -200, 0, 500, 0, 100, 400, 1, 0, 0, 0, 0, 0, 0, 0, 2)
    # cmap format 4: space -> empty glyph, printable ASCII -> block glyph 1.
    codes = list(range(32, 127)) + [65535]
    count = len(codes)
    sub = pack('HHHHHHH', 4, 16 + count*8, 0, count*2, 128, 6, count*2-128)
    sub += pack('H'*count, *codes) + pack('H', 0) + pack('H'*count, *codes)
    sub += pack('H'*count, *[((0 if c in (32,65535) else 1)-c) & 65535 for c in codes]) + bytes(count*2)
    tables = {'head':head, 'hhea':hhea, 'maxp':pack('IH13H',0x10000,2,4,1,*([0]*11)),
              'hmtx':pack('HhHh',500,0,500,0), 'loca':pack('III',0,0,len(glyph)),
              'glyf':glyph, 'cmap':pack('HHHHI',0,1,3,1,12)+sub,
              'name':pack('HHH',0,0,6), 'post':pack('IIhhIIIII',0x30000,0,0,0,0,0,0,0,0)}
    offset = 12 + len(tables)*16
    directory, data = b'', b''
    for tag, value in sorted(tables.items()):
        padded = value + bytes(-len(value)%4)
        checksum = sum(struct.unpack('>'+'I'*(len(padded)//4),padded)) & 0xffffffff
        directory += tag.encode()+pack('III',checksum,offset,len(value))
        data += padded
        offset += len(padded)
    font = bytearray(pack('IHHHH',0x10000,len(tables),128,3,len(tables)*16-128)+directory+data)
    head_offset = 12+len(tables)*16 + sum((len(v)+3)//4*4 for k,v in sorted(tables.items()) if k < 'head')
    checksum = sum(struct.unpack('>'+'I'*(len(font)//4),font)) & 0xffffffff
    struct.pack_into('>I',font,head_offset+8,(0xB1B0AFBA-checksum)&0xffffffff)
    return font


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument('--biq', type=Path)
    args.add_argument('--out', type=Path, default=ROOT/'test-assets')
    opt = args.parse_args()
    manifest = ROOT/'tools/stub_asset_refs.json'
    if opt.biq:
        result = subprocess.check_output(['cargo','run','--release','--manifest-path',str(ROOT/'civ3_utils/biq/Cargo.toml'),
                                         '--example','asset_refs','--',str(opt.biq.resolve())],text=True)
        refs = {k:[] for k in ('unit','leader','tech','wonder')}
        for line in result.splitlines():
            kind,*values = line.split('\t')
            refs[kind].append(values if kind == 'unit' else values[0])
        manifest.write_text(json.dumps(refs,indent=2)+'\n')
    refs = json.loads(manifest.read_text())
    out = opt.out
    def path_for(rel):
        path = out/rel
        if not path.resolve().is_relative_to(out.resolve()):
            raise ValueError(f"asset reference escapes output tree: {rel}")
        return path
    def write(rel, data):
        path = path_for(rel)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    def pcx(rel, size=(1024,1024), special=None):
        digest = hashlib.sha256(rel.encode()).digest()
        pal = [v for i in range(256) for v in (tuple(60+b%120 for b in digest[:3]) if i != 255 else (255,0,255))]
        pal[64*3:64*3+3] = [200,200,200]
        im = Image.new('P',size,64)
        im.putpalette(pal)
        draw = ImageDraw.Draw(im)
        if special:
            draw.rectangle((0,0,size[0],size[1]),fill=255)
            special(draw)
        path = path_for(rel)
        path.parent.mkdir(parents=True,exist_ok=True)
        im.save(path,format='PCX')
    def link(rel, target):
        path = path_for(rel)
        path.parent.mkdir(parents=True,exist_ok=True)
        if path.exists() or path.is_symlink(): path.unlink()
        path.symlink_to(os.path.relpath(out/target,path.parent))
    write('civ3PTW/README.md', b'Synthetic search-path directory; source media fall back to the base root.\n')
    terrain = ['xggc','xtgc','xpgc','xdgc','xdpc','xdgp','wCSO','wOOO','wSSS','polarICEcaps-final',
               'xhills','Mountains','Mountains-snow','grassland forests','plains forests','tundra forests',
               'deltaRivers','mtnRivers','goodyhuts','roads','FogOfWar']+[p[1] for p in prep.IRRIGATION_SHEETS]
    def canopies(d):
        for y in [40,420,770]:
            for x in [30,200]: d.rectangle((x,y,x+100,y+65),fill=64)
    for name in terrain:
        pcx(f'Art/Terrain/{name}.pcx',(2048,1024),canopies if 'forests' in name else None)
    def borders(d):
        for row,(x,y) in enumerate([(20,10),(90,10),(20,50),(90,50)]):
            d.rectangle((x,y+row*72,x+12,y+row*72+12),fill=64)
    pcx('Art/Terrain/Territory.pcx',(256,288),borders)
    pcx('Conquests/Art/Terrain/TerrainBuildings.PCX',(512,256))
    for n in ['rAMER','rEURO','rROMAN','rMIDEAST','rASIAN','AMERWALL','EUROWALL','ROMANWALL','MIDEASTWALL','ASIANWALL','city icons']:
        pcx(f'Art/Cities/{n}.PCX',(501,380))
    city = ['buildings-small','CityIcons','background','ProductionQueueBox','XandView','ProdButton','HurryButton',
            'cityMgmtButtons','TopFadeBar','BottomFadeBar','TopFadeBarAlpha','BottomFadeBarAlpha']
    def exits(d):
        for x in [5,48,91]: d.rectangle((x,55,x+25,85),fill=64)
    for n in city: pcx(f'Art/city screen/{n}.pcx',special=exits if n=='XandView' else None)
    for p in ['Art/SmallHeads/popHeads.pcx','Art/scroll.pcx','Art/resources.pcx','Art/leaderheads/TO.pcx',
              'Art/Units/units_32.pcx','Conquests/Art/Units/units_32.pcx']:
        pcx(p)
    for n in range(32): pcx(f'Art/Units/Palettes/ntp{n:02}.pcx',(16,16))
    for stem in ['box right','nextturn states']:
        for kind in ['color','alpha']: pcx(f'Art/interface/{stem} {kind}.pcx',(294,137) if stem=='box right' else (141,28))
    for n in ['ButtonAlpha.pcx']+[p[0] for p in prep.BTN_SHEETS]: pcx('Conquests/Art/interface/'+n,(256,320))
    advisors = ['science_ancient','science_middle','science_industrial_new','science_modern','domestic','dialogbox',
                'non_required','advisor_EXIT','domesticBUTTON','domestic_icons_aux','domestic_plusminus','domestic_icons',
                'wonders_background','wondersBOX','wondersBOXoverlay']
    for n in advisors: pcx(f'Art/Advisors/{n}.pcx')
    def boxes(d):
        for y in range(16):
            for x in range(4): d.rectangle((x*189,y*40,x*189+180,y*40+30),fill=64)
    pcx('Art/Advisors/techboxes.pcx',(760,640),boxes)
    for p in ['Art/Tech Chooser/scienceNAV.pcx','Art/exitBox-backgroundStates.pcx','Art/popupborders.pcx',
              'Art/X-o_ALLstates-sprite.pcx','Art/pulldownArrows.pcx','Art/SmallHeads/popupDOMESTIC.pcx',
              'Art/SmallHeads/popupSCIENCE.pcx','Art/SmallHeads/advisor_tab.pcx','Art/interface/wondersEye.pcx',
              'Art/Wonder Splash/wonderBackground.pcx']:
        pcx(p)
    for n in ['talk_offer','consider','counter','uparrow','downarrow']: pcx(f'Art/Diplomacy/{n}.pcx')
    write('placeholder-unit.flc',flc(32,32,8))
    write('placeholder-leader.flc',flc(200,240,2))
    write('Art/Animations/Cursor/Cursor.flc',flc(93,46,2))
    # WAV silence is also used as the MP3 input: ffmpeg probes the file contents.
    path = out/'silence.wav'
    with wave.open(str(path),'wb') as wav:
        wav.setparams((1,2,8000,0,'NONE','not compressed'))
        wav.writeframes(bytes(1600))
    for n in prep.UI_SOUNDS: link('Sounds/'+n,'silence.wav')
    for n,_ in prep.MUSIC: link('Sounds/'+n,'silence.wav')
    pedia = []
    for entry,name in refs['unit']:
        pedia += ['#ANIMNAME_'+entry,name]
        folder = f'Art/Units/{name}'
        ini = '[Animations]\n'+''.join(f'{s}=placeholder.flc\n' for s in prep.UNIT_SLOTS)
        ini += '\n[Sound Effects]\n'+''.join(f'{s}=silence.wav\n' for s in sorted(prep.SOUND_SLOTS))
        write(folder+'/'+name+'.ini',ini.encode())
        link(folder+'/placeholder.flc','placeholder-unit.flc')
        link(folder+'/silence.wav','silence.wav')
    for p in sorted(set(refs['leader'])): link(p.replace('\\','/'),'placeholder-leader.flc')
    pcx('Art/placeholder-tech.pcx',(32,32))
    pcx('Art/placeholder-wonder.pcx',(320,320))
    for entry in refs['tech']: pedia += ['#'+entry,'Art/placeholder-tech.pcx']
    for entry in refs['wonder']: pedia += ['#WON_SPLASH_'+entry,'Art/placeholder-wonder.pcx']
    write('Text/PediaIcons.txt',('\n'.join(pedia)+'\n').encode())
    write('Text/diplomacy.txt',b'#HELLO\n#random 1\nPlaceholder greeting.\n')
    write('LSANS.TTF',block_font())
    files = [p for p in out.rglob('*') if p.is_file()]
    print(f'{out}: {len(files)} files, {sum(p.lstat().st_size for p in files):,} bytes (symlinks counted once)')


if __name__ == '__main__': main()
