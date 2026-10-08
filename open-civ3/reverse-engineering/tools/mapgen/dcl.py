"""PKWARE DCL ("explode") decompressor, binary-literal mode only.

The container every shipped `.biq`/`.bic`/`.bix`/`.SAV` uses (`00 06` header);
the full codec, including the ASCII-literal mode no shipped file uses, is the
Rust `biq::dcl` module (`reverse-engineering/biq.md`). This copy exists so the
emulator tools do not need a Rust build to read a save.

Verified byte for byte against the Rust decoder (see `selftest.py`).
"""

# Run-length encoded canonical code lengths: (repeat - 1) << 4 | length.
_LEN_LEN = bytes([2, 35, 36, 53, 38, 23])
_DIST_LEN = bytes([2, 20, 53, 230, 247, 151, 248])
_BASE = [3, 2, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 40, 72, 136, 264]
_EXTRA = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]
_MAXBITS = 13


def _construct(rep):
    lengths = []
    for b in rep:
        lengths.extend([b & 15] * ((b >> 4) + 1))
    count = [0] * (_MAXBITS + 1)
    for n in lengths:
        count[n] += 1
    offs = [0] * (_MAXBITS + 1)
    for i in range(1, _MAXBITS):
        offs[i + 1] = offs[i] + count[i]
    symbol = [0] * len(lengths)
    for sym, n in enumerate(lengths):
        if n:
            symbol[offs[n]] = sym
            offs[n] += 1
    return count, symbol


_LENCODE = _construct(_LEN_LEN)
_DISTCODE = _construct(_DIST_LEN)


class _Bits:
    def __init__(self, data, pos):
        self.data, self.pos, self.buf, self.cnt = data, pos, 0, 0

    def bits(self, need):
        val = self.buf
        while self.cnt < need:
            val |= self.data[self.pos] << self.cnt
            self.pos += 1
            self.cnt += 8
        self.buf = val >> need
        self.cnt -= need
        return val & ((1 << need) - 1)

    def decode(self, table):
        count, symbol = table
        code = first = index = 0
        for length in range(1, _MAXBITS + 1):
            code |= self.bits(1) ^ 1  # codes are stored inverted
            c = count[length]
            if code < first + c:
                return symbol[index + (code - first)]
            index += c
            first = (first + c) << 1
            code <<= 1
        raise ValueError('ran out of codes')


def looks_compressed(data):
    return len(data) > 4 and data[0] == 0 and 4 <= data[1] <= 6


def decompress(data):
    """Inflate a DCL stream (binary literals). Raises ValueError otherwise."""
    if data[0] != 0:
        raise ValueError('only binary-literal DCL streams are supported')
    dict_bits = data[1]
    if not 4 <= dict_bits <= 6:
        raise ValueError('bad dictionary size')
    br = _Bits(data, 2)
    out = bytearray()
    while True:
        if br.bits(1):
            sym = br.decode(_LENCODE)
            length = _BASE[sym] + br.bits(_EXTRA[sym])
            if length == 519:
                return bytes(out)
            shift = 2 if length == 2 else dict_bits
            dist = (br.decode(_DISTCODE) << shift) + br.bits(shift) + 1
            if dist > len(out):
                raise ValueError('distance too far back')
            for _ in range(length):
                out.append(out[-dist])
        else:
            out.append(br.bits(8))
