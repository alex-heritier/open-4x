"""Virtual filesystem for the emulator: Win32 file APIs + CRT stdio."""
import os, struct, sys

class VFile:
    def __init__(self, name, data=None):
        self.name = name
        self.data = bytearray(data or b'')

class Handle:
    def __init__(self, f, pos=0):
        self.f = f
        self.pos = pos

def install(e):
    e.vfs = {}          # lower name -> VFile
    e.handles = {}      # win32 handle -> Handle
    e.maps = {}         # mapping handle -> VFile ; view addr -> (VFile, len)
    e.views = {}
    e.files = {}        # FILE* -> Handle
    e._next_h = 0x100

    def newh():
        e._next_h += 4
        return e._next_h

    def key(p):
        return e.cstr(p).replace('\\', '/').lower()

    # ---- win32
    def CreateFileA(em, name, access, share, sec, disp, flags, tmpl):
        k = key(name)
        if disp in (1, 2):      # CREATE_NEW / CREATE_ALWAYS
            em.vfs[k] = VFile(k)
        elif k not in em.vfs:
            return 0xFFFFFFFF
        h = newh()
        em.handles[h] = Handle(em.vfs[k])
        return h

    def SetFilePointer(em, h, lo, hip, method):
        hd = em.handles[h]
        lo = lo - (1 << 32) if lo >= (1 << 31) else lo
        if method == 0:
            hd.pos = lo
        elif method == 1:
            hd.pos += lo
        else:
            hd.pos = len(hd.f.data) + lo
        return hd.pos

    def SetEndOfFile(em, h):
        hd = em.handles[h]
        n = hd.pos
        if len(hd.f.data) < n:
            hd.f.data.extend(b'\0' * (n - len(hd.f.data)))
        else:
            del hd.f.data[n:]
        return 1

    def CreateFileMappingA(em, h, sec, prot, hi, lo, name):
        m = newh()
        em.maps[m] = em.handles[h].f
        return m

    def MapViewOfFile(em, m, access, hi, lo, size):
        f = em.maps[m]
        p = em.alloc(len(f.data) + 16)
        em.wr(p, bytes(f.data))
        em.views[p] = (f, len(f.data))
        return p

    def UnmapViewOfFile(em, p):
        f, n = em.views.pop(p)
        f.data[:] = em.rd(p, n)
        return 1

    def GetFileSize(em, h, hip):
        return len(em.handles[h].f.data)

    def ReadFile(em, h, buf, n, nread, ov):
        hd = em.handles[h]
        d = bytes(hd.f.data[hd.pos:hd.pos + n])
        em.wr(buf, d)
        hd.pos += len(d)
        if nread:
            em.w32(nread, len(d))
        return 1

    def CloseHandle(em, h):
        em.handles.pop(h, None)
        return 1

    def GetFileAttributesA(em, p):
        return 0x80 if key(p) in em.vfs else 0xFFFFFFFF

    # ---- stdio (CRT)
    def fopen(em, path, mode):
        k = key(path)
        if k not in em.vfs and k.endswith('conquests.biq'):
            # the game loads its default rules from conquests.biq next to the exe;
            # give it the *decoded* stream (`dump FILE --stream OUT`)
            em.vfs[k] = VFile(k, open(os.environ['CIV3_DEFAULT_BIQ'], 'rb').read())
        if k not in em.vfs:
            if em.verbose:
                print(f'[vfs] fopen miss {k}', file=sys.stderr)
            return 0
        fp = em.alloc(32)
        em.files[fp] = Handle(em.vfs[k])
        return fp

    def fread(em, buf, size, cnt, fp):
        hd = em.files[fp]
        want = size * cnt
        d = bytes(hd.f.data[hd.pos:hd.pos + want])
        em.wr(buf, d)
        hd.pos += len(d)
        return len(d) // size if size else 0

    def fseek(em, fp, off, whence):
        hd = em.files[fp]
        off = off - (1 << 32) if off >= (1 << 31) else off
        if whence == 0:
            hd.pos = off
        elif whence == 1:
            hd.pos += off
        else:
            hd.pos = len(hd.f.data) + off
        return 0

    def ftell(em, fp):
        return em.files[fp].pos

    def fclose(em, fp):
        em.files.pop(fp, None)
        return 0

    I = e.import_impl
    I['CreateFileA'] = (7, CreateFileA)
    I['SetFilePointer'] = (4, SetFilePointer)
    I['SetEndOfFile'] = (1, SetEndOfFile)
    I['CreateFileMappingA'] = (6, CreateFileMappingA)
    I['MapViewOfFile'] = (5, MapViewOfFile)
    I['UnmapViewOfFile'] = (1, UnmapViewOfFile)
    I['GetFileSize'] = (2, GetFileSize)
    I['ReadFile'] = (5, ReadFile)
    I['CloseHandle'] = (1, CloseHandle)
    I['GetFileAttributesA'] = (1, GetFileAttributesA)
    e.hook_func(0x64B09F, fopen, nargs=2)
    e.hook_func(0x64B3E3, fread, nargs=4)
    e.hook_func(0x64C47B, fseek, nargs=3)
    e.hook_func(0x64C2F8, ftell, nargs=1)
    e.hook_func(0x64AEB8, fclose, nargs=1)
