"""Run the game's own game_data(load) over a .SAV.raw and record a ground-truth parse trace."""
import sys, struct, time, pickle, os
from emu import *
import loadsave
from loadsave import setup, BUF_BASE


class Tracer:
    def __init__(self, e):
        self.e = e
        self.ev = []          # ordered events
        self.cur = None       # coalescing raw read run [pc, start, end, edi0]
        self.nreads = 0
        n = len(e.data)
        self.e_n = n
        self.done = False
        uc = e.uc
        uc.hook_add(UC_HOOK_MEM_READ, self.on_read, begin=BUF_BASE, end=BUF_BASE + n - 1)
        self.watch(0x4FCAB0, self.on_dispatch)
        self.watch(0x4FCBB0, self.on_chunk)
        self.watch(0x4FCB10, self.on_size)
        # wrap allocator hook to log
        old = e.alloc
        def alloc(size, zero=True, _old=old):
            p = _old(size, zero)
            ra = e.u32(e.reg('esp') + 4) if False else 0
            self.flush()
            self.ev.append(('A', p, size, self.caller()))
            return p
        e.alloc = alloc

    def caller(self):
        e = self.e
        sp = e.reg('esp')
        # we are inside a python func hook: the return address is at [esp]
        try:
            return e.u32(sp)
        except Exception:
            return 0

    def watch(self, addr, fn):
        def cb(uc, a, size, ud):
            fn()
        self.e.uc.hook_add(UC_HOOK_CODE, cb, begin=addr, end=addr)

    def flush(self):
        if self.cur is not None:
            self.ev.append(('R',) + tuple(self.cur))
            self.cur = None

    def on_read(self, uc, access, address, size, value, ud):
        pc = uc.reg_read(UC_X86_REG_EIP)
        off = address - BUF_BASE
        if off + size >= self.e_n:
            self.done = True
            uc.emu_stop()
        c = self.cur
        if c is not None and c[0] == pc and c[2] == off:
            c[2] = off + size
            return
        self.flush()
        self.cur = [pc, off, off + size, uc.reg_read(UC_X86_REG_EDI)]

    def on_dispatch(self):
        e = self.e
        self.flush()
        sp = e.reg('esp')
        obj = e.reg('ecx')
        save, buf = e.u32(sp + 4), e.u32(sp + 8)
        vt = e.u32(obj) if obj else 0
        self.ev.append(('D', obj, vt, buf - BUF_BASE, e.u32(sp)))

    def on_chunk(self):
        e = self.e
        self.flush()
        sp = e.reg('esp')
        obj = e.reg('ecx')
        buf = e.u32(sp + 4)
        off = buf - BUF_BASE
        tag = e.rd(buf, 4)
        sz = e.u32(buf + 4)
        d0, d1 = e.u32(obj + 0x14), e.u32(obj + 0x18)
        self.ev.append(('C', obj, off, tag, sz, d0, d1, e.u32(obj), e.u32(sp)))

    def on_size(self):
        pass


def run(path, quiet=False):
    e = setup(path)
    tr = Tracer(e)
    e.uc.reg_write(UC_X86_REG_ESP, STACK_TOP - 0x1000)
    t = time.time()
    r = e.call(0x590030, args=(BUF_BASE + e.hdr, 0), timeout_us=300_000_000)
    tr.flush()
    last = [x for x in tr.ev if x[0] == 'R'][-1]
    r = last[3] - e.hdr  # end of last read (stream bytes consumed)
    if not quiet:
        print(os.path.basename(path), 'game_data ->', hex(r), 'expected', hex(len(e.data) - e.hdr), 'OK' if r == len(e.data) - e.hdr else 'MISMATCH', 'events', len(tr.ev), round(time.time() - t, 1), 's')
    return e, tr, r


if __name__ == '__main__':
    # usage: trace.py STREAM.raw [OUT.pkl]   (STREAM from `sav FILE --stream OUT`)
    path = sys.argv[1]
    e, tr, r = run(path)
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.splitext(path)[0] + '.trace.pkl'
    snap = {'sm': e.rd(0x9C3508, 0x1000), 'game': e.rd(0xA52658, 0x600)}
    pickle.dump({'ev': tr.ev, 'ret': r, 'hdr': e.hdr, 'n': len(e.data), 'snap': snap}, open(out, 'wb'))
    print('wrote', out)
