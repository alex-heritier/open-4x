"""Run functions of Civ3Conquests.exe under Unicorn, straight from the PE image.

A differential-testing oracle: the exe's own machine code is the reference, so a Rust
port can be compared with it on arbitrary inputs without running the game.

    from emu import Emu
    e = Emu()
    fm = e.alloc(0x20E4)
    e.thiscall(0x5E1B60, fm, W, H, level, flags, seed, 0)     # fractal()
    heights = e.read(fm + 0x20, 129 * 65)

Setup (once):  python3 -m venv /tmp/emu && /tmp/emu/bin/pip install unicorn pefile
Run:           /tmp/emu/bin/python <script>.py        ($CIV3_EXE overrides the exe lookup)

Conventions of this MSVC 6 build: methods are __thiscall (ecx = this, args pushed
right to left, callee pops); free functions are __cdecl or __stdcall.
"""
import os
import struct
import sys

import pefile
from unicorn import (Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_READ_UNMAPPED,
                     UC_HOOK_MEM_WRITE_UNMAPPED, UC_HOOK_MEM_FETCH_UNMAPPED, UcError)
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_EDX,
                               UC_X86_REG_ESI, UC_X86_REG_EDI, UC_X86_REG_EBP, UC_X86_REG_ESP,
                               UC_X86_REG_EIP, UC_X86_REG_EFLAGS, UC_X86_REG_FS_BASE,
                               UC_X86_REG_CR0)



def find_exe():
    """$CIV3_EXE, else Civ3Conquests.exe found by searching upward from this file
    (the same lookup r2q.sh does from $PWD)."""
    if os.environ.get("CIV3_EXE"):
        return os.environ["CIV3_EXE"]
    d = os.path.dirname(os.path.abspath(__file__))
    for _ in range(8):
        for rel in ("civ3/civ3-gog/app/Conquests/Civ3Conquests.exe", "civ3/re/Civ3Conquests.exe"):
            if os.path.isfile(os.path.join(d, rel)):
                return os.path.join(d, rel)
        d = os.path.dirname(d)
    raise FileNotFoundError("Civ3Conquests.exe not found; set CIV3_EXE")


DEFAULT_EXE = None

STACK_TOP = 0x7D100000
STACK_SIZE = 0x100000
HEAP_BASE = 0x10000000
HEAP_SIZE = 0x08000000
STUB_BASE = 0x7F000000
RET_SENTINEL = 0x7E000000
TEB_BASE = 0x7C000000


class Emu:
    def __init__(self, exe=None, trace_unknown=True):
        self.exe = exe or find_exe()
        self.pe = pefile.PE(self.exe)
        self.base = self.pe.OPTIONAL_HEADER.ImageBase
        self.mu = Uc(UC_ARCH_X86, UC_MODE_32)
        mu = self.mu
        size = (self.pe.OPTIONAL_HEADER.SizeOfImage + 0xFFF) & ~0xFFF
        mu.mem_map(self.base, size)
        mu.mem_write(self.base, self.pe.header)
        for s in self.pe.sections:
            data = s.get_data()
            mu.mem_write(self.base + s.VirtualAddress, data[: max(s.Misc_VirtualSize, len(data))])
        mu.mem_map(STACK_TOP - STACK_SIZE, STACK_SIZE)
        mu.mem_map(HEAP_BASE, HEAP_SIZE)
        mu.mem_map(STUB_BASE, 0x10000)
        mu.mem_map(RET_SENTINEL, 0x1000)
        mu.mem_map(TEB_BASE, 0x10000)
        mu.mem_write(STUB_BASE, b"\xC3" * 0x10000)
        mu.mem_write(RET_SENTINEL, b"\xC3" * 0x1000)
        # Minimal TEB so code that reads fs:[0] / fs:[0x18] does not fault.
        mu.mem_write(TEB_BASE + 0x18, struct.pack("<I", TEB_BASE))
        self.heap_ptr = HEAP_BASE + 0x1000
        self.imports = {}          # stub address -> (dll, name)
        self.import_handlers = {}  # name -> callable(emu)
        self.trace_unknown = trace_unknown
        self._bind_imports()
        mu.hook_add(UC_HOOK_CODE, self._code_hook, begin=STUB_BASE, end=STUB_BASE + 0x10000)
        self.stop_at = None
        self.insn_count = 0

    # ---- imports -------------------------------------------------------------------
    def _bind_imports(self):
        i = 0
        for entry in getattr(self.pe, "DIRECTORY_ENTRY_IMPORT", []):
            dll = entry.dll.decode().lower()
            for imp in entry.imports:
                name = imp.name.decode() if imp.name else f"ord{imp.ordinal}"
                stub = STUB_BASE + 0x10 * i
                i += 1
                self.mu.mem_write(imp.address, struct.pack("<I", stub))
                self.imports[stub] = (dll, name)

    def _code_hook(self, mu, addr, size, user):
        if addr in self.imports:
            dll, name = self.imports[addr]
            h = self.import_handlers.get(name)
            if h is None:
                if self.trace_unknown:
                    print(f"[emu] unhandled import {dll}!{name} called from "
                          f"{self.u32(self.reg('esp')):#x}", file=sys.stderr)
                return
            h(self)

    # ---- helpers -------------------------------------------------------------------
    def reg(self, name):
        return self.mu.reg_read(getattr(__import__("unicorn.x86_const", fromlist=["x"]),
                                        "UC_X86_REG_" + name.upper()))

    def set_reg(self, name, v):
        self.mu.reg_write(getattr(__import__("unicorn.x86_const", fromlist=["x"]),
                                  "UC_X86_REG_" + name.upper()), v & 0xFFFFFFFF)

    def alloc(self, n, align=16):
        self.heap_ptr = (self.heap_ptr + align - 1) & ~(align - 1)
        p = self.heap_ptr
        self.heap_ptr += n
        assert self.heap_ptr < HEAP_BASE + HEAP_SIZE, "emulated heap exhausted"
        return p

    def write(self, addr, data):
        self.mu.mem_write(addr, bytes(data))

    def read(self, addr, n):
        return bytes(self.mu.mem_read(addr, n))

    def u32(self, addr):
        return struct.unpack("<I", self.read(addr, 4))[0]

    def i32(self, addr):
        return struct.unpack("<i", self.read(addr, 4))[0]

    def w32(self, addr, v):
        self.write(addr, struct.pack("<I", v & 0xFFFFFFFF))

    # ---- calls ---------------------------------------------------------------------
    def call(self, addr, args=(), ecx=None, max_insns=200_000_000, regs=None):
        """Call `addr` with 32-bit stack args (pushed right to left). Returns eax."""
        mu = self.mu
        sp = STACK_TOP - 0x1000
        for a in reversed(args):
            sp -= 4
            mu.mem_write(sp, struct.pack("<I", a & 0xFFFFFFFF))
        sp -= 4
        mu.mem_write(sp, struct.pack("<I", RET_SENTINEL))
        mu.reg_write(UC_X86_REG_ESP, sp)
        if ecx is not None:
            mu.reg_write(UC_X86_REG_ECX, ecx & 0xFFFFFFFF)
        for k, v in (regs or {}).items():
            self.set_reg(k, v)
        mu.reg_write(UC_X86_REG_EFLAGS, 0x202)
        try:
            mu.emu_start(addr, RET_SENTINEL, count=max_insns)
        except UcError as e:
            raise RuntimeError(f"emulation fault at eip={self.reg('eip'):#x} esp={self.reg('esp'):#x}: {e}")
        return mu.reg_read(UC_X86_REG_EAX)

    def thiscall(self, addr, this, *args, **kw):
        return self.call(addr, args, ecx=this, **kw)


def main():
    e = Emu()
    print("image loaded", hex(e.base), "imports", len(e.imports))


if __name__ == "__main__":
    main()
