# Multiplayer and game mode

Owns: mode global, net gates. Reference: `rust/src/net.rs`.

## Mode global `[0x9AFD74]` (verified: `0x499FC0`/`0x499FE0`)

Two one-purpose predicates, reached via jump thunks used all over the
image (`0x47B530 → 0x499FE0`, `0x47B550 → 0x499FC0`):

```asm
0x499fc0  mov eax, [0x9afd74]   ; mode
          cmp eax, 4 / je true
          cmp eax, 2 / je true  ; 0x47B550: mode in {2, 4}
          xor eax, eax / ret
0x499fe0  mov eax, [0x9afd74]
          cmp eax, 4 / je true
          cmp eax, 5 / je true  ; 0x47B530: mode in {4, 5}
          xor eax, eax / ret
```

`0x47B530` (mode 4|5) gates founding, disease, trade notify, combat
resolve — anything with a local-vs-remote split. Meaning of the values
is HYPOTHESIS: 2 = single-player, 4/5 = multiplayer roles (4 passes both
predicates, so it shares single-player behavior — possibly hotseat).
Mode 4|5 is also what the `0x43FC03` trade path uses to pick the remote
notify `0x475460(this=0x74AF60)` over the local `0x561860`.

`rust/src/net.rs`: `Mode` predicates, tested.

## `FNetQueue::readData` error branch `0x484409` (verified: `r2`)

First cited code address in the dark `0x48` bucket. Full string at
`0x685250`: `FNetQueue::readData  bad itotalnodes count...\n`
(`readData` also at `0x68525B` mid-string and `0x72991F` bare):

```asm
0x484406  ret 4                       ; end of previous function
0x484409  push 0x685250               ; "FNetQueue::readData ..."
0x48440e  call [0x665188]             ; OutputDebugStringA
0x484414  mov eax, edi / pop edi/esi/ebp/ebx / add esp,0xC / ret 4
0x484420  push ecx / push ebx / mov ebx,ecx ...   ; next fn, thiscall
```

The containing function (start open — scan back from `0x484409`) walks a
`[ebx+0x20]→[eax+4]` node chain with a type switch on `0x0E`/`0x11`/`0x2E`
(`0x484453`/`0x484458`/`0x48445D`) and calls `0x46F640` per node
(HYPOTHESIS: the readData deserializer itself).

Race string at `0x6853FC` (full text, corrected — it starts with
`+++`, which is why a mid-string grep found zero refs):
`+++Failed to assign a race to player %d+++\n`. It has **six**
push-imm sites: `0x48E146`, `0x54E504`, `0x54E633`, `0x5A0976`,
`0x5A0CAE`, `0x5A2167` — setup UI (`0x48`/`0x54`) plus game-start
assignment (`0x5A`), all logging via `0x5F9920`.

## Race assignment core (verified: `r2`, sites `0x48E146` + `0x5A0976`)

`[0x9C3DB4]` = player count (loop bound at both sites).

* Setup (`0x48E102…`): walks player slots (stride `0x20E4`, bound
  `0xA646B4`) and a `0x910270–0x912E14` table (stride `0x63C`); failed
  slots log the race string. Count clamps to `[1, 31]` (`cmp ebp,1 /
  cmp ebp,0x1F`) into `[0x9C74C8]` — 31 = max civs.
* Game-start (`0x5A0962…`): per player, `call 0x53A060` picks a race;
  success stores it and or-bits `1<<race` into `[0xA526C4]`
  (used-races mask, `0x5A09D3–0x5A09F2`); failure logs the string and
  stamps `-1` (`mov [edx],0xFFFFFFFF`, `0x5A097B`).

Sites `0x54E504`/`0x54E633`/`0x5A0CAE`/`0x5A2167`: same string,
unread (HYPOTHESIS: setup variants). The `0x685300–0x6855F0` cluster
is the whole MP setup vocabulary: `MP_CHANGE_SESSION_NAME`,
`MP_NO_EXIT`, `MP_TOO_MANY_PLAYERS`, `MP_NO_AI_IN_TURNLESS`,
`MP_NOT_EVERYONE_READY`, `MP_OTHER_PLAYERS_CONNECTED`,
`MP_HOTSEAT_PLAYER_IN_NORMAL_GAME`, `MP_NO_SAME_RACES`,
`MP_NO_KICK_WHILE_JOINING/LOADING`, `MP_END_GAME_LIMITS*`,
`LIMIT_*` victory caps, `MPSave.tmp`, `civ3F.bix`.

`rust/src/net.rs`: `player_count_clamp()`, `race_mask_assign()`,
tested.

## `0x48` bucket map: netcode + MP setup (sweep; core parent-verified)

Dark bucket retired: the `0x48` range holds the lobby (`0x480xxx–
0x483xxx`), `FNetQueue` (`0x483Fxx–0x4846xx`), and staging/setup
(`0x484xxx–0x48Fxxx`), entered from the `0x47` lobby-builder and
`0x55` MP launch.

### `FNetQueue` (parent-verified vtable bytes + `readData` head)

Vtable `0x6672FC` (exact dump): `[0x483FE0 dtor, 0x4FCAB0 base,
0x484420 pack, 0x4842E0 readData, 0x484520 measure]` — pack/read/
measure are virtual-only (zero `E8` refs). Ctor `0x483FB0` tags the
object `'NETQ'` via `mmioStringToFOURCCA`; embedded at `+0x216C` in
the peer-session ctor `0x468100` (child-reported).

* `readData 0x4842E0–0x48441F`: `sub esp,0xC`, node alloc via
  `malloc 0x649A8B`, error path at `0x484409` (see above).
* `pack 0x484420` / `measure 0x484520`: node walk with
  `call 0x46F640(this=0x74AF60)`, msg-type filter `0xE/0x11/0x2E`;
  measure sums sizes from base 4.
* List ops: pop-by-seq `0x484260`, free-list `0x484010`, append
  `0x484040`, seek `0x484580`, peek `0x4845B0`, unlink `0x4845C0`
  (callers in `0x46`, incl. the `0x46F4EA` type-12 poll loop).

### Setup flow (child-reported, byte-evidenced)

| range | contents |
|---|---|
| `0x480E40–0x483DEB` | lobby: key-handler, widget-init, game-select/list-refresh (`0x481E30`, version 22/1 gate), join + password dialogs, chat ring-buffer (`0x482B60`, 0x104-stride, mod-0x100), GameSpy query keys (`numplayers/maxplayers/civ3gamemode/Turnless…`), nick prompt/verify, validators, DirectIP-connect |
| `0x483DF0–0x483F80` | MP-dialog dtor family + mini-ctor |
| `0x4846F0–0x4848A0` | player-record ops (`0x63C` stride): copy/swap/init + config-init (`conquests.biq` default) |
| `0x484DC0–0x485863` | tournament/load gate (7000 ms `MP_NO_LOAD_WHILE_LOADING` window) |
| `0x485870–0x486706` | slot-reset, connect-flow (`0x4858F0` ← `0x55` launch), setup-art loader (`0x485A90`, `art\Multiplayer\…` via `0x598580`), staging-init, orchestrator `0x486710` |
| `0x486A70–0x48B7D2` | staging widgets, row-fill hub `0x48B340`, key-handler |
| `0x48B7E0–0x48D17B` | staging-tick (6000/3000 ms phases), click-dispatch `0x48BDF0` (no direct caller — virtual/dynamic HYPOTHESIS), color-pack `0x48D180` (57 callers, incl. map renderer) |
| `0x48D200–0x48E442` | start-gate, admit chain (`MP_TOO_MANY_PLAYERS`, hotseat checks), pre-race rules, RACE FN `0x48DDE0` (rules copy, `[0xA526BC]` mask, slot loop — owns the `0x48E146` race-string site) |
| `0x48E450–0x490272` | post-race confirm, name-editor (`CUSTOMNAMES`/`CUSTOMGENDER`), row-state/walker, MP-save `0x48FA60`, slot-claim `0x48FB30` (`EnterCriticalSection` + `put player #%d` log), tail-op crossing into `0x49` |

Hubs: `0x60F6A0` clamp (127 image-wide callers), `0x49A340`
self-id (`[0x9AFD5C]`, adjacent to the mode global), `0x47B550`
mode gate. String block `0x6850D8–0x6854D0` (77 refs) fully
clustered; orphans: `iskeuvha.tmp` (`0x68528C`, zero refs),
`MP_NO_EXIT`/`MP_NOT_EVERYONE_READY` (referenced from `0x49`, not
`0x48`).

`rust/src/net.rs`: `FNETQUEUE_VTABLE`, tested exact.
