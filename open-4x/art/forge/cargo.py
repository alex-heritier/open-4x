"""Original procedural cargo cart art; five clips share the existing eight-facing contract."""
from pathlib import Path
from PIL import Image, ImageDraw

def frame(facing=0, phase=0, clip='idle'):
    scale=3
    im=Image.new('RGBA',(160*scale,160*scale))
    d=ImageDraw.Draw(im)
    offset=2 if clip=='run' and phase%2 else 0
    def pts(points): return [(int(x*scale),int((y+offset)*scale)) for x,y in points]
    def polygon(points,color): d.polygon(pts(points),fill=color)
    def line(points,color,width=2): d.line(pts(points),fill=color,width=width*scale)
    d.ellipse((37*scale,107*scale,124*scale,128*scale),fill=(28,23,15,55))
    polygon([(39,94),(81,73),(124,95),(82,117)],'#524237')
    for x in [54,106]:
        d.ellipse(((x-7)*scale,(104+offset)*scale,(x+7)*scale,(124+offset)*scale),fill='#302c28',outline='#8b7962',width=2*scale)
        line([(x,106),(x,121)],'#b9a27b',1)
    for shift in [0,-24]:
        polygon([(48,80+shift),(81,64+shift),(114,81+shift),(81,98+shift)],'#ba9563')
        polygon([(48,80+shift),(81,98+shift),(81,118+shift),(48,100+shift)],'#8b653e')
        polygon([(81,98+shift),(114,81+shift),(114,101+shift),(81,118+shift)],'#624a32')
        line([(58,75+shift),(91,92+shift),(91,113+shift)],'#46382b',3)
        line([(101,75+shift),(68,91+shift),(68,112+shift)],'#4c3c2b',3)
        for y in [86,94]:line([(49,y+shift),(79,y+18+shift)],'#a37e4c',1)
    # Direction changes the cart's tow bar while the crate silhouette stays readable.
    directions=[(-1,1),(0,1),(1,1),(1,0),(1,-1),(0,-1),(-1,-1),(-1,0)]
    dx,dy=directions[facing]
    line([(81,114),(81+dx*32,114+dy*14)],'#675440',3)
    if clip=='death' and phase: im.putalpha(im.getchannel('A').point(lambda a:a//3))
    return im.resize((160,160),Image.Resampling.LANCZOS)

def build(root=None):
    root=Path(root) if root else Path(__file__).resolve().parents[2]/'assets/packs/base'
    (root/'units/crate').mkdir(parents=True,exist_ok=True)
    frame().save(root/'sprites/crate.png')
    for clip in ['idle','run','attack','victory','death']:
        sheet=Image.new('RGBA',(320,1280))
        for facing in range(8):
            for phase in range(2):sheet.paste(frame(facing,phase,clip),(phase*160,facing*160))
        sheet.save(root/f'units/crate/{clip}.png')

if __name__=='__main__':build()
