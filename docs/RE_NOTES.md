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
- Replays: `W3RRPLY` (`crates/game/src/replay.rs`), not .w3g (which it now reads).
- Options and profiles: JSON files (`options.json`, `profiles.json`,
  `profile_vN.json`), not the original's registry settings and `Save\ProfileN`.
- Network games' commands: the port's own inside W3GS's actions, not the original's
  actions (`crates/net/src/action.rs` reads and writes those).

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
