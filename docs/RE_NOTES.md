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
- The keepalive for turn n is made before the turn runs, from the state after turn
  n - 1: `t` (the game's +0x1c70) is the game time in milliseconds then, (n - 1) * 100
  (read: 0x12c0 at turn 49).  The net module (CNetData's vf0 reads its +0x227c, which
  `FUN_6f328550` adds each handled message's hash to) grows by 0x00c0cec0 a turn
  without actions, so is n * 0x00c0cec0 (read: 0x24e792c0 at turn 49).  The presence
  tag is the slot the allocator gives next (the last freed, or the count), the birth
  tag the count of tags given.  With the game (6) and data modules (4, 5, 7, 9, 12 to
  19) at their first-turn values, the port's state gives the original's keepalives
  for Booty Bay's turns 1 to 431 exactly (the run's trace parts there).  A turn
  without actions is one net message of kind 0x1f whose payload is the turn's time in
  milliseconds, two bytes: `FUN_6f328550` adds `bytehash(kind + bytehash(H(payload)))`,
  `H` xoring in the payload's words, then its bytes, rotating by 3 each time; 0x64 0x00
  gives 0x00c0cec0.  A turn with actions is one such message whose payload is the
  turn's time, then each player's block as INCOMING_ACTION carries it (id, length,
  actions); read under winedbg at `FUN_6f328550` (relocated 0x76098622) for turns
  with a selection, a control group, a training order, and a rally point, each sum
  as predicted.  Messages of kinds 0x17, 0x21, and 0x2f add the byte hash of the
  kind plus their fields' byte hashes (`FUN_6f14eb60`); a call with its argument 0
  (every 40 turns) looks at a message without adding or taking it.  The wanderers' first
  waits are drawn before the first keepalive, as the game is made.  `crate::keepalive`
  computes it; the tags example's TAGS_KEEPALIVE compares the port's with
  `docs/reference/bootybay_keepalives.txt`, the original's for 978 turns.
- With the engine's clocks kept exactly and the game time counted in whole
  milliseconds, the port's keepalives equal the original's for all 978 recorded turns
  of Booty Bay (no actions, the game and data modules given), and its tags trace
  agrees through the recording's last full turn.
- On Harrow (two players, the same seed) the original's modules 4 to 19 at each
  keepalive equal Booty Bay's, the game module 6 among them: with nobody acting they
  do not depend on the map (a map's own data or object changes would).
- The game module (6, CGameWar3, `FUN_6f1de210`): `m = 0xc`, then for each of the
  12 players `m = rol(m ^ f, 3)`, so player p's part enters rotated by 3 * (12 - p).
  A player's part (`FUN_6f25c560`): `u = rol(rol(+0x29c ^ tech, 3) ^ +0x2a4, 3)`,
  where the two are floats (1.0 at handicap 100) and `tech` the tech tree's hash
  (+0x2d0, CTechTree, `FUN_6f2292c0`); then `u = rol(u ^ rol(x, 3), 3)` for each
  object of the array at +0x2b4 (+0x2b0 slots, 7, all empty at the start); then `f =
  rol(sel ^ u, 3)` with `sel` the selection hash (`FUN_6f28e740` of +0x34).  `x` is an
  object's slot ^ birth (the list entries are handles carrying the unit's tag at +0xc
  and +0x10), so 0 for an object in a slot never freed.  The selection hash: `u = n`
  (the count selected, +0x10); for each selected unit `u = rol(u ^ rol(x, 3), 3)`;
  `u = rol(i ^ u, 3)` with i the selected subgroup's index (+0x1c0 in the subgroup
  list, -1 with none); for each subgroup (+0x1b8, a type's units) `u = rol(u ^
  count, 3)` and its units as above; the 10 control groups' units (+0x20, 0x14 apart);
  then the units at +0x1a4 (set with the first selection) and +0x1a8 if set.  Read
  under winedbg (`FUN_6f1de210`'s player 1, relocated 0x75f4e210) through a peon's
  selection, a control group, three peons and a second group, and recalling the
  first: each module value as computed.  At Booty Bay's start the two players' tech
  hashes are 0x4086ca18 and the ten empty players' 0x61021200, giving 0xaa436eb2.
  Training a peon flips one bit of its tech-tree entry while it trains.
- The tech tree (CTechTree): a registration per type code, in the order first touched
  (`FUN_6f22f5c0`), of +0x18 the limit (SetPlayerTechMaxAllowed, `FUN_6f2498a0`;
  0x7fffffff unset), +0x1c made, +0x20 in production, +0x24 in production of the
  second kind, +0x28 present, +0x2c killed, and +0x30 flags set as it is made (2 a
  research type, 4 a structure, 8 another unit type, 0 otherwise, 'HERO').  Its hash
  per registration (`FUN_6f229360`): `rol(p ^ m, 12) ^ rol(n ^ l, 12) ^ rol(k ^ c, 9)
  ^ rol(flags, 3)` (p in production, m made, n present, l limit, k killed, c the
  second kind), folded `t = rol(t ^ reg, 3)`.  Changes: a unit made finished adds
  (present and made, `FUN_6f227b40`; `FUN_6f677870` with 2); a start (`FUN_6f249cf0`)
  adds to in production, of a queued item as it takes its slot (`FUN_6f3f7f70`) or
  of a structure placed (unit made with 1); a finish (`FUN_6f22f6a0`; a unit made
  with 4, as from training) takes one from in production and adds present and made;
  a cancel (`FUN_6f228ed0`) takes one from in production; a unit removed
  (`FUN_6f6744d0`) cancels if it was started and takes one from present if it was
  added (its +0x64 bits 8 and 0x10); a change of type (`FUN_6f6703b0`) does both for
  the old type and starts or adds the new; a unit killed (`FUN_6f69c210`) by another
  player's unit adds one killed to the killer's owner's registration of its type.  At
  Booty Bay's start each slot has hrtt, otbk, and uske (InitSummonableCaps), HERO
  and the 24 hero types (MeleeStartingHeroLimit), and a playing slot its town hall
  and workers; the port keeps the same registrations (`crate::techreg`) and computes
  the game module from them, the handicaps (+0x29c SetPlayerHandicap, +0x2a4
  SetPlayerHandicapXP), and empty selections: the 978 keepalives still agree.
- The synchronized selection (CSelectionWar3, a CUnitSet; its debug log names each
  step, "Sync add", "Sync subgroup add ... to like subgroup", and so on): the
  dispatchers DispatchUnitSelectionModify (0x16, `FUN_6f6bc7c0`: 1 add, 2 remove, 3
  replace, at most 12), DispatchUnitDefineControlGroup (0x17, `FUN_6f6b90d0`: the
  group cleared and its units added, at most 12), DispatchUnitSelectControlGroup
  (0x18, `FUN_6f6bc0f0`: the group's units, 1 added or 3 replacing, then the first
  subgroup selected), DispatchUnitSelectSubGroup (0x19, `FUN_6f6bc560`: the first
  subgroup led by the type), and DispatchUnitRefreshSubGroup (0x1a, `FUN_6f6bc040`:
  the subgroups made again, the one led by the old lead's unit selected), each first
  dropping units gone or dead (`FUN_6f28e960`).  A CUnitSet adds a unit before the
  first of its type with a higher slot, else a hero first and another last
  (`FUN_6f28d7c0`); subgroups join a like one (same type, neither a hero, the same
  illusion state) or come in order of priority, then type, then slot
  (`FUN_6f28de20`); the active unit is the highest priority, the lower type between
  equals (`FUN_6f28d660`, `FUN_6f292360`), priority being UnitData prio (the data's
  +0x1cc), plus for a hero its owner's +0x334 less its hero data's +0x138 plus one,
  plus a constant for an illusion (`FUN_6f691980`).  `crate::selection` keeps it
  (the filter `FUN_6f391390` and the hero term not modeled); driven by the bbs3
  run's actions (`docs/reference/bootybay_selection_actions.txt`, the tags example's
  TAGS_ACTIONS), it gives the four module values the original had.
- A turn's actions are carried out as its message is handled, before that turn's
  keepalive is made (and before the turn's time passes): a selection made in turn n
  is in turn n's game module (read by inverting the original's keepalive at a turn
  with a selection; its net module then grew by just the message's hash).  With the
  port so (`Game::play_commands`, then the keepalive, then the step), a game in which
  the original selected units (`crates/net/examples/host.rs` now prints each turn's
  actions as "turn N carries pid hex") gives the original's keepalives through its
  selection turns; the first training order (turn 425 of that game) is the first
  turn they part.
- Without actions the port's keepalives agree with the original's (recorded again to
  turn 1615, `docs/reference/bootybay_keepalives.txt`) through turn 1143; turn 1144
  is the first to part.
- A training order (a traced game, `trace2.sh` with its actions, the order at turn
  991): its dispatch, before the turn's keepalive, makes a +ord, a `` `rcm ``, and a
  `` `rtl `` (the original's keepalive there is the port's with the peon's
  registration in production, the birth count 3 higher, and the next slot 3 freed
  slots on); the +ord is freed first thing in the turn's run.  The peon is made in the
  run of turn 1141 (the port's in 1140): its tags as a unit made by script, then a
  `ptcf`, then the training's `` `rtl `` and `` `rcm ``, its two tasks, and the `ptcf`
  freed.  The melee hero-items trigger (TRAIN_FINISH, filterMeleeTrainedUnitIsHeroBJ)
  does not run for the peon in the original (no +tex), though the port runs it, its
  player-unit filters being taken as never asked; TriggerRegisterPlayerUnitEvent
  (`FUN_6f2178f0`) does keep the filter with the registration (`FUN_6f1ec650` of its
  fourth argument into `FUN_6f27a530`).  The port now does the same: the order's
  three objects as it is given to an empty queue, the time counting from the next
  turn, the made unit's objects freed at once with the ptcf, and a finished
  training's filter asked of the unit trained.  With that, a game where the original
  selected, trained a peon, put a unit in a group, and selected again
  (`docs/reference/bootybay_actions_game.txt`, its keepalives
  `bootybay_actions_keepalives.txt`) agrees to turn 518; its turn 519 sends a peon to
  mine (not yet carried out as the original does).  The traced training game agrees
  to turn 1141, parting at 1142 as the game without actions parts: at turn 1128's
  second step two walks give up (read under winedbg: the two sheep's searches found
  nothing nearer at 112.641 and 112.671, each then turned and held, and neither moved
  from 112.731), and the original makes both walkers' new objects and only then frees
  their objects.  A later step's walk ends' objects too wait for the step's end; with
  that the port's keepalives and trace agree without actions through turn 1201.
- A timer's handler (hdlr) is the timer's: taken with its first function and kept
  when it is started again with another (traced: Blizzard.j's restock timer, started
  at 120 seconds by StartStockUpdates with PerformStockUpdates, takes and frees
  nothing), and released after it when it is destroyed.  With that the keepalives
  agree without actions through turn 1287.
- The trace parted at turn 1219: a wanderer's arrival in a tick's first step had its
  path freed in the next step after a give-up there made its new objects.  An
  arrival's path (a wanderer's at its goal, a worker's at a structure) goes as the
  next step ends, before that step's walk ends' objects, the arrivals' paths in the
  order their walks ended; a give-up's as the next step starts (read with step
  marks, `trstep.sh`: turn 890, a give-up's path freed in the next step before
  another give-up's three objects; turn 1219, an arrival's after them).  A later
  step's end frees the last step's arrivals' paths, then the objects of the orders
  given at its tick, then its walk ends' objects (read: turn 1343, an order's eight
  freed after its step's give-up made its three and before that give-up's tree; turn
  932, after a peon's path).  With that the game without actions agrees with the
  original's trace to its end (turn 1361) and its keepalives through turn 1487.  The tags
  example prints each turn's steps with their main ticks (TAGS_STEPS), and its tag
  trace marks each step (`S k`).
- Gold mining (traced: a Booty Bay peon sent by a smart order to its gold mine, its
  tags, movement steps, waypoint provider, range tests, and speed writes read under
  winedbg in separate games; `crate::goldrun`):
  - Its walks are the engine's (`World::walk_steps`).  The way is searched toward the
    structure's middle cell and ends at the first node popped whose ring for the
    walker's class touches the structure's footprint (`FUN_6f148790` as
    `FUN_6f166310` uses it): from 670, 266 to the mine (676 to 683 by 260 to 267) at
    675, 265; home from 675, 265 to the great hall (650 to 661 by 258 to 269) at 663,
    265.  The path's goal (+0x1c) is the structure's middle, its last point the end
    node's middle.  The walk ends at that point as the wanderers' do (within 0.49,
    the provider's answer 3); within reach the walker stops and turns onto the
    structure's bearing, its heading then exactly that bearing (a turn within its rate
    sets it, read across 0: 5.990847), and arrives the next step.  The range tests
    after (`FUN_6f05b580`: range over 32 plus both position objects' +0x90 radii,
    0.5, 1.5625 for the mine, 5.5 for the hall, the places extrapolated) fail at both
    structures; their contact test (`FUN_6f058cb0`, the ring touching the footprint)
    passes.
  - The first walk, from the player's order, searches as other walks do (its path's
    mask +0x9c 0x02000002; the idle peons' regions, +0x34 0x10000ca, refuse cells).
    After the worker's first entry its orders carry task 0xd0150 and their walks'
    mask is 0x10000010, which no unit's region meets: the search, the line test of
    `World::way_ahead`, and the hold for units ahead all pass through other units
    (read: the walk home along row 265 past the idle peon whose region covers 672,
    264, never held and drawing nothing).
  - The walk's end is taken in the walkers' order (newest walk first) in its step:
    the objects an arrival makes are made then and those it frees wait for the step's
    end, in that order with the others' walk ends' (read: a hall arrival's freed
    order before a newer wanderer's walk end's tree, after an older one's).
  - At the mine (`FUN_6f437ae0`, in the step the walk ends): a worker's first entry
    makes a `+fgm`; the harvest order's move task and the task before it go as the
    step ends; its path as the next step ends (an arrival's).  It goes in for the mine's Agld DataB (1 second) on the agent queue,
    comes out where it went in when that is up (its walk home's first step the first
    step after), and its harvest order's tree goes with the order home's making.
  - Its speed is tuned at each arrival (constants read under winedbg: 10, 0.5, 1.2,
    5): a 10-second timer started at the last arrival gives the trip's time, less the
    mine's 1 second the travel; the factor that would have made the travel (and the
    last trip's, when there was one, each times its own factor) take 5 seconds less
    the mine's, clamped to 0.5 to 1.2, less 1, is the new bonus, added to the
    movement ability's factor (+0x78; `FUN_6f5fb7c0`, `FUN_6f5fc900`).  No tuning at
    the first arrival.  Read: 6.041100 and 6.025844 cells a second after trips of
    5.069793 and 4.979798 seconds.
  - At the hall, in the step the walk ends (`FUN_6f438880`): the gold is given and
    the worker's own harvest order made, its `^clg` freed at once; the order home's
    tree, then the new order's five, go as the step ends; the walk home's path as the
    next step ends.
  - A unit trained is made before the tick's first movement step (read with step
    marks: a peon trained made as its turn began, a worker's arrival and a
    wanderer's walk end in that turn's last step freeing theirs after).
  - The orders' objects: a player's harvest order 19 (`ordt`, two `tskA`, four
    tasks, three `tskt`, a task, a `tskt`, three tasks, `tskA`, `^clg`, `^clu`,
    `l^pa`), five of them freed as the next turn starts; the worker's own from the
    hall 19 (one `tskA` at its head, four tasks at its tail); the order home 16.
  With this the mining game (`docs/reference/bootybay_mining_*`) agrees with the
  original's trace through turn 1261 and its keepalives through 1281; a step-marked
  game of mining and a training (`docs/reference/bootybay_mining_training_*`) to its
  end (turn 1234); the game of selections, a training, and mining to turn 960.
  Open: a game trained before and during mining parts at turn 1125, a worker's
  arrival in a tick's first step freeing its path before a give-up's new objects in
  the next step (no step marks in that capture to place them).
- The data modules (4, 5, 7, 9; `FUN_6f042da0`) are computed once: `rol(sum, 3)` of the
  byte hashes of every cell of a database (`FUN_6f1996c0`: an `I` or `F` cell by its
  four bytes, an `S` by its characters).  No single SLK file's sum matches; which
  tables and how their cells are typed is not yet read.
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
  steps at +0x538; measured: 4, 3, 3 steps a 100 ms turn, ten each 300 ms).  Traced at
  that resolution (Booty Bay, two players): a turn's game logic runs after its first
  step (a wanderer's order issued there, its objects made and nine freed at once); a
  move ends after the step where it does, its path's `l^pa` and `^clu` freed after
  the next step (in the next turn when the move ended in its turn's last).
- The engine simulates with its own float routines (`efloat.rs`): add and subtract
  (`FUN_6f06fbb0`, `FUN_6f06fa90`) on doubled signed mantissas, the smaller exponent's
  shifted right arithmetically (so truncating, and down for negatives), dropped 24
  or more apart; multiply (`FUN_6f06f9c0`) keeping the product's high word; divide
  (`FUN_6f06fcd0`) as a times b's reciprocal (`FUN_6f0711e0`, a 1024-step table at
  0x6fa810c0, interpolated by the mantissa's low 13 bits spread over a word), 1 when
  a equals b; integer to single truncating (`FUN_6f070d80`); single to integer
  toward zero (`FUN_6f070120`); floor (`FUN_6f070c80`); a zero exponent read as 0.
  Also its own square root (`FUN_6f071480`, from a bitwise integer root,
  `FUN_6f071530`, and the constants 0xb504 and 0xb505), a bounded single to integer
  (`FUN_6f070170`), and an exponent-based helper (`FUN_6f06ffa0`); the math routines
  lie in 0x6f06f000 to 0x6f072000 and 0x6f1d4000 to 0x6f1d6000.
- Movement, each step (`FUN_6f15aa80`): the paths' budgets (`FUN_6f167310`), then each
  move's cluster group and cluster (`^clg`, `^clu`; `FUN_6f16c220`, `FUN_6f16e1f0`,
  `FUN_6f16c150`), each cluster's members (0x2c bytes at +0x28, `FUN_6f16c250`) moved
  one by one (`FUN_6f16a790`: the velocity's length and heading, then steering toward
  the waypoint, `FUN_6f16fbd0`), the others' regions flagged 0x40000000 meanwhile (so
  excused); then each position's turn toward its heading (`FUN_6f1705c0`, its rates
  from a table by class at 0x6fd541e0); and every other step separation
  (`FUN_6f1702f0`).  The steering (`FUN_6f170880`): the speed is 0 when stopping or
  while the angle to the way on is at least the window at +0xbc, else grows by +0xb4,
  and the heading turns toward the way by at most +0xb8 (`FUN_6f173720` the angle
  difference wrapped to -pi to pi).  The sheep sampled each step fits it: its heading
  (not its shown facing, which turns on its own) turning 0.5 a step (its turn rate),
  moving from the step after the angle left fell under 60 degrees, its first step
  about the cosine of the angle still to turn.  Read in its steering object
  (measured, cell units): the sheep's top speed 3.125 a second (100 units) at +0x88,
  its speed's growth +0xb4 the same (full speed at once), its turn +0xb8 0.5 a step,
  its window +0xbc 1.047197 (60 degrees); its heading 2.027 turning 0.5 a step to
  4.527, its speed 3.125 from the step the angle left (0.685) fell inside the window,
  its heading then 4.7314 toward the waypoint.  A step at full speed moves 3.125 *
  0.03 cells, the 3 units measured.  A unit's place read by the script (GetUnitX)
  is its step's place carried on by its velocity for the time since that step (the
  script's samples, 10 ms after each step, a third of a step on: the first moving
  sample 1 unit).  Positions are not integrated each step: a position object keeps a
  place (+0x78, +0x7c), a velocity (+0x80, +0x84; its speed +0x88 clamped to the top,
  `FUN_6f15fc70`), and the clock when last placed (+0x70 float, +0x74 epoch); a change
  of velocity (`FUN_6f15f7e0`, each step from the steering, `FUN_6f16fe20`: speed
  times the cosine and sine of the heading, less the velocity now) first moves the
  place on by velocity times the time since (`FUN_6f1603d0`, engine floats), then
  adds it (`FUN_6f1606e0`), its region flagged moving (0x20000000) while the speed is
  above 0.  The engine's game clock (at DAT_6fd53a48 + 0x54, epoch + 0x58, epoch
  length 300 at + 0x5c) gains 0.03 each step by the engine's addition (read:
  1.019999, 1.049999, 1.079999, ...); a time difference (`FUN_6f15cea0`) is the floats'
  difference (tiny ones 0) plus whole epochs times their length.  A second clock (+0xa8,
  real time) serves objects flagged 0x80000000 (local ones).  A waypoint is reached
  (`FUN_6f16e910`) within the radius at +0xb0 (0.49 cells for the sheep's, 0x6fcd53f0)
  with the heading off it by at most 0.2 (0x6fcd53d4; angles under 2e-7, 0x6fcd5470,
  count as 0): the sheep's last step put it 14.05 units from its node's middle (one
  earlier, 17.05), where it searched again and stopped.  tools/ghidra/Sigs.java gives the
  float routines their signatures before decompiling, so their arithmetic reads.
- The first wandering sheep ordered by script, sampled each step (its facing; place):
  facing 116.2, turning 4, then 8, then 11.46 (0.2 radians) each step; still for 5
  steps, then moving 0.98 units, then 3 a step (speed 100), its heading bending from
  259.6 to the waypoint (the middle of cell 430, 45, at 1232, -4944) whatever it
  faces; its last step 2 units, to 1231.73, -4929.95, and its order done two steps
  later.
- After a search finds nothing nearer, the unit stands (speed 0, the steering's stop
  flag) and turns toward its order's goal, done at the first step its heading is
  within 0.2 of it (traced: a pig boxed in by its herd, facing 246.7 degrees, turns
  toward its goal at 0.663 and is done six steps on; the first sheep after its second
  search, done two steps on).  Wanderers walk in this game by these steps
  (`motion.rs` through `World::walk_steps`): the sheep stops at the original's
  1231.73, -4929.95.  A turn runs its first step, then its logic, then the rest of
  its steps; timers (a wanderer's wait) fire at the first step more than 1e-4 past
  their time, and a walk ends at its step, its order's objects going then (traced:
  the third walk given after its turn's third step).  This game's ticks so run walkers'
  first step before their logic, fire wander waits at their step and start the walk
  with the next, and end walks at their step; fitted to the traced turns, a tick holds
  the steps at multiples of 30 ms from 10 ms past its start, and a wait counts from a
  step before the unit's making.  With these every allocation and free agrees through
  turn 69; at turn 70 the original replaces two sleeping creeps' spell timers
  (`` `rtl ``, made at setup by the time of day's change) at the turn's start: the
  Abolish Magic (ACd2) timers of the two creeps that also have Heal (Anh2) and Inner
  Fire (ACif), not those of the creeps with ACdm (the same data) and Anh1.  Traced:
  the spell abilities' event handler (`FUN_6f425aa0`) re-arms them (`FUN_6f422f40`,
  `FUN_6f4224e0`), the old one released by the deferred release.  These `` `rtl ``
  objects are not timers but threshold listeners on the creep's mana
  (CPoReThresholdLis; `FUN_6f27c060`), one a spell, at its cost less 0.1 (mode 0,
  made at setup) or more 0.1 (mode 1) (measured: 74.9 and 75.1 for Abolish Magic and
  Inner Fire, 4.9 and 5.1 for Heal, 99.9 for Shockwave, 89.9 and 90.1 for ACtc).  The
  turn-70 re-arming comes from the property system, a threshold crossed (backtrace
  through `FUN_6f1614fc`, `FUN_6f16186c`, the event 0xd01b7 to the spell).  Measured:
  the creeps' mana starts at 0; a High Priest's (0.75 a second) passes Heal's 5.1 at
  6.80 seconds and a Shadow Priest's (0.5) at 10.20, each then re-armed below its cost
  (4.9); the listeners are made below the costs (Heal, Inner Fire, Abolish Magic) and,
  the mana gone to none, again above them in the reverse order, so Heal's holds the
  last slot; listeners falling due together run in the order made, and one due at a
  turn's end in the next.  With these, every allocation and free agrees through turn
  76; with a wanderer's wait due at the last of a tick's four steps given in the next
  tick (that step being, it seems, the original's next turn's first), through turn 84.
  At turn 85 the original ends a walk given in turn 84's last step whose goal (13.7
  away) was within reach: at its first step, turn 85's first, unturned and before the
  turn's logic, only `tsk.`, three `task`, `tskO`, `tskA`, and `ordt` freed (no end
  objects made; the issue's nine went in turn 84 as usual), and its path (`l^pa`,
  `^clu`) as turn 86 starts.  Through turn 85 the port now agrees.
- The turns and their events, as the port now runs them (agreeing through turn 100):
  turn N holds game time [(N - 2) * 0.1, (N - 1) * 0.1), its movement steps the
  multiples of 30 ms in it (three or four).  A wanderer's order is given at its wait's
  exact end, among the steps by time (a step within 1e-4 of it first, as with timers):
  the first sheep (due 5.0359) after the step at 5.01, the third walk (6.4944) after
  6.48, a pig (8.4944) after the turn's last step, in that turn.  Its walk moves from
  the first step after it, but one given before a turn's first step moves from the
  second (a pig due 7.6012 walks from 7.65, not 7.62).  The next wait counts from the
  order's time (read in the original: a wait drawn at the clock 5.444844 as the last
  fell due).  An order's nine objects (`Tags::move_issued`) and a walk's tree at its end
  are freed at once, before the next order in the same turn (traced: two orders in turn
  99, a walk's end then an order in turn 101).
- At turn 101 the port ends two pigs' walks in the other order.  Read in the original
  (cells dumped at each search, `FUN_6f14ad50`): its searches see the same static cells,
  the same standing pigs (region flags 0), and walking pigs' regions with flags
  0x20000000 (which a wanderer's search excuses; the port now leaves walking units
  out); a search runs in the first step after its order (a pig's at the clock 7.6197
  for an order due 7.6012); and its relaxation, open and closed states, and nearest
  node agree with `lrpath`.  Yet at the clock 8.55 one pig stands on the straight line
  from its start (216.4, 59.4) to its way's last node (210.5, 61.5), not on the nodes'
  route: the way (`FUN_6f147dc0`) holds every node, from the start point to the end
  point, but the steering's waypoint provider (`FUN_6f165ae0`, `FUN_6f165b60`,
  `FUN_6f165f10`, `FUN_6f165d10`) resets the way's remaining count (+0x78 over the
  points at +0x60, the end first) to `FUN_6f167ae0`'s index: from the current point
  toward the end, the first at which the length summed along the way reaches 10 cells
  (`0x6fd54194`, set to 10 at startup; the sum from `0x6fd3c744`, 0), or 0.  Taken as
  "head for that point" it makes the pig's run straight but a sheep's longer way end
  33 turns early (its tree freed at turn 129 in the original), so what the count
  governs is still to be read (CLrPath keeps two ways, +0x50 and +0x70).
- Searches run within the movement steps, between units' steps: a wanderer's order is
  searched in the first step after it (read: the clocks 7.2597, 7.6197, and 8.5496 for
  orders due between the steps before them), so the port now steps wanderers step by
  step, each walker in turn, a new order's search in its step.  A sheep's region read
  at another's search (7.26) was unflagged (flags 0) though in the port that sheep had
  begun moving a step earlier (from 7.23, after turning from 4.33 toward 6.14): when the
  original's starts, or from what heading, is still to be found; the original's sheep
  then went east and searched again from (433, 46) at 7.56, its walk ending in turn 129.
  With a step's searches before its movement and only units with a velocity excused,
  the port's search now heads east too.  But the other sheep (1050117, searching from
  (431, 48) at 7.17 in both) still stood unflagged at 7.56 in the original and next
  searched from (432, 49) at 9.42, (431, 50) at 9.87, and (430, 50) at 10.29, where the
  port's reaches (432, 49) by 7.65: the two sheep's ways cross, and the original holds
  one back (its unit collision, not yet in the port).  Searches are spaced per path
  (`FUN_6f168910`: 10 counter units for kinds 0 and 1, 64 and 32 for others), the
  spacing reset when the class's nodes searched in the step stay under 64
  (`FUN_6f166e90`); a way ending short of its goal sets the path's flag 0x10000000.
- A path (CLrPath) holds two ways: a coarse one (points at +0x60, count +0x70, index
  +0x78), whose next target is the point some 10 cells along (`FUN_6f167ae0`) or else
  the goal (+0x1c, `FUN_6f167d70`), and the local search's (points at +0x40, count
  +0x50, index +0x74), searched afresh toward that target when used up
  (`FUN_6f167ce0`).  A wanderer's coarse way is empty, so its local search runs to its
  goal, as the port has it.  The member step (`FUN_6f16a790`) takes the unit's
  velocity (+0x80, +0x84 of its mover), steers (`FUN_6f16fbd0`), caps the speed at
  +0x88, and sets the member's flags 0x10000 and 0x20000 from the steering's answers.
- Units hold each other back (read, not yet in the port): the waypoint provider
  (`FUN_6f165ae0`) answers 1 while the path's wait counter (+0x94) runs down, a step at
  a time, and the steering then only turns (`FUN_6f16fbd0`, its stop flag).  Each step
  `FUN_6f165c60` asks `FUN_6f166140` whether the cells between the unit's place and
  where its velocity takes it hold other units' regions (by size class,
  `FUN_6f149560`, `FUN_6f1497c0`, `FUN_6f149b00`, `FUN_6f149e60`, into a list); for
  each moving one `FUN_6f168360` settles who yields: the unit no faster than the other
  (or of another class, or with flags that differ) notes it and waits at least 4 steps
  (`FUN_6f168070`), otherwise a wait of 20 (0x14) is set the other way.  Blocked and not
  waiting, it goes on to `FUN_6f167290`.  Traced in the original: sheep 1050117's
  path got the wait 20 before its step at 7.53 (its place 431.8735, 48.5884, as the
  port's), set by sheep 1050120 moving east at (3.1247, 0.0414), strictly faster in
  engine floats; then waits of 4 again and again as 1050120 passed.  The port now
  holds walkers back this way (standing units and walls in the way are not yet
  taken).  Read with it:
  - The cells checked are those a step ahead: the place plus at most one cell toward
    the target (`FUN_6f168280`, scaling to length 1 by the reciprocal), the footprint's
    leading edge there by the direction from the unit's cell (`FUN_6f167940`: 2 east,
    8 west, 4 north, 1 south; class 1's cells from `FUN_6f1497c0`).
  - A walker's target is not its next node: at the start and whenever a target is
    reached (within 0.49), it takes the next node, then each further node while a
    straight line to it is clear (`FUN_6f167bf0`, `FUN_6f168d30`: probes at whole
    cells along the line short of its end, each new cell's leading edge tested as a
    search tests cells, the first against cell 0, 0, since the original drops the start
    cell it works out).  It does not stop at a node it passes; only the way's end needs
    the heading on it (sheep 1050120's target at 7.53 was (432.5, 52.5), six cells
    on).
  - Walkers take each step newest walk first (1050120, ordered after 1050117, held it
    back before it moved in that step; pig 1050123 likewise held 1050124 at 8.70).
  - After a search the target is the way's end if the search refused no cell (the
    searcher's +0xd0, set by `FUN_6f1489a0`), else the node after the start
    (`FUN_6f168870`); the line test applies as each node is reached.
  - A walker blocked by a unit standing there or by a wall, and not waiting, counts down
    (+0x98, from 2 when its goal is within 12 cells and it walks alone, else 7 to 9 by
    a random draw, `FUN_6f1689d0`), holding, then searches again (traced: pig 1050124's
    count 1 at 9.93 and its new search from (211, 61)).
  - Units are placed on a quarter-unit grid, rounding down (pigs at -5719.4, -4560.2 and
    -5627.1, -4500.0 stand at -5719.5, -4560.25 and -5627.25, -4500.0).
  - A walk given before a turn's first step searches and moves in that step (the pig
    given at 7.6012 moved in the step at 7.62).
  - A held walker turns toward its order's goal (the member's +0x18; read: a held pig's
    heading 2.0233, its goal's bearing, its waypoint's 2.886), and arriving at the end
    of a way short of its goal it turns toward the goal too (`FUN_6f16e910` compares the
    heading with that bearing).
  - Blocked (not yielding), a walker sets its count and drops its way (`FUN_6f168740`:
    index -1, count 0, read at 9.42 after a block at 9.39), holding; the search runs as
    its next step starts (`FUN_6f167ce0`, the way used up) and it goes on if the new way
    is clear (sheep 1050117 searched and moved at 9.42).  With this the port follows
    1050117 and 1050120 through 10.26; at 10.26 1050117's and 1050120's speeds squared
    differ by one unit in the last place, and the original's 1050117 is the faster.
    The port's positions drift from the original's by 3e-6 from the step at 8.85, where
    its bearing to (432.5, 48.5) from (431.967072, 48.593716) is 6.1091104 and its
    velocity (3.0777721, -0.5412371) against the original's (3.077775, -0.541219): the
    acos and sine tables' emulation (`efloat`) wants checking bit for bit there.
  - The game clock moves by 0.005 at a time: its timer queue (`FUN_6f054190` adds the
    frame's time; `FUN_6f052380` sets the clock to each due event's time as it fires;
    `FUN_6f053710` reschedules a periodic timer at its time plus its period, by the
    engine's addition) has a timer every 0.005, so a movement step's clock is six such
    additions on from the last (read: the clock at steps 5.069871, 8.849516, 11.069143,
    32.395412, and 46.017925, each bit for bit the 0.005s summed from 0; the steps
    0.029998 apart from 4 to 8 seconds, 0.029995 from 8 to 32, 0.029983 from 32).  A
    step reads the clock before it moves on: the first step's clock is 0.
  - A step's velocity is the speed times the cosine and sine of the heading before it is
    wrapped into 0 to two pi (read: sheep 1050117 turned to -0.174075 at 8.85, its
    velocity (3.077775, -0.541219) that angle's; 6.1091104's table step truncates the
    other way).  A walker's goal in cells is the engine's difference of world points,
    divided (read: 427.847565, not the rounded 427.8475952).
  - A walker's local search (`FUN_6f166e90`) is spaced and budgeted: `FUN_6f168910`
    refuses it when fewer than 10 steps (the table at 0x6fa91c14, kind 0) have passed
    since the walker's last (the path's +0x7c, the engine's step count at
    DAT_6fd53a48 + 0x538); after it, its cost (the searcher's +0x6c, its loop's passes:
    33 for a sheep's search stopped by its limit of 32) is added to its class's budget
    (`DAT_6fd53ae4` + class * 0x70: the sheep's and pigs' class 15, word 0x000102bc,
    limit 1100 at +4, used at +8, renewed every other step by `FUN_6f167fa0` from
    `FUN_6f167310`), and the walker's +0x7c is cleared while the budget's use stays
    under 64.  Refused, its way stays dropped and it holds, turning toward its goal
    (read: sheep 1050122's search at step 0x54c took the budget to 66; it was refused
    at 0x553 to 0x555 and searched at 0x556; under the budget's limit check,
    `FUN_6f168310`, a walker over it is queued, not yet followed).
  - A wanderer's wait (CAbilityWander, handler `FUN_6f5a5b40`) is a timer, event
    0xd01bf, whose firing gives the walk (`FUN_6f5b03d0`); it is restarted, its wait
    drawn on the main generator (`FUN_6f5aa910`, vf189 `FUN_6f59b070`), as an order
    begins (0xd0178, `FUN_6f5af310`) and as one ends leaving the unit no order (0xd0148,
    `FUN_6f5af350`).  Read on the main generator (the object at DAT_6fd53a48): the first
    sheep's order drew at 5.034873 and its walk's end at 5.399847, the next wanderer's
    order at 5.619831 and its end at 5.789819, its next order 6.42 later (12.209).
  - A walk whose goal is within reach as it starts frees its order's objects at its
    first step (traced) but turns toward the goal until judged on it, as any arrival,
    and only then ends (read: a sheep 0.81 off its goal's bearing drew its next wait at
    8.369596, two steps after its first, 8.309607).  A new walk whose search finds
    nothing nearer turns in its search step and the next, judged facing from the third
    (read: a pig searched at 18.117960, on its goal's bearing at 18.147955 and still
    walking, done at 18.177950); one searching again is judged the step after.
  - A unit's place is kept in cells: a walk starts where the last left it, not from
    the world point converted back (which was one unit in the last place off).
  - A move order's objects (`Tags::move_issued`): its `^clg` is freed as it is given,
    the other eight after the engine clock's tick in which it was given (read: two
    wanderers' orders fired at the same tick, 19.867666, freed their eights together
    after both; two fired three ticks apart, 25.511719 and 25.526716, each after its
    own).  A wander timer's order shows the 0.005s tick nearest its due time when its
    wait was drawn in play (due 0.66 and 0.28 of a tick past one: both at the next;
    0.78 short of a turn's end: in the next turn), at or before it for a first wait
    (0.86 past: at it); yet among the steps it falls by its due time itself (0.36 of a
    tick past a step's: after it; 0.70 short of one: before it).  The mechanism (the
    agent's timer, `FUN_6f0608d0`, vtable slot 2) is not yet read; the port models
    the three observations as they are.
  - An arrival's path goes a step after its tree, as a give-up's does; after a turn's
    last step, it goes first thing in the next turn, before that turn's orders (read:
    an arrival at a turn's last step; a give-up's path freed before an order due ahead
    of the next turn's first step).
  - The agent queue runs one main tick ahead: while the main clock stands at a tick,
    the agent queue is brought to the next tick's clock (read: its clock written to
    10.009321 at the main clock 10.004322, the next tick's value, and to its events'
    times as they fire between).  The movement steps are a 0.03s timer on it, so a step
    is taken at the last main tick whose clock is short of the step's time, and moves
    units by that tick's clock: as the main clock trails more, from step 1164 (34.92s)
    on at tick 6k + 1 (read: step 1164 at main tick 6985; 1223 at 7339).  A turn holds
    the main ticks 20m to 20m + 19 (read: step 1223, at tick 7339, in turn 368, with
    the steps of 36.60 to 36.69).  A wander order falls likewise, in the main tick whose
    next clock first reaches its due, and among the steps by its time (read: due
    43.090098 at the next turn's first tick, 43.089535; due 19.868235 and 19.871306
    both at 19.867666).  An order's eight objects are freed after its tick's events,
    so after a step at that tick and the path the step frees (read: turn 436).  A
    walk's path goes as the next step starts, the next tick's first after its last.
    The port models all this (`World::plan_steps`, `World::agent_tick`); the trace and
    the keepalives agree through turn 501.
  - The wander timer is a CAgentTimer (embedded in the ability at +0x6c; started by
    vf2, `FUN_6f060190`: its due is its queue's clock plus the wait by the engine's
    addition, with a serial).  Its queue (`[DAT_6fd3c82c + 0x40]`, 0x2dc0088 in the
    traced run) is not the main one: its clock at a step is the sum of 0.03 per step
    by the engine's addition (read: 5.399984 at step 180, 34.529137 at step 1151, to
    within a unit in the last place), so it trails exact time by less than the main
    clock does (0.000863 at 34.53).  An order fires when that clock reaches its due
    (the queue's clock then the due itself: 5.035873 for a first wait of 5.035860),
    and the main clock then shows the tick during which it fired (see above).
  - The timers a creep's Hurl Boulder and Thunder Clap (ACtb, ACtc) make as its mana is
    first set are listeners on its mana rising past each spell's cost plus 0.1, the
    last spell's in the first slot (read: both Granite Golems' Hurl Boulder listeners,
    75.1 at 1.5 a second, replaced at 50.07 seconds, turn 502).
  - A wanderer's point is its place plus, on each axis, 500 times a draw less 250, by
    the engine's arithmetic, and a unit's place in world units is its place in cells
    times 32 plus the map's origin, likewise (read: a pig's goal 54.040680 in cells;
    rounded arithmetic gave 54.04065, one world unit in the last place off).
  - A listener on mana falls due on the game clock, which trails exact time: it fires
    in the main tick whose clock first reaches its due, among the turn's orders and
    steps by that tick (read: an Ogre Lord's 100.1 at 1.25 a second, due 80.08,
    re-armed in turn 803 before its first step, the clock then 0.025 short).  The
    listeners of a tick are all re-armed, then the old ones released (read: two High
    Priests' in turn 70, two made then two freed; the Ogre Lord's, one then one).
  - In a tick's later steps a walk's path is freed as the step after its end starts,
    and a walk end's objects at once.  In a tick's first step the walk ends' objects
    wait for the step's end, after the paths carried from the last tick (read: turn
    812, a give-up's three new objects made, then the last turn's path freed, then its
    tree; turn 668, the last turn's path before an arrival's tree; turn 101, in a
    later step, a path before a give-up's new objects).  Why the first step differs is
    not read.
  - Steps and orders tie: one due exactly at a main tick's clock is taken at that tick
    (read: step 2468, due 74.037521, the clock at tick 14813).
  - The blocked count's draw (`FUN_6f1689d0`: 2 for a goal within 12 cells and a walker
    alone, else 7 plus `FUN_6f1d62b0(2)`, 0 or 1) is on the generator at DAT_6fd53a48,
    the main one (read: the game object there, 0x2b300c0 in the traced run, is the
    generator every wander wait is drawn on).
  - Open: which cells a walker's region holds as it moves.  Read: sheep 1050120, held at
    7.53 to 7.56 after moving from 432.97 to 433.06, held cells 431 to 433 (both
    places); sheep 1050117 at 9.42 was blocked by 1050120, which had moved off that row
    in the step before and was then held.  But pig 1050124 at 9.57 was not blocked by
    pig 1050127 moving on steadily, whose place before that step lay on the cells
    checked.  A region spanning both places only while its unit is held after moving
    fits all three, and the port now takes it so (the trace agreement then reaching
    turn 118).
  - Within reach of the end of a way short of its goal, a walker does not arrive: it
    holds a step and searches again (traced: sheep 1050121 0.42 from its way's end held
    at 11.70 and searched at 11.73; the first sheep, 0.44 from it, held at 5.34 and
    searched at 5.37), then faces its goal.  Held so, and in the step of that search
    when it finds nothing nearer, it turns toward its goal without being judged done
    (read: the first sheep's heading 4.7315, 4.2315 at 5.37, 4.0887, its goal's
    bearing, at 5.40; it ends in turn 56); from the next step it faces the goal, done
    within 0.2.  The provider restores its entry target, the goal, before the blocked
    count (`FUN_6f165c60`).
  - Wanderers' orders due between steps are given just before the step after them,
    among the walkers' steps, and a walk's path (`l^pa`, `^clu`) goes as the next step
    starts (traced at turn 101: a pig's walk ending in step 1, an order due before step
    2, the pig's path, then another walk's end in step 2).
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
  port's own: the custom campaign buttons (`profile_vN.json`).  The original's
  SetCustomCampaignButtonVisible (`FUN_6f2125b0`) acts only within a custom campaign
  (`FUN_6f219630`), setting key `Button%02d` (`FUN_6f24d3b0`) to the value in section
  `Version%06d` (`FUN_6f247390`) of a table it writes to a file of the campaign's
  (`FUN_6f247160`, `FUN_6f2325c0`); a plain map's call changed nothing on disk or in the
  registry (measured).  Which file is still to be measured with a custom campaign.
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
  for the y it was made at), and a ten-bit mask of `FUN_6f2907e0`'s answers.  Its
  layout, read from the writer SyncStoredUnit's action uses (`FUN_6f331530`,
  `FUN_6f21b560`, `FUN_6f224e20`, `FUN_6f21b4d0`) and the same in the file: the type; a
  count of item slots (6, or 0 for a unit with no inventory) and each slot's type,
  charges, and third word; experience, the level less one, skill points, a word (its
  high half 1 in each seen, its low half 0 or 10), strength and its gain, agility, two
  reals (0, and 0.26 or 0.32 as agility was 13 or 16), its gain, intelligence and its
  gain; a count of abilities (5) and each one's id and level; three reals; a count of
  words (2) and the words; a real; and the mask as 16 bits.  A peon's is 90 bytes.
  The file (`FUN_6f246fe0`, `crates/formats/src/w3v.rs`) is written in blocks of 2048
  bytes, product W3XP (WAR3 for Reign of Chaos), version 27, build 6059, its length
  field never set; the record a word, then each cache's name, a word, and missions,
  each mission's name, five words, and its lists.  The words are the tables' counters
  (+0x28): 1 and 3 for the sample's one cache and three missions, 0 for each
  mission's tables though they held entries.  The port writes it from its profile's
  caches, missions and keys in name order, and the original reads it (checked: a
  value changed by the port read back in the original, its Paladin restored).
- Network games' commands: the original's actions for selection changes and orders to
  the selection (`crates/game/src/actions.rs`), alliances and trades (0x50, 0x51: the
  Allies dialog's flags with 0x400 the sender's own Allied Victory), and the script's
  inputs, measured on a test map with a trigger on each: a chat line (0x60, one for
  each chat event registration it matches, naming the registration by its id; the line
  itself goes as W3GS chat, scope 0 for all), Escape (0x61, always), a trackable's hit
  or track (0x64, 0x65, one for each registration), a dialog's button (0x6A with the
  dialog and the button, then 0x69 with the button and the dialog), an arrow key (0x75
  and its number, only when its event is registered), and game cache syncs (0x6B to
  0x6E with the file, mission, and key, then an integer, a real, a boolean as a word,
  or a unit's record; 0x70 to 0x73 with none for an integer, real, boolean, or unit
  not stored, 0x70's handler removing it from the mission's first table;
  SyncStoredString a bare 0x6F, or 0x74 for a string not stored).  Measured too: the card's
  Cancel while training (Order, flags 0x40, cancel 851976), a cargo slot clicked
  (OrderTarget, flags 2, unload 852047, the passenger as target; its point, 8640, 1664
  then 8576, 1664 for a Zeppelin's peon, multiples of 64 near where it boarded, not yet
  understood), and a dead hero awakened at a tavern (OrderTarget, flags 0x46, the
  unnamed order 852462, the hero where it lies).  The rest are still the
  port's own commands in a private action (0x7F) until each is measured; the objects' ids come from the
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
