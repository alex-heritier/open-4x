import sys, struct
from emu import *
import imports

def make_emu(verbose=False):
    e = Emu(verbose=verbose)
    imports.install(e)
    # allocator hooks: _nh_malloc(size, flag) and _free(p)
    e.hook_func(0x649fb9, lambda em, size, flag: em.alloc(size), nargs=2)
    e.hook_func(0x649ebe, lambda em, p: 0, nargs=1)
    return e

FAILED = []

def run_cpp_inits(e):
    tab = [struct.unpack('<I', e.rd(a, 4))[0] for a in range(0x680000, 0x6803C4, 4)]
    e.uc.reg_write(UC_X86_REG_ESP, STACK_TOP - 0x1000)
    for i, fn in enumerate(tab):
        if not fn:
            continue
        e.uc.reg_write(UC_X86_REG_ESP, STACK_TOP - 0x1000)
        try:
            e.call(fn)
        except Exception as ex:
            print(f'init #{i} fn={fn:#x} FAILED: {ex}', file=sys.stderr)
            FAILED.append((i, fn))

if __name__ == '__main__':
    e = make_emu(verbose='-v' in sys.argv)
    run_cpp_inits(e)
    print('inits done, heap used', hex(e.heap_ptr - HEAP_BASE), 'failed', [(i, hex(f)) for i, f in FAILED])
