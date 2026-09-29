# Culture, corruption, trade economy

Owns: culture accumulation/border expansion, corruption/waste, commerce
split, victory culture thresholds. Reference: `rust/src/economy.rs`.

## Culture-border expansion event `0x4B0D70…` (verified: `r2`)

The expansion *event* (dialog + flag + map update), not the
accumulation math. Shape is the founding/diplo dialog idiom:

```asm
0x4b0d9b  call 0x47B530            ; mode check → 0x4000 MP flag (neg/sbb/and)
0x4b0db5  push CULTUREBORDERVERBOSE ; 0x6861AC
0x4b0dba  push 0xCADC18
0x4b0dbf  call [edx+0x170]          ; modal dispatch (4th 0xCADC18/0x170 site)
0x4b0dcb  call 0x611530             ; commit
0x4b0de8  mov ecx, [edx*4+0xA52ED8] ; owner-indexed flag table ([0x9FD4BC] key,
0x4b0def  or ecx, 8                 ;  same lea/shl index math as relation matrix)
0x4b0df2  mov [edx*4+0xA52E98+0x40], ecx
```

Then an owner branch (`[esi+0x28]` vs `[0x9FD4BC]`): own city pushes
`CULTUREBORDER` (`0x4B0E04`), foreign pushes `CULTUREBORDEROVER`
(`0x4B0E1F`), both `call 0x4ED220(this=0x9F8700, x=[esi+0x24],
y=[esi+0x26])` (HYPOTHESIS: border map repaint from tile-coordinate
words). Pre-calls: `0x53AA50(this=0xA9590C)` + `0x61C5A0` twice.

## Culture math (verified: sweep + parent spot-checks)

* Thresholds `0x4B0C60` (parent-verified head): prologue `51 53 55
  56 57 8BF1`; accumulated `[esi+owner*4+0x140]` (`owner =
  [esi+0x28]`); level = count of powers `base^k <= accum` for
  `base=[0x9C7300]`, capped at 6 (`cmp ebx,6`); stored to
  `[esi+0x5C]`. City-screen bar `0x41988F` renders `fild/fidiv`
  progress ratio (child-reported).
* Per-turn `0x4B2680` (sole caller `0x4BEBDB`): zeroes
  `[city+0x13C]`, loops BLDG table (`count [0x9C3D80]`, stride
  `0x110`) with has/obsolete/flag-`0xF0&4` gates; accumulation at
  `0x4B2897` adds into `[city+owner*4+0x140]` with `max(0,·)` clamp,
  then calls the border check (child-reported).
* Per-building `0x4F8CE0` (child-reported): base `BLDG+0x98`,
  doubled if age (`0x4C2420`) vs 1000, halved (`(ebp+1)/2`) when
  `[owner*8420+0xA52F3C]==1`.
* Victory ini keys confirmed: `one city culture to win`
  (`0x72D3CC`, default 100000) / `all cities culture to win`
  (`0x72D3B0`, default 66 — units HYPOTHESIS) at `0x58607C`/
  `0x58611D`.

## Corruption math (verified: sweep + parent spot-checks)

`0x4B1190` returns LOST commerce (disorder/anarchy return gross):

* Courthouse count `0x4B1250` (child-reported): has + !obsolete +
  `BLDG+0xEC` bit 16; capital bonus `ebx+10`.
* OCN `0x5676C0` (child-reported): RULE table + govt class
  `[GOVT+0x18C]` (×61×8 record math) + city count; class switch at
  `0x567771`.
* Distance arms (class switch `[ebp*4+0x4B1A2C]`, table bytes
  parent-verified: `[0x4B14E2, 0x4B156F, 0x4B156F, 0x4B1576,
  0x4B14F0, 0x4B14E5]`): arm 0 = `3D/4` (parent-verified `lea` +
  trunc `/4`); arms 1–2 = `D`, 3 = `3D/2`, 4 = `(W+H)/4`, 5 =
  `(W+H)/16` (child-reported, communal HYPOTHESIS on 5).
* Rank loop `0x4B1617` over `[0xA52E78]` cities (child-reported):
  same-owner + capital-distance ordering with tiebreaks; finalize
  (parent-verified at `0x4B18D9`): `rank' = rank>=R ? 2*rank-R :
  rank`, then `imul gross`.
* Waste (child-reported): disorder → gross; else
  `([city+0x138] − tilecount) * [0x9C72B4]` (`[city+0x138]` writer
  open — population/tile-count HYPOTHESIS).

## Commerce split (parent-verified at `0x4B0802–0x4B0881`)

```asm
call 0x4B1190               ; lost commerce
[esi+0x24C] = lost          ; 0x4B0814
[esi+0x258] = gross - lost  ; 0x4B0821 (net)
[esi+0x25C] = (net*[o*8420+0xA5303C]+5)/10   ; science, 0x66666667 magic
[esi+0x260] = (net*[o*8420+0xA53040]+5)/10   ; luxury
```

Owner index rebuilds `o*8420` inline (`al*2105*4` lea/shift chain at
`0x4B0827–0x4B0841` — second independent confirmation of the 8420
stride). Rates are tenths in the per-owner table (`+0x1A4` science,
`+0x1A8` luxury); TAX `[esi+0x264] = net−lux−sci` and building
multipliers (`BLDG+0xEC` bits) child-reported at `0x4B0906`.
Mood flags `[city+0x30]` (bit0 disorder, bit1 WLTKD-halves-distance):
child-reported.

## Leads (unverified)

* `GCON_Corruption` (`0x6840F0`, pushed `0x42007F`/`0x4200AF`),
  `GCON_Culture` (`0x6840B4`, pushed `0x4201B7`/`0x422D6D`/`0x4231B1`/
  `0x4FB8A6`), `GCON_Commerce` (`0x684070`, pushed `0x4202EF`/
  `0x521095`) — all via `call 0x4CBE10(this=0x9E85F0)` in `PtInRect`
  city-screen mouse code (HYPOTHESIS: help-text/constant lookup, not
  the yield math itself). Siblings: `GCON_Treasury`/`GCON_Research`/
  `GCON_Moods` (`0x4200CF…`).
* Culture advisor UI: `CULTURE_ADVISOR*` + `cultureometer.pcx` pushed
  at `0x4FA5BD…` (`0x4F` thin bucket).
* Open: per-tile gross summation, mood engine `0x4BE440`, slider
  dialog writer, trade-route income, palace-as-capitalIdx only (no
  separate palace flag found).
