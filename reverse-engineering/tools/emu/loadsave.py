import sys, struct, time
from emu import *
from run_init import make_emu, run_cpp_inits, FAILED
import vfs

BUF_BASE = 0x50000000

def setup(raw_path, verbose=False):
    e = make_emu(verbose)
    vfs.install(e)
    run_cpp_inits(e)
    data = open(raw_path, 'rb').read()
    assert data[:4] == b'CIV3', data[:8]
    size = (len(data) + 0xFFF + 0x10000) & ~0xFFF
    e.uc.mem_map(BUF_BASE, size, UC_PROT_ALL)
    e.wr(BUF_BASE, data)
    e.data = data
    ver, sub = struct.unpack_from('<II', data, 6)
    e.w32(0xA32BC4, ver)
    e.w32(0xA32BC8, sub)
    e.hdr = 6 + 8 + (16 if sub >= 7 else 0)
    e.hook_func(0x5B04F0, lambda em: 0, nargs=0)
    e.hook_func(0x5506A0, lambda em: 0, nargs=0)
    e.hook_func(0x59A760, lambda em, a: 0, nargs=1, callee_pops=True)
    e.hook_func(0x598E60, lambda em, a, b, c, d: 0, nargs=4, callee_pops=True)
    e.hook_func(0x4062A0, lambda em, *a: 0, nargs=6, callee_pops=True)
    e.w32(0x9FD5B0, 1)
    e.hook_func(0x4C0AC0, lambda em, a, b, c: 0, nargs=3, callee_pops=True)  # city sprite attach
    # UI sinks
    e.hook_func(0x558650, lambda em, a, b, c: 0, nargs=3, callee_pops=True)
    e.hook_func(0x558700, lambda em, a, b: 0, nargs=2, callee_pops=True)
    return e

def set_counts(e, counts):
    for addr, v in counts.items():
        e.w32(addr, v)

if __name__ == '__main__':
    path = sys.argv[1]
    e = setup(path, verbose='-v' in sys.argv)
    e.uc.reg_write(UC_X86_REG_ESP, STACK_TOP - 0x1000)
    e.trace_blocks()
    t = time.time()
    r = e.call(0x590030, args=(BUF_BASE + e.hdr, 0))
    print('game_data ->', hex(r), 'expected', hex(len(e.data) - e.hdr), 'in', round(time.time() - t, 1), 's')
