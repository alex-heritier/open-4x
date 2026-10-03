"""Win32 import stubs for emu.Emu."""
import struct, sys

def ret0(n):
    return (n, lambda e, *a: 0)

def ret1(n):
    return (n, lambda e, *a: 1)

def _fourcc(e, s, flags):
    b = e.cstr(s).encode('latin1')[:4].ljust(4, b' ')
    if flags & 0x10:
        b = b.upper()
    return struct.unpack('<I', b)[0]

def _heapalloc(e, h, flags, size):
    return e.alloc(size)

def _heaprealloc(e, h, flags, p, size):
    n = e.alloc(size)
    old = e.alloc_sizes.get(p, 0)
    if p and old:
        e.wr(n, e.rd(p, min(old, size)))
    return n

def _heapsize(e, h, flags, p):
    return e.alloc_sizes.get(p, 0)

def _globalalloc(e, flags, size):
    return e.alloc(size)

def _virtualalloc(e, addr, size, typ, prot):
    return e.alloc(size)

def _tlsalloc(e):
    i = e.tls_next
    e.tls_next += 1
    return i

def _tlsget(e, i):
    return e.tls.get(i, 0)

def _tlsset(e, i, v):
    e.tls[i] = v
    return 1

def _qpf(e, p):
    e.wr(p, struct.pack('<Q', 1000000))
    return 1

_qpc_t = [0]
def _qpc(e, p):
    _qpc_t[0] += 1000
    e.wr(p, struct.pack('<Q', _qpc_t[0]))
    return 1

def _getmodulehandle(e, name):
    return 0x400000

def _gettick(e):
    _qpc_t[0] += 16
    return 100000 + _qpc_t[0]

def _dbg(e, s):
    if e.verbose:
        print('[dbg]', e.cstr(s).rstrip(), file=sys.stderr)

def _getstartup(e, p):
    e.wr(p, b'\0' * 68)
    e.w32(p, 68)

def _getversionex(e, p):
    e.wr(p + 4, struct.pack('<IIII', 5, 1, 2600, 2))

def _getcmdline(e):
    if not hasattr(e, '_cmd'):
        e._cmd = e.alloc(16)
        e.wr(e._cmd, b'civ3.exe\0')
    return e._cmd

def _getenvstrings(e):
    if not hasattr(e, '_env'):
        e._env = e.alloc(16)
    return e._env

def _getmodfile(e, h, buf, n):
    s = b'C:\\Civ3\\Civ3Conquests.exe\0'
    e.wr(buf, s)
    return len(s) - 1

TABLE = {
    # WINMM
    'mmioStringToFOURCCA': (2, _fourcc),
    'timeSetEvent': ret0(5), 'timeKillEvent': ret0(1), 'timeEndPeriod': ret0(1), 'timeBeginPeriod': ret0(1),
    'timeGetDevCaps': ret0(2), 'timeGetTime': (0, _gettick),
    # ADVAPI32 (registry): fail -> defaults
    'RegCloseKey': ret0(1), 'RegQueryValueExA': (6, lambda e, *a: 2), 'RegOpenKeyExA': (5, lambda e, *a: 2),
    'RegCreateKeyExA': (9, lambda e, *a: 5), 'RegSetValueExA': ret0(6),
    'CoCreateInstance': (5, lambda e, *a: 0x80004002), 'CoCreateGuid': ret0(1),
    # KERNEL32
    'UnhandledExceptionFilter': ret0(1), 'FlushFileBuffers': ret1(1), 'SetStdHandle': ret1(2), 'GetFileType': ret0(1),
    'GetStdHandle': ret0(1), 'SetHandleCount': ret0(1), 'FreeEnvironmentStringsA': ret1(1), 'FreeEnvironmentStringsW': ret1(1),
    'GetEnvironmentStrings': (0, _getenvstrings), 'GetEnvironmentStringsW': (0, _getenvstrings),
    'LCMapStringW': ret0(6), 'LCMapStringA': ret0(6), 'MultiByteToWideChar': ret0(6), 'WideCharToMultiByte': ret0(8),
    'HeapSize': (3, _heapsize), 'GetStringTypeA': ret0(5), 'GetStringTypeW': ret0(4),
    'IsBadReadPtr': ret0(2), 'IsBadWritePtr': ret0(2), 'IsBadCodePtr': ret0(1), 'SetUnhandledExceptionFilter': ret0(1),
    'VirtualAlloc': (4, _virtualalloc), 'VirtualFree': ret1(3), 'GetCPInfo': ret0(2), 'HeapDestroy': ret1(1),
    'GetEnvironmentVariableA': ret0(3), 'GetModuleFileNameA': (3, _getmodfile),
    'TlsGetValue': (1, _tlsget), 'TlsSetValue': (2, _tlsset), 'TlsAlloc': (0, _tlsalloc),
    'SetLastError': ret0(1), 'GetLastError': ret0(0),
    'GetSystemTime': ret0(1), 'GetLocalTime': ret0(1), 'GetTimeZoneInformation': ret0(1), 'GetSystemTimeAsFileTime': ret0(1),
    'GetACP': (0, lambda e: 1252), 'GetOEMCP': (0, lambda e: 437),
    'CompareStringA': ret0(6), 'CompareStringW': ret0(6),
    'GetVersion': (0, lambda e: 0x0A280105), 'GetVersionExA': (1, _getversionex), 'GetStartupInfoA': (1, _getstartup),
    'GetModuleHandleA': (1, _getmodulehandle),
    'FileTimeToLocalFileTime': ret0(2), 'FileTimeToSystemTime': ret0(2),
    'CreateDirectoryA': ret1(2), 'SetFileAttributesA': ret1(2), 'GetFileAttributesA': (1, lambda e, p: 0xFFFFFFFF),
    'HeapReAlloc': (4, _heaprealloc), 'HeapAlloc': (3, _heapalloc), 'HeapFree': ret1(3), 'HeapCreate': (3, lambda e, *a: 0x1000),
    'RaiseException': ret0(4), 'InterlockedIncrement': (1, lambda e, p: (e.w32(p, e.u32(p) + 1), e.u32(p))[1]),
    'InterlockedDecrement': (1, lambda e, p: (e.w32(p, e.u32(p) - 1), e.u32(p))[1]),
    'TerminateProcess': ret0(2), 'ExitProcess': ret0(1), 'RtlUnwind': ret0(4),
    'SetEnvironmentVariableA': ret1(2),
    'GetFileSize': (2, lambda e, *a: 0xFFFFFFFF),
    'ResetEvent': ret1(1), 'SetEvent': ret1(1), 'WaitForMultipleObjects': ret0(4),
    'GetCurrentThreadId': (0, lambda e: 1), 'GetCurrentProcess': (0, lambda e: 0xFFFFFFFF),
    'SetProcessAffinityMask': ret1(2), 'GetProcessAffinityMask': ret1(3),
    'SetCurrentDirectoryA': ret1(1), 'GetCurrentDirectoryA': ret0(2),
    'GlobalAlloc': (2, _globalalloc), 'GlobalLock': (1, lambda e, p: p), 'GlobalUnlock': ret1(1),
    'FreeLibrary': ret1(1), 'LoadLibraryA': ret0(1), 'GetProcAddress': ret0(2),
    'MoveFileA': ret1(2), 'CreateFileA': (7, lambda e, *a: 0xFFFFFFFF), 'SetFilePointer': (4, lambda e, *a: 0xFFFFFFFF),
    'SetEndOfFile': ret1(1), 'CreateFileMappingA': ret0(6), 'MapViewOfFile': ret0(5), 'UnmapViewOfFile': ret1(1),
    'OutputDebugStringA': (1, _dbg), 'WritePrivateProfileStringA': ret1(4), 'GetPrivateProfileStringA': ret0(6),
    'lstrcpynA': (3, lambda e, d, s, n: (e.wr(d, e.cstr(s, n - 1).encode('latin1') + b'\0'), d)[1]),
    'FindClose': ret1(1), 'FindFirstFileA': (2, lambda e, *a: 0xFFFFFFFF), 'FindNextFileA': ret0(2), 'WinExec': ret0(2),
    'LeaveCriticalSection': ret0(1), 'EnterCriticalSection': ret0(1), 'DeleteCriticalSection': ret0(1),
    'InitializeCriticalSection': ret0(1), 'Sleep': ret0(1), 'GetTickCount': (0, _gettick),
    'QueryPerformanceCounter': (1, _qpc), 'QueryPerformanceFrequency': (1, _qpf),
    'GetDriveTypeA': (1, lambda e, p: 3), 'ReleaseMutex': ret1(1), 'GetCommandLineA': (0, _getcmdline),
    'CreateMutexA': (3, lambda e, *a: 0x1234), 'WriteFile': ret1(5), 'ReadFile': ret0(5), 'DeleteFileA': ret1(1),
    'CloseHandle': ret1(1),
}


def _setrectempty(e, p):
    e.wr(p, b'\0' * 16)
    return 1

def _sysmetrics(e, i):
    return {0: 1024, 1: 768}.get(i, 0)

TABLE.update({
    'SetRectEmpty': (1, _setrectempty), 'GetSystemMetrics': (1, _sysmetrics),
    'IsRectEmpty': ret1(1), 'PtInRect': ret0(3), 'OffsetRect': ret1(3), 'InflateRect': ret1(3),
    'IntersectRect': ret0(3), 'UnionRect': ret0(3), 'ReleaseCapture': ret1(0), 'GetForegroundWindow': ret0(0),
    'GetFocus': ret0(0), 'GetKeyState': ret0(1), 'GetAsyncKeyState': ret0(1), 'ShowCursor': ret0(1),
    'GetCursor': ret0(0), 'LoadCursorA': ret0(2), 'SetCursor': ret0(1), 'CharUpperA': (1, lambda e, p: p),
    'MessageBoxA': ret1(4), 'MessageBeep': ret1(1), 'GetWindowLongA': ret0(2), 'PostMessageA': ret1(4),
    'SetTimer': ret0(4), 'KillTimer': ret1(2), 'ClipCursor': ret1(1), 'DestroyWindow': ret1(1),
})

def install(e):
    e.import_impl.update(TABLE)
