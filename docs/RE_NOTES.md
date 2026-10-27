# Reverse-engineering notes

Findings from the 1.27b binaries (`game.dll` SHA-256 d51e5680…8236, `war3.exe`,
`Storm.dll`) that the port relies on.  Addresses are virtual addresses in the
original images.

## Startup and data

- **Command line** (war3.exe, Storm argument table at `0x45d090`): `-d3d`, `-opengl`,
  `-swtnl`, `-classic`, `-crash`, `-datadir <v>`, `-loadfile <v>`, `-gametype <v>`,
  `-fullscreen`, `-window`.
- **Archive priorities** (`FUN_6f03df20`): `War3.mpq` 0, `War3x.mpq` 1,
  `War3xLocal.mpq` 3, `War3Patch.mpq` 8.  Both base archives are always mounted.
- **Data sets** (`FUN_6f03e070`): the folder `Melee_V<n>` or `Custom_V<n>` inside
  War3x, War3xLocal, and War3Patch (twice) is mounted at priorities 9–12 via Storm
  ordinal 300 (`SFileOpenPathAsArchive`).  `n` is the game version
  (`DAT_6fcd52f0`): 0 under `-classic` or for a RoC replay (`FUN_6f046800`), else 1.
- **MPQ Huffman** (Storm `FUN_15015410` and helpers): weight tables at
  `0x150421b8`, 9 × 0x102 bytes; adaptive updates only for compression type 0.

## Scripts

- Native registration: `FUN_6f207490` (common.j, 1,167 entries:
  `push sig; mov edx,name; mov ecx,impl; call 0x6f90d120`) and `FUN_6f9bdec0`
  (common.ai, 124 entries).  Natives are cdecl; reals are passed by pointer.
- Order ids: `FUN_6f487cb0` registers 347 `(id, name)` pairs; `smart` (851971) is the
  constant at `0x6fb77ddc`.
- RNG: state update `FUN_6f1b7130`, seeding `FUN_6f1cc4e0`, table at `0x6fa92f10`.
  `GetRandomInt` (`0x6f201e30`) returns `lo + rand*(|hi-lo|+1)>>32`, which exceeds
  `lo` when `hi < lo`.
- `StringHash` is Storm ordinal 590: Jenkins lookup2 over the upper-cased string with
  `/` mapped to `\`.
- `R2S` formats with 3 decimals; `S2I` is `atoi`.
- Invalid-player defaults: `GetPlayerController` → 5, `GetPlayerRace` → 1.
- `GetCameraMargin` reads `UI\MiscData.txt [CameraMargins]` × 128.

## Rendering

- Cliff models: `Doodads\Terrain\<dir>\<dir><TL><TR><BR><BL><var>.mdx`, letters
  `'A' + layer - min` (`FUN_6f744560`).
- Day/night: first light of the DNC model; frame = time of day × sequence length;
  direction = light node rotation × −Z; colors are stored BGR.
- `war3map.shd`: one byte per 32×32 cell, rows from the north edge.
- Projection, measured against frames captured from the original at 4:3: the
  vertical extent matches 70° across the 4:3 diagonal, and the camera target projects
  0.085 NDC above center.  Not yet confirmed in code.
- CGameUI (`0x6f367710`): world frame from y 0.13 to 0.58; `ResourceBarFrame`
  TOPRIGHT; `UpperButtonBarFrame` TOPLEFT.  `FUN_6f111600` is SetPoint
  (frame, point, relative, relativePoint, x, y), points in TOPLEFT…BOTTOMRIGHT order.
- Player gold is stored in tenths internally (`CInfoPanelIconAlly::vf27`).

## Fog of war

- `CFogOfWarMap` cells are 128 units: a static initializer stores 128.0 at
  `0x6fd723d8` and 64.0 at `0x6fd723dc` (`mov edx, 0x80; mov ecx, &global`).
- `UI\MiscData.txt [FogOfWar]` colors are alpha, red, green, blue.

## Interface

- Message frames (CGameUI init): `WorldFrameMessage` 0.38 × 0.05 with its bottom 0.18
  above the screen bottom (errors, 10 s, cleared first; `FUN_6f36de80`);
  `WorldFrameUnitMessage` 0.5 × 0.25 whose bottom center sits 0.098125 left of center
  on the message frame's top, moved by DisplayTextToPlayer x and y toward x 0.8 and
  y 0.452 (`FUN_6f374d20`); default duration 3 s + 0.1 s per byte.
- Error texts: `Units\CommandStrings.txt [Errors]`; per-race lists run Human, Orc,
  Undead, Night Elf.
- Acknowledgments (`FUN_6f697c30`, `FUN_6f690dd0`): three consecutive "What" lines on
  the same unit, then "Pissed" lines in order; the counter resets on another unit or
  when the Pissed list runs out.  Suffix table at `0x6fce5734`.
- Script dialogs (`FUN_6f3c9550`, `FUN_6f3aa500`): height = 0.03 margins + message +
  0.011 + buttons (0.003 apart), rounded up to 0.048 steps, at least 0.096; the
  message is centered in the extra space.
- CTextFrame starts with white text and a black shadow.

## Glue screens

- 1.27 stretches the 0.8 × 0.6 glue space over the whole window (unlike the in-game
  console, which stays 4:3).  Wine reference captures have a 29-pixel title bar, so
  the client area is 871 pixels tall at 900.
- `CGlueMgr` sprite layers are war3skins keys `GlueSpriteLayerBackground_V<n>` (the
  3D scene with its camera and `[MenuZFog]`), `GlueSpriteLayerTopLeft` and
  `GlueSpriteLayerTopRight` (panel models whose sequences are named after the screen:
  `MainMenu Birth`, `SinglePlayerSkirmish Stand`, ...; word list at `0xab6aa0`).
- `CSpriteFrame` (`FUN_6f1add30`): orthographic over the frame, depth −500 to 500
  with z flipped, model origin at the frame's lower left; a frame without a size
  takes the model's bounds (`FUN_6f0f7080`, `FUN_6f0f6fc0`).
- The menu background camera's field of view spans the 4:3 diagonal, as in game.
- A glue button plays `GlueScreenClick` (`FUN_6f358c30`) and fades the screen's controls
  for `UI\MiscData.txt [Glue] ControlFadeDuration` (0.25); the timer it starts
  (`FUN_6f33fd40`, duration from `FUN_6f047680`) then changes screens, and only then do
  the panels play their Death.
- Sprite models' event objects all go through `FUN_6f782860`; `SND` events play their
  `AnimLookups` label unpositioned (`FUN_6f358940(code, 0, 0)`).  The glue panels' events
  (`ARPD`, `ABPU`, ...) are the chains' sounds.
- Map size names: `UI\WorldEditData.txt [MapSizes]`, the first area bound at or above
  the playable area (`FUN_6f71f720`); tileset names `[TileSets]`; strings from
  `UI\WorldEditGameStrings.txt`.
- Loading: `LoadingMeleeBackground` per race skin, `LoadingProgressBar` stepped through
  its Birth; custom maps use the w3i model or `[LoadingScreens]`.

## Network (W3GS)

The packets as `crates/net` reads them were captured from the original under Wine;
these are the findings behind them.

- Dispatch: the host's handlers by packet id in `FUN_6f99b870` (`Net::NetRouter`),
  the player's in `FUN_6f98a8e0` (`Net::NetClient`).  0x0E (`FUN_6f98b5d0`) is the host's
  desync notice: it groups players by agreement with a majority rule.
- GAMEINFO's settings word is the setup's map flags (as CAdvancedOptionsPane keeps them)
  shifted left 8, the speed in the low 2 bits; 1.27b's default fast game is 0x64802.
  Its game type is 1 for a custom map, 9 for a Blizzard map, 0x200 for a saved game.
  The list shows `slots - player slots + players` of `slots`.
- A saved game is offered by its save (`Save\Multiplayer\<name>.w3z` in the stat
  string, size 0, SHA-1 zero) and a 32-bit identifier the original stores in the save's
  decompressed header (offset 140 in one sample), in the map checksum's place; MAPCHECK
  still checks the map.  PLAYERINFO carries 0x0101 after the name in its lobby, else
  0x0001.
- Map checks: size, CRC-32 of the file, the xoro of common.j, blizzard.j, 0x03F1379E,
  and the map's game files, and their SHA-1 (verified against the original's values).
- 1.27b will not join a game at its own address on its own game port.
- Save actions: `06 <name>\0` saves, `07 01000000` says the save is done.
- The keepalive's checksum (`FUN_6f330830`, sent by `FUN_6f32e3f0` through
  `FUN_6f96a4e0`): `c = t ^ rol(m ^ 0x17ab, 3)`, `c = rol(c, 3) ^ presence`,
  `c = rol(c, 3) ^ birth`, sent `rol(c, 6)`.  `m` combines the game state's (TLS slot
  0xd) modules from last to first, `m = rol(m ^ module.vf0(), 3)` (`FUN_6f042dd0`); the
  debug dump `FUN_6f328d70` names module 0 cheat, 1 combat, 2 net, 3 rand, 4 unit, and 6
  game.  `t` is the game's field 0x1c70, or the Tempest checksum (`FUN_6f324930`) when
  field 0x2f8 is set.  `presence` and `birth` are the next presence and birth tags
  (`[DAT_6fd6860c + 0x40]`, or `+0x1c` when that is -1, and `+0x50`): counters of the
  objects created.  0x17ab is the build, 6059.
- The modules, read under winedbg in a Booty Bay game (20 of them; game.dll was
  relocated to 0x75d70000, so subtract 0x6d70000 from what a debugger shows), by their
  vf0: 0 `FUN_6f379b10` (a flags word less bit 0x1000: cheats); 2 `FUN_6f3248c0` (a
  sub-object's vf1: net); 3 `FUN_6f672050` (rand: `b * 0x7a2d ^ b` summed over 45 pairs
  of words of state, all eight bytes of each); 4, 5, 7, 9 `FUN_6f042da0` (once a turn,
  when enabled, mixes in `FUN_6f1996c0`, a hash of a store of typed values, `I`, `F`, and
  `S`, by the same byte sum); 6 `FUN_6f1de210` (game: for each of the 12 players
  present, `FUN_6f25c560`: fields 0x29c and 0x2a4, each entry's 0xc ^ 0x10, and
  `FUN_6f2292c0` and `FUN_6f28e740`); 12 to 14 and 17 to 19 `FUN_6f6a80e0` (the
  typed-value hash again, and `FUN_6f6e50d0` over 0x18-byte entries); 1, 8, 10, 11, 15,
  16 empty in that game.
- Debugging the original under Wine: `Frozen Throne.exe` only starts `war3.exe`, and
  attaching or breaking in crashes the injected thread under WoW64; start `war3.exe`
  under `winedbg --file` with `$BreakOnFirstChance = 0`, `$CanDeferOnBPByAddr = 1`, and
  breakpoints at the relocated addresses.  The reference install's RenderEdge plugin
  shows a version error once when war3.exe starts on its own; dismissing it goes on.
- Generators: besides the main one (`[0x6fd53a48]`, GetRandomInt's), CRandData
  (`FUN_6f65c7c0`) holds 45, each drawn on by index through `FUN_6f693660(index, n)`
  (an integer below n), `FUN_6f6936a0(out, index)` (a real), or `FUN_6f695c70(out, index,
  dice)`.  The game starts (`FUN_6f29e300`) by seeding the main one with the setup's seed
  ('war3', 0x77617233, under MAP_LOCK_RANDOM_SEED 0x8000) and the 45 from it
  (`FUN_6f693710`: a generator seeded with it gives each its seed in turn).
  SetRandomSeed (`FUN_6f214140`) seeds the main one, draws once, and seeds the 45 from the
  draw.  In Booty Bay with seed 0x35000000, by the first turn the main one had drawn 27
  times and streams 2, 21, and 37 46, 10, and 34 times; 37 is drawn on by
  `FUN_6f524e00` (a unit with Ahid, Ashm, Amim, Apiv, or Adet, a delay), 21 by
  `FUN_6f41ae70`, and 2 by `FUN_6f4993b0`, `FUN_6f6d48b0`, `FUN_6f4ccb80`,
  `FUN_6f508730`, `FUN_6f5d0410`, `FUN_6f5d9d60`, and (dice) `FUN_6f498960`.
- The main generator's draws before the first turn of Booty Bay (seed 0x35000000, both
  players random): 12 random races (`FUN_6f1e9dd0`: each of the 12 players whose
  preference is random, in order, `(r >> 30) + 1`), the start locations' shuffle
  (`FUN_6f1d1460`, from `FUN_6f1ea2f0`: for i from 1, `j = r * (i + 1) >> 32`, swap), and
  14 wander waits (CAbilityWander vf189, `FUN_6f59b070`).  The streams: 2 by
  CAbilityAttack vf194 (`FUN_6f4993b0`, a new armed unit's first look for a target), 21
  by CAbilityAlarm vf32 (`FUN_6f41ae70`), 37 by creep sleep (CAbilityCreepSleep and
  CAbilitySleepAlways vf32 and vf3, `FUN_6f524e00`: a delay to the next sleep check,
  drawn when the ability starts and at each check).  Checksum module 6 is CGameWar3.
  The `draws` example prints this game's draws to compare.
- Creep sleep: the ability checks as it starts (unit creation) and on the day's change
  (event 0x80261): a creep of Neutral Hostile (or one that sleeps always), at night,
  falls asleep at once (flag 0x80) and draws on stream 37 a wait of up to 3.0 (the
  constants at 0x6fd3c740 are -1, 0, 1, 2, 3, 4, 5, 6) before it shows it (event
  0xd01c1, `FUN_6f524b40`: the sleep effect and animation).  Units made by the map
  script are made at midnight, before the melee setup's 8:00, so every creep falls
  asleep as it is made and wakes with the day.
- Alarm (CAbilityAlarm): every structure has one (town halls, gold mines, shops; not in
  the unit data), which as it starts draws on stream 21 a wait before it watches for
  enemies (event 0xd01c1, `FUN_6f436510`, an area watch for 0xd01c8).
- With these, all 46 generators stand as the original's after the first turn of Booty
  Bay from seed 0x35000000.
- The keepalive checksum exactly (`FUN_6f330830`, from its instructions; verified
  against the original's turn-1 keepalive 0x610c6f3a): `c = t ^ rol(m ^ 0x17ab, 3)`,
  then `c = rol(c, 3) ^ presence`, `rol(c, 3) ^ birth`, `rol(c, 3) ^ h`, sent `rol(c,
  3)`, where `h` is the byte hash (`b * 0x7a2d ^ b` summed) of the main generator's
  value and taps (`FUN_6f330910`).  The modules by class: 0 CCheatData, 2 CNetData, 3
  CRandData, 4 and 7 CWar3SlkDatabase, 5 CAbilityDatabase, 6 CGameWar3, 9
  CUpgradeDatabase, 12 to 19 the map's custom data (unit, item, destructable, ability,
  buff, upgrade).  Their values at Booty Bay's first turn: 19 0, 18 c7d37d14, 17
  50bc09d8, 16 a4fc2e24, 14 68b89b88, 13 and 12 38eb3c00, 9 d85c9c00, 7 98054236, 6
  aa436eb2, 5 a996f046, 4 b5891e1b, 3 4c0afd48, 2 00c0cec0, 0 0; t 0, both tags 0x3205.
  The databases' and custom data's values are made once as the data loads
  (`FUN_6f658a20`), the typed-value hash being a sum over cells (order does not
  matter).  The player module (`FUN_6f25c560`) mixes objects' tag pairs (`+0xc ^ +0x10`).
- The object manager (`DAT_6fd6860c`, Tempest's) keeps two tag tables: at +0 entries of
  8 bytes (-2, object), its count at +0x1c (the presence tag unless +0x40 is set), and
  at +0x20 another (its next tag at +0x50).  Its 12805 objects at Booty Bay's first turn
  (read under winedbg through the table): 7216 CPmRegion (pathing map regions), 2636
  CAgentBaseAbs, 1410 CPoPos, 725 CPoFlag, 378 CPoReal, 134 CPoReThresholdLis, 130
  CPoReContinuousMod, 71 CPoPosCl, 71 CLrPath, 20 CPoFgLis, 6 CPoPoInterfLis, 4
  CAcceleratorMap, 2 CProximityMap, a CLrPathingSys and a CLrPathingAcc (all NIpse).
  The tags, and so the object ids the actions name, follow the engine's pathing regions
  and property objects as much as the game's objects.
- Every tagged object carries type codes at +0xc and +0x10 and its tag at +0x14 and +0x18
  (equal for a new object).  The whole sequence at Booty Bay's first turn is in
  `docs/reference/bootybay_tags.txt`.  In it: each player `+ply`, 26 `` `flg ``, `` `fcl ``,
  `ally`, 11 `` `flg ``; then system objects (`+tmr`, `+gam`, `+qum`, groups, timers,
  `^pma`, `^ama`, `l^sy`, `l^ac`, `gccm`, `AI-1`); each destructable `+w3d`, `` `pos ``, five
  `^pmr`; each unit `+w3u`, `` `pcu ``, two `^pmr`, five `` `rel ``, `l^pa`, `` `rcm `` (one
  that moves), `` `rtl ``, then each ability (some with a `` `rel ``, `` `rtl `` or `` `pil ``
  after it), then two `task`; the script's handles as it makes them (a trigger `+trg`, two
  `` `flg ``, its event, `+tac`, `hdlr`; locations, groups, timers, sounds, forces).
- A unit's tags (`FUN_6f68a060`; measured by a census of every unit type, made by
  script for an empty slot and for Neutral Hostile, under Wine): `+w3u` `` `pcu `` two
  `^pmr` five `` `rel `` `l^pa`; its life regeneration's `` `rcm `` (regenType always or
  night; for blight, `Ablr`, with an `` `rcm `` when made on blight); `` `rtl ``; its
  mana's `` `rcm `` (manaN above 0); three `^pmr` for a structure.  Then its abilities:
  the race's build ability when it builds a unit type (`FUN_6f686570`: AHbu, AObu, AUbu,
  AEbu, AGbu for naga, else ANbu) and `` `rel ``; `Aque` `` `rel `` when it trains or
  researches anything; `Amai`, `Asei`, `Asel`, `Aupg`, each with a `` `rel ``, for items
  made and sold, units sold, and upgrades; `Amov` when it moves; `Aatk` when it has a
  weapon, with a `spla` for each weapon slot of a splash kind (msplash, artillery, aline,
  mline, mbounce, enabled or not); a hero's `AHer` `` `flg `` `` `flg `` and four
  `` `rcm ``; the type's list (`FUN_6f4725f0`), each ability's code with the objects in
  `docs/reference/ability_tags.txt` (measured by adding each ability to a sheep);
  `ARal` when it trains or revives; `AInv` for a hero of an old map; fire (Afih, Afio,
  Afiu, Afin by race) for a structure or a machine; `Abdt` `` `pil `` for a flyer
  (`FUN_6f3f69b0`); `+ord` when its type's autocast ability (UnitAbilities auto, by code)
  may be on (`FUN_6f48b230`: its research done, when AbilityData checkDep asks); `ACsp`
  for a sleeper that is not a structure (`FUN_6f501130`); the alarm `Aalr` for a
  structure (`FUN_6f3f5a20`); then two `task`; `AIts` for a gold mine.  A creep then
  has its AI: `AIcs` when it starts a camp (no camp's first creep within 26 pathing
  cells, CreepCampPathingCellDistance, `FUN_6f9ba6d0`), `uref`, `AI-U`, `+ord`, `tskA`,
  four `task`, `tskA`, `+ord`, `task`.
- An aura gives its bearer its buff (and the buff's objects) at once when it serves it:
  on a sheep, without attack or mana, Trueshot, Vampiric, War Drums, Command, and
  Brilliance Aura gave none.  A morph gives its result an object: Amil's Bmil on a
  militia, Aphx's Bphx on an egg, Stone Form's `` `rcm `` on a gargoyle of stone.
- A creep's Hurl Boulder and Thunder Clap (ACtb, ACtc) each make a timer the first time
  its mana is set (Booty Bay's Granite Golems, after SetUnitState).
- A new structure with blight growth (Abli) blights the four 128 cells about it at once
  (a Necropolis made by script: blighted to 96 on its right before the next line, to 768
  a second later).
- The script's objects: a trigger `+trg` and two `` `flg ``; its events `uevt` (unit),
  `pevt` (player, player-unit), `tmet` (a timer's expiry), `gfvt` `` `rtl `` (game
  state), `gevt` (game), `alvt` `` `fcl `` (alliance), `psvt` `` `fcl `` (player state);
  `+tac` for an action; `+flt` for a filter or condition; `+tmr`, `+grp`, `+loc`,
  `+rct`, `+frc` with two `` `flg ``, `+tid`, `+snd` for every sound.  A script
  function given to an object (an action, a filter, a timer) has a handler `hdlr` the
  first time (`FUN_6f2872f0`, a table keyed by the function; a timer's apart).
  Measured by a census calling each native once between marker timers: events `tmvt`
  (TriggerRegisterTimerEvent), `wdvt` (death), `usvt` `` `rtl `` (unit state), `+rev`
  (enter and leave region), `pcvt` (chat), `vevt` (variable), `devt` (dialog), `bevt`
  (dialog button), `rgvt` `` `pil `` (unit in range), `+tev` (trackable), none for
  TriggerRegisterFilterUnitEvent; `tcnd` for TriggerAddCondition, `bAnd`, `bNot`;
  CreateRegion `+agr` `^pmr`; pools `pool` and `ipol`; `+dlg`, `+dlb`, `+ldb`, `+mdb`,
  `+que`, `+qui`, `+dfc`, `+fgm`, `+trk`, `+cst`; InitHashtable `ghth`, InitGameCache
  `gcch`; an item `item` `` `pos `` and two `^pmr`; AddSpecialEffect `+EIP` `` `pos ``
  and two `^pmr`; none for weather, images, ubersplats, lightning, and text tags.  An
  immediate order makes `+ord` `tskA`, four `task`, `tskA` (an autocast toggle none);
  KillUnit `tskA`, seven `task`, `tskA`; RemoveUnit `tskA`, five `task`, `tskA`.
- A trigger's execution is a `+tex` (ConditionalTriggerExecute, or an event firing it).
  A time-of-day event that becomes true fires, its run followed by a `` `rtl ``.  A change
  between day (6 to 18) and night makes two `` `rtl `` for day, one for night, then each
  living unit, the last made first, an `` `rcm ``; at night an idle creep that may sleep
  falls asleep at once, and with the day a sleeping one wakes, a `` `rtl `` for each of
  its spells that has one (Ensnare a `` `pil `` too).  After main, by the first turn, the
  engine makes an `` `rcm `` and a `` `rtl ``.  Measured with Booty Bay with and without
  MeleeStartingVisibility, and a map that changes the time between making units.
- Slots and frees (`FUN_6f1c53f0`, `FUN_6f1cbf40`, `FUN_6f1cbff0`): the manager holds two
  tables (+0, and +0x20 for objects with the high bit), each with a free head (+0x40,
  +0x44) and a live count (+0x48, +0x4c); an object takes the head's slot (whose word
  is the next free one), else a new slot, and a birth count (+0x50, +0x54) into +0x18.
  A freed slot becomes the head.  Booty Bay at turn 200: 12805 slots, 509 free, 12296
  live, 13338 born: 533 objects made and 1042 freed in ten seconds, mostly orders and
  tasks, timers, and others' aura buffs.
- A trace of every allocation and free after the first turn (breakpoints at 0x75f3bf40
  and 0x75f3bff0 with game.dll at 0x75d70000, the keepalive as a turn marker) shows the
  frees deferred to the next turn, all at once: units in creation order, each freeing
  the timers its abilities replaced (a waking creep's spells', a golem's after its mana
  was set), then its creation's two tasks, the second first, then a creep's initial AI
  order block (`tskA` +1, `tskA` +6, `task` +5 to +2, `+ord` +0, `task` +8, `+ord` +7);
  after the map's units the time of day's trigger execution, its fired event's timer, the
  engine's timer at slot 0x287, and the melee setup's destroyed groups and locations;
  then the melee starting units likewise; then the melee initialization's execution and
  the game-started timer and its handler.  From the third turn the creep AI and aura
  scans make and free objects every few turns.
- Those frees are Tempest events: an object destroys itself (event `^sig`,
  `FUN_6f15e500`, vf 0x10) when its event is dispatched from the engine's event queue,
  a binary heap (`FUN_6f04f990` push, `FUN_6f04f3c0` pop; array at +0xc, count at
  +0x1c) ordered by due time (+4) and, between equal times, by a serial (+0x14) that
  grows with each insertion; `FUN_6f054190` dispatches everything due each frame
  (`FUN_6f052380`, `FUN_6f054370` calls the target's vf 0x48).  In Booty Bay's second
  turn every event is due at 0.0001: the units' periodic timers (`` `rtl ``, flags
  0x10001) and the self-destructions, in insertion order, units in creation order.  To
  free as the original does, a game must queue the same events in the same order.
- What a trace of the queue's insertions shows (Booty Bay, breakpoints on the push and
  dispatch with each event's due time, serial, and target): an object queues its own
  destruction as it is released, due at once, and the next frame runs them in that
  order.  A new unit releases, in order: if a creep falling asleep as it is made, its
  autocast's order and its spells' first timers, the last first; its making's two
  tasks, the second first; a creep's first AI orders and tasks (`tskA` +1 and +6,
  `task` +5 to +2, `+ord` +0, `task` +8, `+ord` +7).  A creep's mana set replaces its
  spells' timers (the old released, the last first).  A trigger's execution is
  released as its actions end, a fired time-of-day event's timer as it is replaced, the
  engine's day timer at the change of day, a group or location as the script destroys
  it, a timer as DestroyTimer runs and its function's handler with its last timer.
  The day's change gives each unit an object lasting 3 seconds.  A town hall's blight
  dispel (Abdl: 64 every 0.08 to 960) releases its objects (its two regions, position,
  Xbli, then itself) as it ends.  An event due exactly at a turn's end runs in the next
  turn (the melee setup's 2-second timer, the dispels' end at 1.2).  ForForce's and the
  enumerations' functions take handlers as ForGroup's do.  With these, every allocation
  and free agrees with the original's for Booty Bay's first 51 turns (5.1 seconds);
  then the critters' wandering begins, each order with its own tasks (`ordt`, `tskA`,
  `tskO`, `tsk.`, `task`).
- A move order's objects (traced with the wanderers): issued, eighteen (`ordt`, `tskA`,
  `tskA`, `tskO`, three `task`, `tsk.`, six `task`, `tskA`, `^clg`, `^clu`, `l^pa`), nine
  freed at once (`^clg`, the second, the fifteenth, the six tasks from the last); done,
  three more (`tskA`, `task`, `task`), then `tsk.`, those three from the last, the three
  tasks before `tsk.` from the last, `tskO`, the third, `ordt`, and the path's `l^pa`
  and `^clu` (the same turn or the next).
- Wander (CAbilityWander): the wait is the main generator's, 5 to 10 seconds (vf189,
  `FUN_6f59b070`), counted from the unit's making; the point is the unit's place plus,
  on each axis (x first), stream 8's draw times 500 less 250 (vf190, `FUN_6f59af20`;
  1000 and 2000 for units with flag 0x20000000 at +0x5c and +0x200 below 1).  The
  engine's float arithmetic here is its own (`FUN_6f06f9c0` multiplies, `FUN_6f06fa90`
  subtracts, `FUN_6f06fbb0` adds).  Booty Bay's fourteen first walks start in the same
  turns as the original's; the first (a sheep near the map's south edge) ends 0.4
  seconds in for the original but not for this game.  Measured (that sheep ordered to
  its point by script): with its flock about it, the original's moves 18 and stops by
  0.4 seconds, the standing sheep blocking its path; with the flock removed it walks
  straight southwest and stops at 1108.4, -5059.2 by 3.5 seconds, the closest it can
  reach.  This game's goes west then south through the flock and stops at 1108.8,
  -5091.3: its pathfinding (units as obstacles, the route, and the closest reachable
  point) is not yet the original's, and every lifetime after this follows from it.
- The object manager's second table (+0x20, its births at +0x54) holds the engine's
  private objects, which the actions' ids and the checksum never see: effect images
  (`+EIm`, a buff's art), the game interface's camera (`+cam` with eleven `` `rel ``,
  two `` `pos `` `^pmr` `^pmr`, and a `` `rel ``, made by `FUN_6f36cb20` as slots are
  assigned in a game on one computer), and the computer AI's camp records (`+ccu`,
  `+ccp`).  The allocator (`FUN_6f1cbf40`) names the table in its second argument, the
  free (`FUN_6f1cbff0`) in its third.
- Against a computer (measured under Wine, alone with an undead computer on Booty
  Bay): an `AI-1` for each computer playing before Neutral Hostile's (`FUN_6f1e9a40`);
  each of the computer's units, structures among them, takes the creep's AI after its
  two tasks; a structure removed releases its footprint's three `^pmr` at once, before
  the removal's tasks; CreateBlightedGoldmine's hidden gold mine takes no `AIts` and
  releases its footprint as it is made.  Each new worker's AI block then carries one
  more `+ord` `tskA` `task`: a worker's AI setup (`FUN_6f9c3660`) orders each of its
  owner's idle workers, itself among them (`FUN_6f9c9cc0`, `FUN_6f6802f0`), each such
  order freed next turn `tskA`, `task`, `+ord` before the usual nine.  A unit with an
  inventory then takes its item pickup range, a `` `pil `` (`FUN_6f67dff0` starting its
  abilities' AI; AInv's `FUN_6f606a20`).  Next, StartMeleeAI makes `AItr`, two
  captains (`AI-C` `` `pcu `` `^pmr` `^pmr` `l^pa` and four `` `pil `` each), and the
  AI script's first orders, among them a move.
- Pathing (NIpse, the engine's pathing and motion; read from game.dll).  A move asks
  through `FUN_6f05b970`: the goal clamped to the map and divided by 32 (adding
  0xfd800000 to a float's bits), set on the unit's position behavior (`FUN_6f16db30`,
  `FUN_6f171340`, ...), and searched later, in the engine's step.  The long-range
  search (CLrPathingSys) is A* over pathing cells, its nodes (CLrCellNode, 0x24 bytes
  at +0x30: x, y, a stamp, state -1 new / -2 open / else on the expanded list, g, h,
  parent) made per search as reached (`FUN_6f147af0`, at most 0x8000).  Its open
  list is a binary heap from 1 on unsigned keys g + h (push `FUN_6f1483f0`, pop
  `FUN_6f148240`: the right child taken when its key is no greater, the sift stopping
  at a key no greater than the moved one), entries checked by stamp as popped
  (`FUN_6f14a4c0`, which stops at the goal node +0x94, after +0x68 pops, or when the
  goal's region is touched, +0xcc).  Costs are 15 a side and 21 a diagonal; the
  heuristic, over cell offsets times 15 from the goal (+0x88, +0x8c), is max(a, b),
  or when min > max/4, with m = min * 1.5: max + max/64 + m/4 + m/128 - max/16 (all
  integer).  Relaxing (`FUN_6f14a560`) a node first reached keeps the one nearest
  the goal by squared cell distance (+0x98, +0x9c), where a search that cannot reach
  it ends.  Expansion (`FUN_6f14b760`) tries eight neighbors in the order SW, S, SE,
  W, E, NW, N, NE; which are open comes from a ring of cells about the node, sized by
  the unit's class (+0xa0: 3x3, 4x4, ...; `FUN_6f148d00` reading the ring's bottom
  row left to right, right column up, top row right to left, left column down, a bit
  for each cell refused), a diagonal needing its two sides open too.  A cell
  (`FUN_6f1489a0`) is refused for its static flags (the high byte of the map's word,
  against the search's mask +0xa4), or for a region on it (CPmRegion, `^pmr`; the low
  24 bits link the cell's tokens, records of 8 bytes, type 0x1000000) that is active
  (+0x37 bit 0), of a pathing type the search minds (+0x34 against +0xa4), and not
  excused by its flags (+0x40: none of 0x8fffffff, and none of 0x60000000 unless the
  search counts those, +0xd4).  Units' and structures' regions so block the search:
  the standing flock that stops Booty Bay's first wandering sheep.
- A search (`FUN_6f14ad50`) starts and ends at the cells of points in cell units, its
  class from the unit's collision in cells (below 0.5, 1, 1.5, or more), its pops
  limited by the path's word (+0x84 of CLrPath, which holds two paths at +0x50 and
  +0x70, each with a limit word, +0x84 and +0x86: 700 and 400 by default), a new
  search id each time.  The path (`FUN_6f148100`, from CLrPath's `FUN_6f166e90`, the
  unit's own two regions excused meanwhile by raising their counts at +0x40) runs to
  the goal when found; otherwise to the middle of the nearest node reached, or, when
  that is the start, nowhere: the unit stays.  Searches are also spaced in time per
  kind (`FUN_6f168910`) and counted against per-class budgets.
- The pathing map's static byte is the map file's flags with 0x01 added, and 0x10 where
  0x02 (unwalkable) is set (measured: 0x40 reads 0x41, 0xca 0xdb, 0xce 0xdf).  A
  unit's collision region (types 0xca for a sheep) covers, on each axis, cells
  floor(p - 0.5 - r) to floor(p - 0.5 + r) for its position p and radius r in cells
  (measured with Booty Bay's sheep, radius 0.5: two cells each way); its second region
  (types 0x1000000) one cell.  Replayed on the map read about the first wandering sheep,
  the search (`lrpath.rs`) steps only south from 430, 46 and then finds nothing nearer.
- Paths have two levels (measured: a footman ordered across Booty Bay first searches
  locally from 142, 262.5 to 143.5, 239.5, a waypoint).  CLrPathingAcc plans over an
  accelerator map (CAcceleratorMap, cells of 8 bytes at +0x28; its census held four),
  by the same A* (`FUN_6f164c30` sets it up: start and goal cells, a layer bit
  1 << n, a pop limit; `FUN_6f163f50` pops; nodes 0x24 bytes at +0x5c, each a cell,
  expanded by `FUN_6f1643d0`, or a warp, +0x22, by `FUN_6f1644d0`), its step costs
  integer square roots of squared offsets (`FUN_6f1d58e0`, Newton's method from
  n / 12 + 1, n / 200 + 21, or n / 26743 + 444, halving toward zero), the same
  nearest-node fallback; CLrPathingSys then searches between its waypoints.  The
  accelerator is a pyramid of four maps (at +0x1c + 4 * level, coordinates shifted
  right by the level; CAcceleratorMap's size the pathing map's, `FUN_6f151760`), each
  cell [stamp, word]: the word's two bits for the search's layer (shifted left by
  +0xd4 into bits 30 and 31) 0 open, 1 blocked, 2 mixed; its low 16 bits the cell's
  node in the current search (made again when the stamp is old, `FUN_6f163ef0`).  A
  cell's node is its largest open block (`FUN_6f1625f0` climbs while the next level's
  cell is open); a block's expansion (`FUN_6f1644d0`) walks the cells along its four
  sides and its corners, a single cell's (`FUN_6f1643d0`) its four sides, then its
  corners when both sides beside them were open, then a warp (+0x3c, 12 bytes:
  flags, x, y) when it has one.  Its level-0 cell x, y stands for the pathing cells
  2x to 2x + 1 by 2y to 2y + 1 (`FUN_6f15cf80` with `FUN_6f15d0e0`): for each of four
  layers (shifts 0, 2, 4, 6; masks 0x06000006, 0x80000080, 0x40000040, 0x04000004)
  open when none of the four is refused, blocked when all are, else mixed.  A pathing
  cell is refused there (`FUN_6f148e90`) for its static byte against the mask's high
  byte, or for a region on it that is static (flags 0x10000000, types 0x1000000;
  structures', destructables', doodads': not units') and of the mask's low types.  A
  higher level's cell (`FUN_6f15d470` with `FUN_6f15d1c0`) is mixed when one of its
  four children is mixed or marked (byte +6), else open or blocked when all four are,
  else mixed, the map's outside counting as blocked.  Measured against Booty Bay's
  (read whole under Wine, its levels 393 by 201, 196 by 100, 98 by 50, 49 by 25):
  level 0 is built for cells 0 to 384 by 0 to 192 (those past it stay open), from the
  map file's flags and the cells destructables and structures block (refusing layers
  0 and 1; this game's blockers give every cell but around a structure of another
  race), and levels 1 to 3 follow from it by the rule above, every cell.  Its search's
  nodes are blocks, each remembering the level-0 cell it was first reached from (and
  its level, +0x22); a step costs isqrt((24 |dx|)^2 + (24 |dy|)^2) between the two
  nodes' cells, the estimate the same to the goal's cell (`FUN_6f164020`), a warp's
  step 1 more than its source (`FUN_6f165220`).
- A path searches its accelerator plan only with +0x88's 0x200000 (`FUN_6f167120`,
  `FUN_6f165b60`); without it (Neutral Passive's wanderers) its plan is the goal
  itself, and the local search runs straight for it; a local path is searched again
  once its points are used up (`FUN_6f167ce0`).  With this, the first wandering
  sheep's walk ends as the original's: 18 units south, stopping as it enters the
  cell its search reached (this game's at 1232, -4930, the original's 1231.73,
  -4929.95, its turn on the way bending it).
- The engine steps movement every 30 ms of game time (`FUN_6f15aa80` counting the
  steps at +0x538; measured: 4, 3, 3 steps a 100 ms turn, ten each 300 ms).
- A path's +0x88 holds its class in bits 16 to 19 (measured: 0 for player 0's units,
  1 for creeps, 15 for Neutral Passive's) and flags (0x200000, 0x800000, ...).  A
  wanderer owned by Neutral Passive has 0x200000 cleared and its search limit set to
  32 (CAbilityWander's `FUN_6f5a6d40`: `FUN_6f0594a0(0)`, `FUN_6f05c720(0, 0x20)`;
  other owners keep 700): measured, Booty Bay's sheep and pigs search 32 nodes deep,
  player 0's sheep and Neutral Passive's footmen 700.  Booty Bay's sheep's searches:
  from 430.5, 46.5 to 426.56, 40.48 (mask 0x2000002, radius 0.5), then from 430.49,
  45.94 after 18 units.
- New units on walkable ground need room by their size class (collision in cells below
  0.5, 1, 1.5, or more; `FUN_6f170080` with `FUN_6f149370`): sheep (16) made in the cell
  row next to unwalkable ground move up a row; Booty Bay's fifth pig stands at -5456,
  -4432.
- The script's globals run before the map's destructables: Blizzard.j's first timer,
  then the engine's `+gam` `` `flg `` `` `flg `` `` `rel `` `` `rel `` `` `flg ``
  `` `rtl `` `+qum`, then the other globals.
- A destructable placed with flags 3 is scenery, drawn but not made (Booty Bay's three
  such trees are not found by EnumDestructablesInRect and take no tags).
- Script handle ids (GetHandleId) are another table's (the object manager's at +0x20,
  0x45d entries at that turn; Player(0) is 0x100008): the tags above are the engine's
  own ids for all its objects, which actions (two words each) and the checksum use.
- Replays (.w3g) record each turn's actions as W3GS carries them, and after each turn
  (0x22) the same checksum, so a replay is a turn-by-turn reference.

## Where the port departs from the original

Formats the port made its own, to be replaced by the original's:

- Saved games: the port's world state (`crates/game/src/save.rs`), not the original's
  .w3z.
- Replays: .w3g files as the original writes them (header and its CRC, 8192-byte
  zlib blocks with their checksums, the record of host, players, slots, and seed, a
  turn per frame, verified by writing back a replay of the original's), but each
  command still the port's own in a private action (0x7F), each turn's exact length
  in another (0x7D, from player 0), and the port's setup in a private record (0x7E);
  older `W3RRPLY` files still play.  The original's own encodings measured so far
  (`docs/reference/bootybay_orders.w3g`): a selection change is `Select` (remove the
  old, add the new), `PreSubselection`, and `SelectSubgroup` by the first unit's type;
  a right-click is `OrderTarget` with order 851971 (smart), flags 0 on the ground with
  no object, 8 on a gold mine; training is `Order` with flags 0x42 and the unit type;
  object ids are (slot, birth), equal for the peasants at the start.
- Options: the original's registry keys and values (HKEY_CURRENT_USER\Software\Blizzard
  Entertainment\Warcraft III, its Gameplay, Misc, Sound, Video, and String keys, as the
  reference install's Wine registry has them), kept in a registry file (REGEDIT4,
  `Warcraft III.reg` in the data folder) since the port has no Windows registry.
- Profiles: campaign progress, the menu's race, the tutorial, and the difficulty are in
  the original's `Campaigns.w3p` (`war3/src/w3p.rs`: `[Campaign]` for Reign of Chaos and
  `[Campaign1]` for The Frozen Throne, a 64-bit mask per campaign named as CampaignList
  has it, mission n at bit n, 63 available, 62 opening and 61 ending cinematic, 59 and
  60 the menu's; `Magic` and `Magic1` the CRC-32 of each section's masks; read and
  written byte for byte as the reference install's).  Profiles are `Save\ProfileN`
  folders named by their PlayerName, the chosen one in the registry's
  Misc\campaignprofile (a list kept before in `profiles.json` still serves).  Still the
  port's own: saved game caches and the custom campaign buttons (`profile_vN.json`).
- Saved game caches: SaveGameCache writes every saved cache into the profile's one
  `Campaigns.w3v`, in the replay container (header, zlib blocks; sub-header product
  PX3W, version 27, build 6059).  Its record (`docs/reference/cache_sample.w3v`, from a
  test map storing each kind): u32 1, u32 the caches; each cache its name (C string),
  u32 3 and u32 its missions (3 for three); each mission its name, five zero words,
  then lists, each a u32 count and its entries: integers (name, i32), reals (name,
  f32), flags (name, u32), units (name and a record), strings (name, C string).  The
  unit record (built by `FUN_6f24abc0`, CItemCacheEntry for its items): its type, u32
  6 and six item slots of three words (type, charges, a third from `FUN_6f6581f0`), the
  hero's experience and level, its attributes with their gains per level (a Paladin's
  22 and 2.7, 13 and 1.5, 17 and 1.8), its abilities (5 entries), a position (1800.0
  for the y it was made at), and a ten-bit mask of `FUN_6f2907e0`'s answers; its exact
  layout is still to be read from the writer.
- Network games' commands: the original's actions for selection changes and orders to
  the selection (`crates/game/src/actions.rs`), the rest still the port's own commands
  in a private action (0x7F) until each is measured; the objects' ids come from the
  port's tags, which match the original's at a game's start but not yet once objects
  are freed (see the event queue above).

## Tooling

The decompilation is produced by headless Ghidra 12.1.2 with MSVC RTTI applied,
virtual methods named `Class::vfN`, native implementations named `jass_<Name>` and
`ai_<Name>`, and the Decompiler Parameter ID analyzer enabled.  Reference frames are
captured by running the original under Wine on a private Xvfb display.

An analyzed Ghidra project is kept in `ghidra/` at the repository root (ignored by
git).  The analysis takes about 15 minutes, longer than REA's Ghidra startup allows,
so build it once from the repository root with Ghidra's default auto-analysis:

    mkdir -p ghidra
    GHIDRA_MAXMEM=8G /opt/ghidra/support/analyzeHeadless ghidra war3 \
        -import "Warcraft III/game.dll" -analysisTimeoutPerFile 7200

Its names are Ghidra's own (`FUN_<address>`), which the addresses in these notes use.
Query it headlessly with the scripts in `tools/ghidra`:

    /opt/ghidra/support/analyzeHeadless ghidra war3 -process game.dll -noanalysis \
        -readOnly -scriptPath tools/ghidra -postScript StrRefs.java \
        GlueScreenClick fn:6f301cf0 asm:6f301a00 callers:6f306dd0

- `StrRefs`: each string's references and their decompilation; `fn:<addr>` decompiles a
  function, `asm:<addr>` lists its instructions, `callers:<addr>` its callers.
- `Refs <addr>...`: the functions referencing an address (a global, say), decompiled.
- `Range <from> <to> [text]`: decompiles every function in a range, or only those whose
  C contains the text.
- `Vt <addr>`: for a function, the vtables pointing at it, with their neighbors.

The output is on stdout, each line prefixed `INFO  <Script>.java> `.
