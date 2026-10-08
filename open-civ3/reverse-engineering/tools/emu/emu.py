"""Unicorn harness for Civ3Conquests.exe (32-bit x86).

Maps the PE image, stubs the Win32 imports with Python thunks, replaces the
CRT allocator with a bump allocator, and lets us call arbitrary exe
functions (thiscall/cdecl/stdcall) on the real global objects.  Used to run
the game's own save *load* routines over real .SAV files and record exactly
which input bytes each instruction consumes.
"""
import os
import struct, sys, collections
import pefile
from unicorn import *
from unicorn.x86_const import *
from capstone import Cs, CS_ARCH_X86, CS_MODE_32

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
EXE = os.environ.get('CIV3_EXE') or os.path.join(
    REPO, '..', 'civ3', 'civ3-gog', 'app', 'Conquests', 'Civ3Conquests.exe')
BASE = 0x400000

THUNK_BASE = 0x0F000000
HEAP_BASE = 0x20000000
HEAP_SIZE = 0x20000000
STACK_TOP = 0x7F800000
STACK_SIZE = 0x800000
TEB = 0x7FFDF000
PEB = 0x7FFD0000
RET_SENTINEL = 0x0E000000


class Stop(Exception):
    pass


class Emu:
    def __init__(self, exe=EXE, verbose=False):
        self.verbose = verbose
        self.pe = pefile.PE(exe)
        self.uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.md = Cs(CS_ARCH_X86, CS_MODE_32)
        self.heap_ptr = HEAP_BASE + 0x1000
        self.func_hooks = {}
        self.imports = {}      # thunk addr -> (dll, name)
        self.import_impl = {}  # name -> (nargs, fn)
        self.tls = {}
        self.tls_next = 1
        self.block_ring = collections.deque(maxlen=64)
        self.read_log = None
        self.log = []
        self._map()
        self._setup_fs()
        self._hook_imports()
        self.uc.hook_add(UC_HOOK_MEM_UNMAPPED, self._unmapped)
        self.uc.mem_map(RET_SENTINEL, 0x1000)
        self.uc.mem_write(RET_SENTINEL, b'\x90' * 16)
        self.uc.hook_add(UC_HOOK_CODE, self._sentinel, begin=RET_SENTINEL, end=RET_SENTINEL + 0xF)
        self.alloc_sizes = {}

    # ------------------------------------------------------------ image
    def _map(self):
        pe = self.pe
        uc = self.uc
        size = (pe.OPTIONAL_HEADER.SizeOfImage + 0xFFF) & ~0xFFF
        uc.mem_map(BASE, size, UC_PROT_ALL)
        uc.mem_write(BASE, pe.header[:pe.OPTIONAL_HEADER.SizeOfHeaders])
        for s in pe.sections:
            data = s.get_data()[:s.SizeOfRawData]
            uc.mem_write(BASE + s.VirtualAddress, data)
        uc.mem_map(HEAP_BASE, HEAP_SIZE, UC_PROT_ALL)
        uc.mem_map(STACK_TOP - STACK_SIZE, STACK_SIZE, UC_PROT_ALL)
        uc.mem_map(TEB, 0x1000, UC_PROT_ALL)
        uc.mem_map(PEB, 0x1000, UC_PROT_ALL)
        uc.mem_map(THUNK_BASE, 0x10000, UC_PROT_ALL)

    def _setup_fs(self):
        uc = self.uc
        uc.mem_write(TEB + 0x00, struct.pack('<I', 0xFFFFFFFF))  # SEH chain end
        uc.mem_write(TEB + 0x04, struct.pack('<I', STACK_TOP))
        uc.mem_write(TEB + 0x08, struct.pack('<I', STACK_TOP - STACK_SIZE))
        uc.mem_write(TEB + 0x18, struct.pack('<I', TEB))
        uc.mem_write(TEB + 0x30, struct.pack('<I', PEB))
        # PEB: image base at +8
        uc.mem_write(PEB + 8, struct.pack('<I', BASE))
        # GDT with a flat data segment whose base is TEB, used for FS
        GDT = 0x7FFC0000
        uc.mem_map(GDT, 0x1000)
        def desc(base, limit, access, flags):
            return struct.pack('<HHBBBB', limit & 0xFFFF, base & 0xFFFF, (base >> 16) & 0xFF,
                               access, ((limit >> 16) & 0xF) | (flags << 4), (base >> 24) & 0xFF)
        gdt = desc(0, 0, 0, 0)
        gdt += desc(0, 0xFFFFF, 0x9B, 0xC)       # 0x08 code
        gdt += desc(0, 0xFFFFF, 0x93, 0xC)       # 0x10 data
        gdt += desc(TEB, 0xFFF, 0xF3, 0x4)       # 0x18 fs (dpl3)
        uc.mem_write(GDT, gdt)
        uc.reg_write(UC_X86_REG_GDTR, (0, GDT, len(gdt) - 1, 0))
        uc.reg_write(UC_X86_REG_CS, 0x08)
        uc.reg_write(UC_X86_REG_DS, 0x10)
        uc.reg_write(UC_X86_REG_ES, 0x10)
        uc.reg_write(UC_X86_REG_SS, 0x10)
        uc.reg_write(UC_X86_REG_FS, 0x18 | 3)

    # ------------------------------------------------------------ imports
    def _hook_imports(self):
        uc = self.uc
        idx = 0
        for entry in self.pe.DIRECTORY_ENTRY_IMPORT:
            dll = entry.dll.decode()
            for imp in entry.imports:
                name = imp.name.decode() if imp.name else f'ord{imp.ordinal}'
                taddr = THUNK_BASE + idx * 16
                idx += 1
                uc.mem_write(imp.address, struct.pack('<I', taddr))
                uc.mem_write(taddr, b'\xC3')  # ret (never executed; hook intercepts)
                self.imports[taddr] = (dll, name)
        self.uc.hook_add(UC_HOOK_CODE, self._import_hook, begin=THUNK_BASE, end=THUNK_BASE + 0xFFFF)

    def _import_hook(self, uc, addr, size, ud):
        dll, name = self.imports[addr]
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret = struct.unpack('<I', uc.mem_read(sp, 4))[0]
        impl = self.import_impl.get(name)
        if impl is None:
            print(f'[import] UNHANDLED {dll}!{name} from {ret:#x}', file=sys.stderr)
            raise Stop(f'unhandled import {name}')
        nargs, fn = impl
        args = [struct.unpack('<I', uc.mem_read(sp + 4 + 4 * i, 4))[0] for i in range(nargs)]
        r = fn(self, *args)
        if r is None:
            r = 0
        uc.reg_write(UC_X86_REG_EAX, r & 0xFFFFFFFF)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + 4 * nargs)
        uc.reg_write(UC_X86_REG_EIP, ret)

    # ------------------------------------------------------------ func hooks
    def hook_func(self, addr, fn, nargs=0, callee_pops=False):
        """Replace the function at `addr` with python `fn(emu, *args)`.
        cdecl by default; `callee_pops` => stdcall/thiscall style cleanup."""
        def cb(uc, a, size, ud):
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret = struct.unpack('<I', uc.mem_read(sp, 4))[0]
            args = [struct.unpack('<I', uc.mem_read(sp + 4 + 4 * i, 4))[0] for i in range(nargs)]
            r = fn(self, *args)
            if r is None:
                r = 0
            uc.reg_write(UC_X86_REG_EAX, r & 0xFFFFFFFF)
            uc.reg_write(UC_X86_REG_ESP, sp + 4 + (4 * nargs if callee_pops else 0))
            uc.reg_write(UC_X86_REG_EIP, ret)
        self.func_hooks[addr] = self.uc.hook_add(UC_HOOK_CODE, cb, begin=addr, end=addr)

    # ------------------------------------------------------------ memory
    def _unmapped(self, uc, access, addr, size, value, ud):
        if addr < 0x10000:
            print(f'[mem] null-page access {addr:#x} size {size} access {access} eip={uc.reg_read(UC_X86_REG_EIP):#x}', file=sys.stderr)
            return False
        if access == UC_MEM_FETCH_UNMAPPED:
            print(f'[mem] fetch unmapped {addr:#x}', file=sys.stderr)
            return False
        page = addr & ~0xFFF
        try:
            uc.mem_map(page, 0x1000, UC_PROT_ALL)
        except UcError:
            return False
        if self.verbose:
            print(f'[mem] auto-map {page:#x} (access {access}) eip={uc.reg_read(UC_X86_REG_EIP):#x}', file=sys.stderr)
        return True

    def alloc(self, size, zero=True):
        size = max(size, 1)
        p = (self.heap_ptr + 15) & ~15
        self.heap_ptr = p + size + 16
        if self.heap_ptr > HEAP_BASE + HEAP_SIZE:
            raise Stop('heap exhausted')
        self.alloc_sizes[p] = size
        return p

    def rd(self, addr, n):
        return bytes(self.uc.mem_read(addr, n))

    def u32(self, addr):
        return struct.unpack('<I', self.rd(addr, 4))[0]

    def wr(self, addr, data):
        self.uc.mem_write(addr, data)

    def w32(self, addr, v):
        self.uc.mem_write(addr, struct.pack('<I', v & 0xFFFFFFFF))

    def cstr(self, addr, maxn=512):
        out = b''
        while len(out) < maxn:
            c = self.rd(addr + len(out), 1)
            if c == b'\0':
                break
            out += c
        return out.decode('latin1')

    # ------------------------------------------------------------ calling
    def _sentinel(self, uc, addr, size, ud):
        uc.emu_stop()

    def reg(self, name):
        return self.uc.reg_read(getattr(sys.modules[__name__], 'UC_X86_REG_' + name.upper()))

    def call(self, addr, args=(), ecx=None, stdcall=True, max_insn=0, timeout_us=0):
        """Call function: pushes args (right to left), return addr = sentinel.
        Returns EAX.  Callee cleanup is left to the callee (esp restored after)."""
        uc = self.uc
        sp = uc.reg_read(UC_X86_REG_ESP)
        if sp < STACK_TOP - STACK_SIZE or sp > STACK_TOP:
            sp = STACK_TOP - 0x1000
        base_sp = sp
        for a in reversed(args):
            sp -= 4
            uc.mem_write(sp, struct.pack('<I', a & 0xFFFFFFFF))
        sp -= 4
        uc.mem_write(sp, struct.pack('<I', RET_SENTINEL))
        uc.reg_write(UC_X86_REG_ESP, sp)
        if ecx is not None:
            uc.reg_write(UC_X86_REG_ECX, ecx)
        try:
            uc.emu_start(addr, RET_SENTINEL, timeout=timeout_us, count=max_insn)
        except UcError as e:
            eip = uc.reg_read(UC_X86_REG_EIP)
            print(f'[emu] UcError {e} at eip={eip:#x}', file=sys.stderr)
            self.dump_state()
            raise
        eax = uc.reg_read(UC_X86_REG_EAX)
        uc.reg_write(UC_X86_REG_ESP, base_sp)
        return eax

    def dump_state(self):
        uc = self.uc
        regs = 'eax ebx ecx edx esi edi ebp esp eip'.split()
        print(' '.join(f'{r}={uc.reg_read(getattr(sys.modules[__name__], "UC_X86_REG_" + r.upper())):#x}' for r in regs), file=sys.stderr)
        eip = uc.reg_read(UC_X86_REG_EIP)
        try:
            code = bytes(uc.mem_read(eip, 16))
            for i in self.md.disasm(code, eip):
                print(f'  {i.address:#x}: {i.mnemonic} {i.op_str}', file=sys.stderr)
                break
        except Exception:
            pass
        print('  blocks:', ' '.join(f'{b:#x}' for b in list(self.block_ring)[-40:]), file=sys.stderr)
        print('  bt:', ' '.join(f'{w:#x}' for _, w in self.backtrace(150)[:24]), file=sys.stderr)
        esp = uc.reg_read(UC_X86_REG_ESP)
        try:
            st = struct.unpack('<16I', uc.mem_read(esp, 64))
            print('  stack:', ' '.join(f'{x:#x}' for x in st), file=sys.stderr)
        except Exception:
            pass

    def trace_blocks(self):
        def cb(uc, addr, size, ud):
            self.block_ring.append(addr)
        self.uc.hook_add(UC_HOOK_BLOCK, cb)


def _backtrace(self, depth=400):
    uc = self.uc
    esp = uc.reg_read(UC_X86_REG_ESP)
    out = []
    for i in range(depth):
        try:
            w = struct.unpack('<I', uc.mem_read(esp + 4 * i, 4))[0]
        except UcError:
            break
        if 0x401000 < w < 0x665000:
            try:
                b = bytes(uc.mem_read(w - 7, 7))
            except UcError:
                continue
            if b[2] == 0xE8 or b[1:3] == b'\xFF\x15' or b[5:7] in (b'\xFF\x10', b'\xFF\x11', b'\xFF\x12', b'\xFF\xD0', b'\xFF\xD1', b'\xFF\xD2') or b[0:2] == b'\xFF\x90' or b[4:5] == b'\xE8' or b[1:2] == b'\xFF':
                out.append((esp + 4 * i, w))
    return out
Emu.backtrace = _backtrace


def _retn(self, addr, limit=0x3000):
    """Collect `ret imm16` / `ret` immediates found by linear sweep from addr until the first ret at depth 0 (approx)."""
    code = bytes(self.uc.mem_read(addr, limit))
    rets = []
    for i in self.md.disasm(code, addr):
        if i.mnemonic == 'ret':
            rets.append(int(i.op_str, 16) if i.op_str else 0)
            if len(rets) >= 3:
                break
    return rets
Emu.retn = _retn
