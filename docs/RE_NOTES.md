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
- **Map object strings**: a `war3map.w3u` (and kin) text value `TRIGSTR_n` is the map's
  `war3map.wts` string n (`FUN_6f6ee6f0` parses the number), so a custom unit's name,
  tip, and tooltip show as written (Mafarazzo TD's towers are named only so) and
  GetUnitName gives them.  A leveled profile field (an ability's `atp1`, `aub1`,
  `aut1`, `auu1`: Tip, Ubertip, Untip, Unubertip, index 0, repeat 3; an upgrade's
  Name, Tip, Ubertip, Hotkey, Art) sets its level's entry of the comma list, as the
  strings files list them a level each.
- **Target flags** (`FUN_6f6a9350`, table of 51 name and bit pairs at `0x6fce61b8`,
  compared ignoring case): each flag also goes by its first four letters, and some by
  more names: `enem`/`enemies`/`enemy` 0x10000, `alli`/`allies`/`ally` 0x4000,
  `frie`/`friend` 0x6000 (player and ally), `nots`/`notself` 0x1e000 (player, ally,
  neutral, enemy), `tote`/`totem` and `ward` both 0x10, `_` 0xfdd1f00e; an unknown
  name logs `Unknown Target Flag`.  The editor writes the plural forms (`enemies`).
- **A map's data set**: a map whose w3i flags lack melee (0x4) loads the Custom data
  set, whatever its players (measured: Mafarazzo TD, flags 0xdc78, its keepalive's
  data modules 4, 7, and 9 the Custom_V1 tables' at turn 1, the melee ones' not).
- **Forced start locations** (ForcePlayerStartLocation, `FUN_6f1e0510`): the location
  is marked taken and forced, its player placed; the game's placement
  (`FUN_6f1e9fd0`) leaves both, placing only the others on the locations left (its
  Teams Together pass clears the taken marks of locations not forced).  Mafarazzo TD,
  every player forced by its slots' setup, draws nothing for the placement: the
  original's 16 draws of the main generator before the first turn are other draws
  (measured with a breakpoint on `FUN_6f1b7130`), where the port had drawn 20 more.
- **Destructables without pathing** (a pathing texture empty or `_`): two pathing
  regions as a dead one, not five (measured: Mafarazzo TD's two OTds, its keepalive's
  births and presence agreeing at turn 1 only so).
- **The time of day's listeners** (`` `rcm `` and `` `rtl ``, made as the setup ends):
  only while the time of day runs.  SuspendTimeOfDay(true) frees them and sets its
  bit (`FUN_6f28bb00`), (false) clears it and makes them again once no bit is left
  (`FUN_6f28bb80`, `FUN_6f28bbf0`); a map that suspends it before the setup's end
  (Mafarazzo TD's UseTimeOfDayBJ(false) in its map initialization) has none
  (measured: its births 1430 at turn 1, not 1438).

## Scripts

- Native registration: `FUN_6f207490` (common.j, 1,167 entries:
  `push sig; mov edx,name; mov ecx,impl; call 0x6f90d120`) and `FUN_6f9bdec0`
  (common.ai, 124 entries).  Natives are cdecl; reals are passed by pointer.
- Order ids: `FUN_6f487cb0` registers 347 `(id, name)` pairs; `smart` (851971) is the
  constant at `0x6fb77ddc`.
- RNG: state update `FUN_6f1b7130`, seeding `FUN_6f1cc4e0`, table at `0x6fa92f10`.
  `GetRandomInt` (`0x6f201e30`) returns `lo + rand*(|hi-lo|+1)>>32`, which exceeds
  `lo` when `hi < lo`.
- `GetRandomReal` (`0x6f201e70`) computes in the engine's own arithmetic
  (`FUN_6f06fa90`, `FUN_6f06fbb0`, `FUN_6f06f9c0`): the first bound plus the bounds'
  distance times a draw in [0, 1) (the draw's bits as a single in [1, 2), less 1), so
  past the first bound when it is the greater, as `GetRandomInt` is; the first bound
  when the two are nearer than an epsilon.  Measured: Mafarazzo TD's 120 first creep
  speeds (GetRandomReal(speed - 50, speed + 50) each, a breakpoint on the move
  ability's speed setter `FUN_6f5fee60`) all the port's to the last place only so;
  with the processor's rounding four were a unit off there.
- `StringHash` is Storm ordinal 590: Jenkins lookup2 over the upper-cased string with
  `/` mapped to `\`.
- `R2S` formats with 3 decimals; `S2I` is `atoi`.
- Invalid-player defaults: `GetPlayerController` → 5, `GetPlayerRace` → 1.
- `GetCameraMargin` reads `UI\MiscData.txt [CameraMargins]` × 128.
- TimerStart (`FUN_6f249ca0`) gives a timer its handler (`hdlr`) the first time it
  starts, with a function or none (traced: Mafarazzo TD's StartTimerBJ, its callback
  null, the handler made before CreateTimerDialogBJ's `+tid`).
- A global's value set by its initializer is no counted reference of its handle
  (Blizzard.j's timers and groups, count 1 each, their objects' own); a store over it
  still lets one go, so that a value nothing else holds is destroyed: StartTimerBJ's
  `set bj_lastStartedTimer = t` frees Blizzard.j's first timer (traced: Mafarazzo TD
  at 18 seconds, its `+tmr` freed after the Timer trigger's timer dialog and before
  its execution, both at the turn's end).
- A timer's expiry registration (TriggerRegisterTimerExpireEvent) makes the event's
  data (`wsed`, CTimerExpireEventData) as the timer expires, gives it a handle while
  its trigger fires, and lets the handle go after (CTimerExpireEventReg's vf3,
  `FUN_6f27cfe0`: `FUN_6f282e30`, its vf 0x10, `FUN_6f283f40`); the data goes at the
  turn's end after the execution (traced: Mafarazzo TD's New_Round at 20 seconds).  A
  timer event's (TriggerRegisterTimerEvent, CTimerEventReg) makes none.  The timer
  fires its registrations (`FUN_6f240350`, the timer's vf 0x10) before its callback
  (through its handler, `FUN_6f1f83e0`); the port runs the callback first
  (provisional: no capture has both).
- A computer player's AI in its campaign mode (flag 1 at +0x2d0, until a script's
  SetMeleeAI, which common.ai's InitAI calls) gives each unit it takes a guard post of
  its own where it stands (`FUN_6f9c3660` → `FUN_6f9c72d0` → `FUN_6f9c3270` →
  `FUN_6f9c2ee0`, an empty post of its type at that point taken instead; heroes,
  structures, and others aside), its `AIgp` made between the unit's making's two
  orders.  Measured on Mafarazzo TD, whose computer runs no script: a breakpoint on
  `FUN_6f9c2ee0` stops for each creep, from `FUN_6f9c3660` through `FUN_6f9c72d0`,
  and the flag's setter `FUN_6f9caf10` is never called; 180 posts at the first wave,
  none freed by turn 323.  A melee map's computers take none before their scripts
  start (Forestwalk's undead computer agrees only so; provisional: that the map's
  melee flag is what decides).
- Leaderboard items by index (`FUN_6f2af270`): none for an index below 0 or past the
  last, so that LeaderboardSetItemValue, Label, Style, LabelColor, and ValueColor do
  nothing then.  LeaderboardGetPlayerIndex (`FUN_6f20d880`) is -1 for a player without
  an item, a null one too: Blizzard.j's LeaderboardSetPlayerItemValueBJ for a killer's
  owner when a unit dies unkilled (Mafarazzo TD's kill count on a creep exploded at the
  end of its path) changes nothing.
- Leaderboard sorts (`FUN_6f3d2980`, by value `FUN_6f3d2ba0`, by player, by label): the
  items into an array, the C runtime's qsort over it (Visual C++'s: a median-of-three
  quicksort, runs of eight or fewer by a selection sort that moves the first of the
  greatest to the end, so ties are not kept in order), the board emptied and the items
  added back in that order.  Its comparisons are differences: the value (item +0x54)
  or the player (+0x64), the second's less the first's when descending; the labels by
  Storm's ordinal 508.
- Reincarnation (CAbilityReincarnation, CAbilityCreepReincarnation, the creeps' ACrn):
  a dying unit's death (`FUN_6f679bb0`) marks it dead and runs its death's course
  (`FUN_6f678280`), where each ability's dying hook runs by priority (`FUN_6f48e3e0`,
  vf 0x280 and 0x284): Reincarnation's, ready (its cooldown done; the creeps' version
  `FUN_6f5197e0`, the hero's `FUN_6f519bf0`, neither asking whether the unit is a
  hero), starts it (vf 0x3e8, `FUN_6f51fd50`), which first forgets the unit's killer
  (`FUN_6f6989c0(0)`: its +0x250 and +0x254), puts the effect XOre on it
  (CEffectReincarnation, whose vf30 `FUN_6f518510` calls vf31 `FUN_6f5253c0`, clearing
  the unit's flags 0x10, 0x200, and 0x800 at +0x64, its type counted and its food used
  and made, without giving them back), sets its flag 0x1000, and empties its queued
  work (`FUN_6f673fe0`).  The death's work is queued on the unit as a stack
  (`FUN_6f691f20` puts a task first on its list at +0x174; `FUN_6f67df00` runs them,
  the last put first, at once, till one waits): the course's own (its removal 0xd015a,
  a burst's art 0xd0157, a corpse's decay 0xd0158) before the hooks, so that a unit
  starting to rise loses them; then 0xd0154 and the corpse's wait (0xd0155, or 0xd0156
  for a burst, no wait), and the death's 0xd0153 (`FUN_6f69c210`: its bounty,
  experience, and score, which ask the killer, so none for a fall to rise from; then
  `FUN_6f6744d0`, giving back the food only of a unit whose flags still have it, so
  none for the fall, and firing its death events only for a unit without XOre,
  `FUN_6f48e180`; see below).  The rising (CEffectReincarnation's vf32 `FUN_6f50e870`)
  revives the unit (`FUN_6f695620`, which empties its task list, clears its flags
  0x2c00 at +0x5c, SetUnitExploded's 0x2000 among them, and with its flags 0x500 adds
  its food used and made, setting the unit's flags again) and takes the food used and
  made away again: the food stays the unit's all along.  The creeps' hook with the
  ability not ready (its cooldown) empties the work too and puts its removal and a
  dissipation (0xd015b, `FUN_6f69c2d0`, the effect `Xdis`) in its place.
- A unit set to explode (SetUnitExploded, the flag 0x2000 at +0x5c, `6f215110`) and
  killed takes the course's burst path, where the dying hooks run as well
  (`FUN_6f678280`, after its art's task is put); the creeps' Reincarnation's checks
  (`FUN_6f5197e0`: the unit's flags 0xc0000000 at +0x5c, 0x40000000 given at its
  making by its flag 0x10000, `FUN_6f677870`, read as an illusion's, and 0x80000000 by
  `FUN_6f4e66f0`; its +0x20 bit 0; the effect not on it already, `FUN_6f50d710`; and a
  pathing test at its place, `FUN_6f04e090`) ask nothing of the flag, so that one ready
  rises: its burst's art and removal are emptied with its queued work, and it lies and
  rises where it fell.  Mafarazzo TD's level 30 and 40 bosses, exploded as they reach
  WalkEnd, so rise at its edge 7 seconds later, idle and without orders (the round goes
  on while they stand, their food kept), and attack a structure within their
  acquisition range (1200), else stand.
- A region's enter and leave events (TriggerRegisterEnterRegion's registration on the
  region, 0x401900c8 and 0x401900c9, `FUN_6f27a820`) come of a unit's place moving to
  another cell (CPoPosCl's vf 0x54, `FUN_6f16fa00`: the regions over its old cell and
  its new, those in both left out, `cp&l` sent to the old ones and `cp&e` to the new,
  CTriggerRegion's `FUN_6f05fe00`), its cell first none (`FUN_6f170d30`, from its
  making, `FUN_6f16ea70`), so that a unit made inside a region enters it.  A death and
  a rising in place change no cell: a unit risen inside a region does not enter it
  again (provisional: the rising places it where it lies, vf 0x180, read as keeping
  its cell).  IsUnitInRegion asks the place alone (`FUN_6f05f9d0`).
- A unit's death's objects (`FUN_6f679bb0`, read against the flyer game's creep killed
  by the Arrow Tower's eighth missile, its landing in the main tick 27.381405, turn
  274, `abtest/mafb.*`): the structures attacking it give it up (their tasks
  destroyed, the attack's first, then the rest top first, `FUN_6f49d280`; a damage
  point still to come called off, and the cooldown with it, `FUN_6f497e20` stopping
  the timers at +0x1ec and +0x1d8), its AI's `uref` and `AI-U` go, its orders are
  cleared (`FUN_6f673610`: a block made and destroyed, its walk's task stack
  destroyed top first, then its `ordt`), its death's course is put on its stack
  (`FUN_6f678280`: the removal 0xd015a, a corpse's 0xd0159 and 0xd0158, 0xd0154, and
  the corpse's wait 0xd0155; a burst's 0xd015a, 0xd0157, 0xd0154, and 0xd0156), then
  0xd0153, 0xd0178, 0xd0144, and a block to state 4, and run, the last put first: the
  block, 0xd0144, and 0xd0178 go; 0xd0153 (`FUN_6f69c210`) gives the bounty and runs
  `FUN_6f6744d0`, its regeneration's `` `rtl `` going and its death events firing,
  the owner's with its data (CPlayerUnitDeathEventData, `pudd`, `FUN_6f260ab0`, made
  only when the event is listened for, let go after its triggers' `+tex`), then goes,
  and its walk's path and cluster go as the corpse's wait begins.  All 28 go in that
  order with the turn's releases; the tower, which looks again 0.01 after the death
  (`FUN_6f49d000`), takes up its next creep after them, its tasks' last block freed in
  that turn too, and sends its first missile at it 0.4 seconds on (turn 278).  The
  corpse's wait ends as its death has played, its type's death time on (3, turn 304),
  0xd0155 and 0xd0154 freed then.  The death's effect (`+EIP`, `` `pos ``, two
  `^pmr`) is the second table's.  The port so makes them (`World::death_begun`,
  `World::death_ended`, `World::looks_after_deaths`; provisional: the corpse's two by
  a type whose corpse decays, its flag 0x200 not read; a current order other than a
  walk's not cleared; a hero's, an illusion's, and a burst's left as before, and
  KillUnit's own as that native makes them).  With these the flyer game's objects
  agree to turn 311 (from 273; the second table's left out of the original's trace),
  where the original frees four of the map's `pcvt`, the port none.  Open: its
  keepalives part still at turn 275, its objects, births, and next slot the
  original's, so in its generators or its tech trees, not read.
- A kill's bounty is drawn on the random data's streams (`FUN_6f66a670`): the lumber's
  dice on 44 (`FUN_6f69e5b0`), then the gold's on 0 (`FUN_6f687490`), each die a draw,
  `r × sides >> 32` and one (`FUN_6f695c70`, `FUN_6f1ec180`), plus its bountyplus;
  none without dice (the port had drawn on the main generator).
- A unit's death events (EVENT_PLAYER_UNIT_DEATH, 0x80214 to the owner,
  `FUN_6f260ab0`; then EVENT_UNIT_DEATH, 0x80235 to the unit) come in `FUN_6f67b8a0`,
  from the death's 0xd0153 task after its bounty and its food given back
  (`FUN_6f6744d0`), only for a unit that is not to rise: a fall to Reincarnation fires
  none.
- Player state events (TriggerRegisterPlayerStateEvent, `6f2177d0`): the registration
  (CPlayerStateEventReg, `FUN_6f27a5f0`) puts a listener (IntegerListener,
  `FUN_6f056a60`) on the player's state (its integer at +0x40 + 0x10 times the state:
  food made 4 at +0x80, food used 5 at +0x90).  Setting the integer (`FUN_6f056ae0`)
  tells its listeners at once if the value changed (`FUN_6f160e80`, `FUN_6f1610b0`,
  the newest listener first, as `FUN_6f15e160` puts each first; the flag listener's
  `FUN_6f160a50`; `FUN_6f055950` delivers the event at once), and the registration
  (`FUN_6f27cd00`) compares the value now (gold and lumber in tenths divided by ten;
  the gathered states' limits taken in tenths) by its op (`FUN_6f1fdf80`: equal within
  an epsilon) and fires its trigger if it passes (CPlayerEventReg's `FUN_6f27cba0`),
  the state in the event's data (GetEventPlayerState).  So it fires on every change to
  a value its comparison passes, not only as the comparison becomes true, inside the
  code that changed it: a unit's food is given back in its death's 0xd0153 task
  (`FUN_6f6744d0`: food made, then used) before its death events, so Mafarazzo TD's
  End Round (Player 11's food used equal to 0) runs inside the KillUnit of the last
  creep, before its death's triggers, and for a creep exploded at WalkEnd inside that
  trigger, before Losing life (the region's next trigger) takes its life.
- ForceEnumPlayers, ForceEnumAllies, and ForceEnumEnemies (`FUN_6f22ef20` through
  `6f1fe750` and `6f1fe790`): players 0 to 11 whose slot is not empty (+0x270, so a
  player who left is among them), the neutral players never; the filter asked first,
  then for allies IsPlayerAlly(the player enumerated, the one given) (`FUN_6f1e2b60`,
  its passive mask at +0x38 of its alliances, as `6f205a90`), for enemies its
  negation.  A player is allied to itself, so it is among its allies and not its
  enemies.  Blizzard.j's GetPlayersAllies and GetPlayersEnemies make a force so:
  Mafarazzo TD's GetPlayersEnemies(Player(11)) is players 0 to 10 not empty, its
  GetPlayersAllies(Player(0)) the same.
- A unit hurt runs from its attacker (CAbilityMove's damage hook, vf63
  `FUN_6f5fa400`): one whose move is not disabled (`FUN_6f5fb040`), hurt for some
  damage (its flag 2 not set) by a living attacker it sees (`FUN_6f66fdd0`) not allied
  to it (`FUN_6f68bfa0`), runs if its owner's AI would have it flee (`FUN_6f607ba0`)
  or if it has no order (`FUN_6f419fb0`) and no attack (+0x1e8), its attack disabled,
  or no target taken (`FUN_6f49cf60`, the attack's own hook taking the attacker first
  when it can).  The run (`FUN_6f5fc350`) is for a unit that may flee (canFlee,
  `FUN_6f66efa0`), straight away from the attacker (along its facing were they at one
  point): for a unit its owner's AI has taken (+0x5c flag 4, set as a computer's or a
  neutral player's AI takes the unit, `FUN_6f9b9230`) the attacker's reach and 100
  more (300 from an attacker without an attack, or for a Hero below 200 or a third of
  its most) on a move order, then the order 0xd0006 queued after it (`FUN_6f693450`);
  for others 300, on an attack order (a move order for a worker).  The order 0xd0006
  runs as the unit's AI's look at it (`FUN_6f9bd6e0` → `FUN_6f9cd390`; a unit whose
  owner has no AI just stops, `FUN_6f690b80`): a worker to its harvest, a captain's
  unit to its captain (`FUN_6f9d8a90`), else a unit keeping a guard post more than 50
  from it attack-moves back (`FUN_6f9c0b00`, ReturnGuardPosts' too) and is looked at
  again after.  No timer: the AI looks as the run ends.  Traced on Mafarazzo TD
  changed to make a Plague tower and an idle fort runner without a weapon (Player 11,
  its post where it was made): the runner's first hit at turn 9 gives it a move order
  and a walk; the tower's attack ends at 48; at 84 that walk's objects go with an
  attack order and a new walk made (the run of 1100, the tower's reach 1000 and 100,
  at its slowed speed held to 150, ending then); the tower attacks again from 119; at
  139 that walk ends, and at 141 the runner is ordered off again.

## Rendering

- Cliff models: `Doodads\Terrain\<dir>\<dir><TL><TR><BR><BL><var>.mdx`, letters
  `'A' + layer - min` (`FUN_6f744560`).
- A corner by a cliff or ramp shows its cliff type's ground tile in place of its own
  (`FUN_6f75f0b0`, run over each changed region, `FUN_6f734f70`, `FUN_6f73f190`, keeps it
  in the corner's texture byte's high nibble, 0xf for none, 0xe for its own;
  `FUN_6f760b00` hands it to the tile layers, `FUN_6f73bf20`).  Of the four tiles about
  the corner (its own, left, below, below left; offsets at `0x6fb85aa4`): with one
  bearing a corner lettered R or S (`FUN_6f74b760`), the first of those that is not a
  whole ramp (`FUN_6f7506d0`: R counts 1, S 2, a whole ramp more than 1) gives
  `CliffTypes.slk` groundTile, its type that of the highest corner of it and of the
  ramp-bearing tiles right, left, above, and below (`FUN_6f7443b0`, offsets at
  `0x6fa9f490`, `0x6fb86bd0`), and with none the corner keeps its own; else the first
  whose corners' levels differ (`FUN_6f74b950`, `FUN_6f74a670`) gives groundTile at the
  tile's lowest level and upperTile above it (`FUN_6f7439f0`), its type the tile's first
  corner's, else the nearest within two (`FUN_6f742a90`).  A tile the map lacks is added
  after its own (`FUN_6f740650`); "_" shows none.  So Mafarazzo TD's raised White Marble
  lanes edged with Ashenvale cliffs show grass (CAgr's Agrs) on both levels of each
  cliff, not the marble's border cells.  (Left out: a corner with no cliff or ramp about
  whose neighbors carry the record's flag 8, `FUN_6f74b9d0`.)
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
  `WorldFrameChatMessage` (CGameUI +0x3ec, `FUN_6f367710`): the chat font (option 0x4e,
  else MasterFont, `FUN_6f2bfb60`) at its FontHeights (0.013), from the screen's top
  down to 0.2 above its bottom, 0.00625 in from the left and 0.02 from the right.
  Each is a CSimpleMessageFrame: as many whole lines as its height holds (vf0
  `FUN_6f116290`), laid a line apart from its top, the newest lowest (`FUN_6f115e60`,
  `FUN_6f116900`), so the height's remainder lies under the lowest line: 0.005 for
  the error frame, 0.01 for the unit and chat messages (measured from the original's
  LAN capture: chat baselines 0.2153 and 0.2283, the unit messages' lowest 0.2457, the
  error line's 0.191).  So the chat lines sit in their own block at the lower left,
  above the hero portrait and under the unit messages.
- UI\MiscUI.txt [FontHeights] values all carry a "// comment"; the game reads the number
  before it (`FUN_6f047680`).  (The port had read WorldFrameChatMessage, 0.013, as its
  default, 0.015.)
- Text keeps its shape on a screen wider than 4:3 while the interface stretches across
  it, so text measured for layout (tooltip widths and wrapping, unit tip boxes, the chat
  bar's prompt) is narrower in interface units by the stretch: on the original's 16:9
  capture the HSBC tooltip wraps "Build this gives  50%" on one line, the "Enemy" tip box
  is 65 pixels wide, and "To Allies:" is followed by the typed text 0.002 after it.
  The chat bar's box blends as the tooltips' do (`FUN_6f39cb10`, `FUN_6f0fa000`'s last
  argument; the original's shows the grass through it).
- Error texts: `Units\CommandStrings.txt [Errors]`; per-race lists run Human, Orc,
  Undead, Night Elf.
- Acknowledgments (`FUN_6f697c30`, `FUN_6f690dd0`): three consecutive "What" lines on
  the same unit, then "Pissed" lines in order; the counter resets on another unit or
  when the Pissed list runs out.  Suffix table at `0x6fce5734`.
- Script dialogs (`FUN_6f3c9550`, `FUN_6f3aa500`): height = 0.03 margins + message +
  0.011 + buttons (0.003 apart), rounded up to 0.048 steps, at least 0.096; the
  message is centered in the extra space.
- CTextFrame starts with white text and a black shadow.
- Command card slots (`FUN_6f3aad50` placing each button with `FUN_6f3aa430`, the 4 by
  3 grid at `+0x154`): a button whose Buttonpos slot is taken goes to the next free
  slot after it, left to right and row by row, else the nearest free one before it;
  with none free it is left out.  So towers made from one base type, which share its
  Buttonpos, all show in a builder's menu (Mafarazzo TD's peasant: Poison, Ice, and
  HSBC Towers all ask for 3,0).  A unit type without a Buttonpos (a sheep, nshe) asks
  for 0,0 (provisional: the field's default).
- The command bar is refilled for the input mode on top (`FUN_6f3c6c40`: the selected
  units' abilities told the mode through `FUN_6f67e4c0`, `FUN_6f67e570`, vf112 and
  vf113).  Move and Attack add theirs only in the select mode (`CAbilityMove::vf112`
  `FUN_6f5fee80`, `CAbilityAttack::vf112` `FUN_6f49c550`, `param_2 == 0`), a spell only
  in the select mode or its own or its book's (`FUN_6f430a00`); so while a target or a
  structure's place is awaited (`CTargetMode`, `CBuildMode`) the card holds Cancel
  alone (`CmdCancel`, order 0xd0007, `FUN_6f40d100`; provisional: which ability adds it
  in those modes is not read).
- `CAbilityAttack::vf112` adds Attack (0xd000f) and Stop (0xd0004), and Attack Ground
  (0xd0010) for an artillery weapon, whether the unit moves or not: an armed tower shows
  them.  The in-progress abilities' vf108 (`FUN_6f42a3c0`, `CAbilityBuildInProgress`
  and `CAbilityUpgradeInProgress` alike) add `CmdCancelBuild` (0xd0008) alone.
- A structure's place clicked (`FUN_6f3e0790` → `FUN_6f3e13a0`) is checked there
  (`FUN_6f66f050`); refused, the mode goes on and `FUN_6f36d390` shows the error
  (`FUN_6f370da0`: 0x44 Cantplace, 0x46 Outofbounds, 0x47 Tooclosetomine, 0x48
  Tooclosetohall, 0x49 Buildingblocked) with InterfaceError (but for a Night Elf
  builder, race 4) and `CantPlaceSound` (CantPlaceHuman, ...) for 0x44 and 0x47.  A place
  taken ends the mode with `PlaceBuilding` for the race (`0x6fac2ecc`).  (Provisional:
  the port refuses with Cantplace alone.)
- A unit's tip names another player's unit's owner (`FUN_6f3d16c0`, owner below 12 and
  not the local player) in the color of the local player's standing toward it
  (`FUN_6f376e60` → `FUN_6f340400`): the owner sharing control with the local player
  (alliance type 6, the record's +0x98 = 0x38 + 16 × 6, as GetPlayerAlliance `0x6f2016a0`
  reads it) in `UI\MiscData.txt [SelectionCircle] ColorFriend`, one the local player is
  not passive toward in `ColorEnemy`, others and all to an observer (the player's
  +0xf0, provisional) in `ColorNeutral` (each A,R,G,B; loaded by `FUN_6f3535d0`).  The
  original's Mafarazzo TD capture names a creep's owner "Enemy" in red.
- Health bars, unit tips, and text tags are placed in the interface's own space, laid
  out at 4:3 and stretched across a wider screen (as the pointer maps to it); the
  original's capture at 16:9 has a creep's tip and bar over its head.
- Script dialog and game menu buttons (`EscMenuButtonTemplate`) light under the pointer
  with their `ControlMouseOverHighlight`, `EscMenuButtonMouseOverHighlight` decorated
  through war3skins (`DecorateFileNames`), added; the original's defeat dialog capture
  shows it on Quit Campaign.
- Command tooltips (CToolTipWar3, `FUN_6f3a1ef0`): the backdrop blends (its +0x30 set,
  as BackdropBlendAll; so does CUnitTip's), ToolTipBackground (Human: 24,34,49 at alpha
  195) showing the world through it (measured: the original's tooltips read 23,49,42
  over grass and its unit tips 56,64,77 over the road); the middle text (+0x12c) is
  0xff919090 gray.  A button whose requirements are unmet gives its title, the
  requirements, and its text, with no cost line (captured from the original: an unmet
  Cutter Tower, Ice Tower, HSBC, Lightning Tower, and Upgrade to Cannon Tower; a met
  Poison or Arrow Tower shows its gold; provisional: where the costs are dropped is not
  read).  The requirements read REQUIRESTOOLTIP ("|Cffffff00Requires:", unclosed) then
  "\n - %s" per type (`FUN_6f343230`): a color code holds past line breaks until |r, so
  the " - <type>" lines are yellow, and a line keeps its leading space.
- A button lit as the current order's: CAbilityMove::vf112 (`FUN_6f5fee80`) adds Stop
  lit while its unit has no order and does not hold its place; CAbilityAttack::vf112
  (`FUN_6f49c550`) adds Stop unlit, Attack lit by its ability's flags 1 without 0x1000,
  Attack Ground by both (provisional: the flags' meaning).  So an idle tower lights no
  button (captured from the original's idle Arrow Tower) and an idle peasant lights Stop.
- A game with two human players (Host and Jack, captured) has bj_isSinglePlayer false:
  CustomDefeatDialogBJ's dialog holds Quit Campaign alone, the game is not paused, and
  each defeated player is told "%s was defeated." (PLAYER_DEFEATED); Mafarazzo TD's
  lives go on below 0 under the dialog.  The port plays it so (playtest W3LOBBY="R1,L1").

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
- The menu background is a `CSpriteFrame` over the whole glue frame (`FUN_6f0f6980`,
  set up at `0x6f2e09ad`); with a model camera its projection is `FUN_6f1adf40`, which
  calls `FUN_6f126e30(fov, aspect, near, far)` with the frame's aspect.  That shares the
  angle, not its tangent, along the diagonal: the vertical half angle is
  `fov / 2 / sqrt(1 + aspect²)` (3/5 of half the field of view at 4:3), and x is scaled
  by `1 / aspect` of y.  MainMenu3D_exp's Camera02good has a field of view of 1.04656,
  so 35.98° vertically (the tangent's share would give 38.18° and show the water's edge
  at the bottom).  Fitted against captures: at 1592 × 1195 the scene matches 4:3 exactly;
  in a 1272 × 686 window on a 1280 × 720 display it keeps that vertical extent and spans
  exactly 16:9 across, so the scene is not stretched with the glue but sees more to the
  sides (16:9 is likely the size it renders at, scaled into the window; where x is
  widened was not traced).
- The main menu's version (`0x6f2dc0c4`): "%d.%d.%d.%d" of the constants 1, 27, 1, 7085
  in a `CTextFrame` made by code (`FUN_6f0fd2c0`), right justified (`FUN_6f0fdd20(2)`),
  its BOTTOMRIGHT on the menu's at (−0.013, 0) (`0x6f2dc144`).  Captured, its digits'
  baseline is 0.0025 above the screen's bottom and they are 0.0105 tall (FRIZQT about
  0.014 high, set tighter than the font's advances); its right edge 0.013 in.
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
- Leaves: the router keeps LEAVEGAME's word at the player's +0x44 and answers LEFTGAME
  (0x1B, empty: `FUN_6f99cdc0`, `FUN_6f994500`), which the leaving player waits up to
  5 s for (`Net::NetClient::vf7_6f986990`); then (`FUN_6f99dbb0`) STOP_LAG if the game
  waited on them, and PLAYERLEFT with their id and that word to the others.  Each net
  client makes PLAYERLEFT a message of kind 0x17 (`FUN_6f98c680`, `FUN_6f98b9d0`,
  `FUN_6f97a290`: reason 1, the id, the word, its own +0xc), which the keepalive's net
  module hashes as the kind plus the byte hashes of its first three fields
  (`FUN_6f328550`), and the game takes as CNetEventPlayerLeave (`FUN_6f32daf0`: a save
  under way counts the player done, `FUN_6f32c7b0` with 0).  The net messages' kinds
  are the replay's record ids (`FUN_6f327040` records 0x17, 0x1E, 0x1F, 0x20, 0x21,
  0x23, and 0x2F as they come); of those hashed, 0x21 is never posted and 0x2F only
  from W3GS 0x50, under the client's flag 0x100000 (Battle.net).  The word a player
  leaves with is `FUN_6f32c130`'s argument: 7 from the game menu's quits, 11 once its
  game is over (measured after a defeat).
- Chat goes only to the recipients the sender lists (`FUN_6f36ee10`, `FUN_6f32ca60`):
  for all, every other person; for allies, the players playing (slot state +0x270 1)
  whom the speaker is allied with by IsPlayerAlly's alliance; for observers, the
  observers (IsPlayerObserver's +0xf0); for a player (3 + its number), that one;
  computers are left out.  The speaker's own line is recorded when it is not among them.
- The router's lag screen (`FUN_6f997a30`, `FUN_6f99a170`, `FUN_6f999a60`, read): with
  no one waited on, those behind the last turn who have been silent 4 s are (START_LAG,
  each with the time it has been waited on, +0x88, at most 45 s, less 45 ms for each
  290 ms past 10 s since its last wait); one caught up gets STOP_LAG with its wait added.
  DROPREQ (0x29, empty) votes; once more than half of those not waited on have voted,
  those whose 45 s less their time waited have passed are dropped, and after 60 s of
  waiting all are.
- A desync (`FUN_6f999b50`, as each keepalive comes): the players grouped by their
  keepalives for the turn; each told by 0x0E (the turn, its group's keepalive data, and
  the others of its group, itself first in the largest group); the others' connections
  close after 2 s (`FUN_6f998880`).
- Turns: the LAN provider's turn is 100 ms (`FUN_6f974710`; Battle.net's 250,
  `FUN_6f970fa0`), each turn as long as the tick count since the last, at least 1 and
  at most four turns' (`FUN_6f99e310`).
- Discovery: a host answers SEARCHGAME with GAMEINFO (`FUN_6f974ed0`), for the game its
  third word names by host counter, or all for 0, and otherwise says REFRESHGAME (host
  counter, people, slots for people: `FUN_6f9742f0`); a browser answers CREATEGAME, and
  REFRESHGAME from a game it does not know, with a search for that game (`FUN_6f974e40`,
  `FUN_6f975020`).
- The player's 0x13 closes the connection with reason 0x0E, 0x1C with its word, and
  0x1B (the answer to its LEAVEGAME) closes it.  Its 0x14 and 0x16 to 0x1A are its
  states 9 and 10 (a host's handoff, as its strings name it), 0x3E, 0x40, 0x44, and 0x45
  a map from another player, and 0x47, 0x50, and 0x51 only under flag 0x100000.
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
  the old type and starts or adds the new; a unit killed adds one killed to its
  killer's owner's registration of its type, made last if there is none, unless that
  owner is passive toward the dead unit's (its death's 0xd0153, `FUN_6f69c210`: the
  killer at +0x250, `FUN_6f68bd60`, which reads the killer's owner (+0x58) alliance
  mask of type 0 at the dead unit's owner's bit, as GetPlayerAlliance and IsUnitAlly
  do, then `FUN_6f266fb0` → `FUN_6f23b060`, +0x2c; a player is passive toward itself,
  so its own kills count none).  Measured: the flyer game's keepalive at turn 275,
  after its first kill, wants the game module of the Arrow Tower's owner's tree with
  a registration of the creep's type (0x7fffffff, killed 1, flags 8) after its
  tower's; the port, which had none, kept the keepalives only to turn 274.  At
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
- A walk's end, a wanderer's give-up or arrival or a worker's arrival at a structure,
  frees its path in the next step at the place its walk would take among that step's
  walkers, newest walk first, or after them all when older (read with step marks,
  `trstep.sh`: turn 890 of the game without actions, a give-up's path freed before an
  older walker's give-up made its three objects; turn 1219, an arrival's after a newer
  one's; a peon's and a wanderer's paths, ended in the same step, newest walk first,
  each way round; a give-up's carried from a tick's last step freed before an older
  peon's hall arrival in the next tick's first step, after a newer one's).  A later
  step's end then frees the objects of the orders given at its tick, then its walk
  ends' objects (read: turn 1343, an order's eight after its step's give-up made its
  three and before that give-up's tree; the mining game's turn 932, after a peon's
  path).  The walks' order is their serials': a wanderer's and a gold run's each
  taken before its order's objects, so a walk begun later is newer (read: a
  wanderer's path, its walk begun at the tick a worker left its mine and ended
  first, freed before the worker's hall arrival).  With that the game without actions
  agrees with the original's trace to its end (turn 1361) and its keepalives for all
  1615 turns recorded.  The tags example prints each
  turn's steps with their main ticks (TAGS_STEPS), and its tag trace marks each step
  (`S k`).
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
    the provider's answer 3); within reach the walker stops and turns toward the
    structure's bearing (a turn within its rate sets it exactly, read across 0:
    5.990847), and arrives the next step, its heading within a step's turn of the
    bearing, or at once when it comes within reach on it (read: a peon stopped at a
    mine 0.069 short of its bearing, its turn 0.6, went in at the next step with its
    heading as it was; one walking straight at its hall arrived as it came within
    reach).  The range tests
    after (`FUN_6f05b580`: range over 32 plus both position objects' +0x90 radii,
    0.5, 1.5625 for the mine, 5.5 for the hall, the places extrapolated) fail at both
    structures; their contact test (`FUN_6f058cb0`, the ring touching the footprint)
    passes.
  - The first walk, from the player's order, searches as other walks do (its path's
    mask +0x9c 0x02000002; the idle peons' regions, +0x34 0x010000ca, refuse cells).
    After the worker's first arrival its harvest's flag 0x40000000 is set and its
    orders carry task 0xd0150 (ghosting, `FUN_6f422890`, `FUN_6f699220`): its region
    loses 0xd6 and its mask 0xc6 and gains 0x10 (the walks' mask 0x10000010).  The
    walk to the mine always ends with task 0xd0151 (`FUN_6f43b7e0`, `FUN_6f699260`),
    which sets 0x10 in both; the walk home has none.  So a ghosting search, line test
    (`World::way_ahead`), and hold pass through all units but workers stopped at a
    mine, and other walks pass through workers that have ghosted on their run
    (`Unit::region_flags`, `Unit::region_mask`; read: the walk home along row 265 past
    the idle peon whose region covers 672, 264, never held and drawing nothing;
    workers waiting at a mine after their first walk turned another's first walk
    aside, and one waiting after an arrival a ghosting way off row 263; a peon just
    out of a mine, turning to walk home, let one walking into its cell on).  As the
    walk to the mine ends (0xd0151), its owner's units within 0.1 of touching it whose
    mask holds 0x10 (`FUN_6f427510`; workers that have been in a mine) are each placed
    again where they stand, clear of the units their masks meet, still walking (read:
    a peon walking home through the cell where another stopped moved to the next cell
    clear of that one and the one waiting beside it, 673.5, 263.5).
  - The walk's end is taken in the walkers' order (newest walk first) in its step:
    the objects an arrival makes are made then and those it frees wait for the step's
    end, in that order with the others' walk ends' (read: a hall arrival's freed
    order before a newer wanderer's walk end's tree, after an older one's).  Its path
    goes in the next step as any walk's (above).
  - At the mine (`FUN_6f437ae0`, in the step the walk ends): the mine's first worker
    in makes a `+fgm` (one for five peons); the harvest order's move task and the
    task before it go as the step ends.  It goes in for the mine's Agld DataB (1
    second) on the agent queue when the mine has room (Agld DataC, one: its slots
    +0xec, their count +0x90 under +0x8c, `FUN_6f5c74c0`), and else waits where it
    stands, a `uref` its place at the end of the mine's queue (+0x94 to +0xa0,
    `FUN_6f5c75e0`).  Its speed is tuned either way.  When the occupant's time is up
    (the mine's timer, event 0xd01c1, `FUN_6f5eee60`): the occupant is shown again
    where it went in, or at the nearest cell clear of the units its mask meets
    (`FUN_6f699c00`'s placing, without the size class's room the made units ask for:
    read, the first out at the next cell but one as another stood where it went in,
    later ones by the mine's footprint staying); then the queue's first goes in, its
    time from the occupant's due (`FUN_6f5eed80`; read: the agent clock at each
    let-in a whole second past the first entry's), its `uref` freed with the tick's
    orders' objects; then the occupant takes its gold (`FUN_6f4382c0`): with workers
    still queued the mine's FloatModifier (`FUN_6f5e64b0`, `FUN_6f058120`, a
    `` `rcm ``) is made, or its end put off, to a second after that step's clock, and
    is freed ten seconds after its end (read: made at turn 602, put off at 612 and
    622, freed at 732); then the order home.  Its walk home's first step is the first
    step after, and its harvest order's tree goes with the order home's making.
  - A player's order to more than one unit has two cluster groups of its own while
    its units take it, and they take it nearest the target first (read: five peons
    sent to a gold mine, two `^clg` made before their five orders and freed after;
    the orders made by the peons' distances to the mine, 8.0 to 12.0 cells).
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
    tree, then the new order's five, go as the step ends.  At the mine's timer the
    harvest order's tree and the order home's five go with the objects of the orders
    given at that tick (read: after a wanderer's order given then, before its eight).
  - A unit trained is made before the tick's first movement step (read with step
    marks: a peon trained made as its turn began, a worker's arrival and a
    wanderer's walk end in that turn's last step freeing theirs after).
  - The orders' objects: a player's harvest order 19 (`ordt`, two `tskA`, four
    tasks, three `tskt`, a task, a `tskt`, three tasks, `tskA`, `^clg`, `^clu`,
    `l^pa`), five of them freed as the next turn starts; the worker's own from the
    hall 19 (one `tskA` at its head, four tasks at its tail); the order home 16.
  With this every recorded game agrees with the original's trace and keepalives to
  its end: the mining game (`docs/reference/bootybay_mining_*`, 1615 turns);
  step-marked games of mining and a training (`docs/reference/bootybay_mining_training_*`,
  1234, and one with selections and a control group too, 1400); a game trained
  before and during mining (1303); the game of selections, a training, and mining
  (1056); five peons sent to the mine together (`docs/reference/bootybay_mining5_*`,
  1141), their steps (read in three games) bit for bit.
- Lumber (traced: a Booty Bay peon sent by a smart order to a tree, its tags, steps,
  and the harvest's handlers read under winedbg in two games; `crate::goldrun`, a gold
  run whose `lumber` is set) runs as gold does with the tree for the mine:
  - The orders lack the gold orders' move to the mine and the task before it: a
    player's 17 (`ordt`, two `tskA`, four tasks, three `tskt`, three tasks, `tskA`,
    `^clg`, `^clu`, `l^pa`; its `^clg` at once, 1, 13, 12, 11, and 10 as the next turn
    starts), the worker's own from the hall 17 (one `tskA` at its head, four tasks
    at its tail).  The order home is the gold one's 16.  Their three `tskt` are the
    order's tasks 0xd0185, 0xd0184, and 0xd0171 (`FUN_6f419530` pushing them by
    `FUN_6f6926b0`); the last, the move, is the walk.
  - The walk is searched to the tree's footprint (a Booty Bay tree's 4 by 4 cells) as
    one to a mine, but it ends in range: as a step finds the walker within Ahar's Rng
    over 32 plus its radius of the tree's middle (the tree's radius none: read, 4.134
    cells walking on, 3.972 stopped, Rng 116), its course within 0.2 of the tree's
    bearing, its move is done there in that step (event 0xd0184, `FUN_6f4377d0`; read:
    a peon 0.102 off, its heading kept); else it stops there and turns toward the
    tree (onto its bearing when the turn is within its rate), its move done the next
    step (read: one 0.424 off).
  - Within range but not yet free to chop (the harvest's 0x1000), it walks to reach
    the tree, its tasks counted down from 3: the move to it in range (`tskt`, 0xd0171),
    the end of its ghosting (`task`, 0xd0151, `FUN_6f43b7e0`, which sets 0x1000), a
    block (`tskA`, `FUN_6f6927a0(5)`), and that walk's `^clg` (at once), `^clu`, and
    `l^pa`.  The move's target task, then the new block and task, go as the step
    ends, the move's path in the next step.  That walk is done in the next step, its
    target task going as that step ends and its path in the step after; 0xd0184 again
    finds it free and it starts to chop (`FUN_6f436400`, the harvest's 0x40000000 set:
    its later orders ghost).  Read: turns 853 and 854, 990.
  - Chopping is three events on the harvest's agent timer (+0x1dc, `FUN_6f0608d0`): a
    swing (`FUN_6f436a90`) chops at its damage point (+0x1cc, event 0xd01b1,
    `FUN_6f405110`: Ahar DataA lumber, as much of the tree's life), its backswing
    (+0x1d4, at least 0.01) runs out (0xd01b2, `FUN_6f40bce0`), and the next swing
    starts after Dur less both (at least 0.01; 0xd01be, `FUN_6f40c4a0`).  Full (DataB),
    it stops as the last backswing runs out (`FUN_6f436ca0`) and is given its order
    home at once, walking from the next step.  A peon's 0.5, 0.5, and 1.1 make ten
    chops 10.899918 seconds on the agent clock (read on three trips, from 212.181900 to
    223.081818).
  - At the hall the lumber is given and its own order made as with gold, the order
    home's tree and the new order's five going as the step ends.
  - A chop takes as much of the tree's life as the lumber it gives; the chop that
    takes the last fells it (a Booty Bay tree at its fiftieth): its pathing lifted
    and the last three of its five `^pmr` freed at that chop (turn 2775), the worker
    stopping as that chop's backswing ends (its target gone, `FUN_6f40bce0`), full
    or not.  At the hall a felled tree's worker is ordered to the live tree cheapest
    to reach from the tree its order named (the felled one, the first time) within
    Ahar's Area (`FUN_6f416740`, `FUN_6f4153a0`,
    `FUN_6f439e00`): the trees about it are enumerated by 256-unit blocks from the
    map's corner, row by row, each block's latest made first (`FUN_6f651170`), each
    judged by a path search from the felled tree to it (`FUN_6f059590`, under a limit
    of 32 that falls as better ones are found, 0xffffffff past it), and the first of
    the cheapest is taken (read under winedbg: every candidate's cost in order, the
    three about the felled tree 4 each and the first of them taken; from the next
    felled, the one of its three in the earlier block).  Its order and walk are any
    trip's.  The port judges by the steps of the local search's way to the tree's
    footprint (equal for the trees about the felled, as the engine's), and does not
    yet run a tree's death events; a tree felled short of a full load is not yet
    read.
  The search is about the tree the worker's order named, not the last felled
  (`FUN_6f416740` tries Ahar's +0x174, then +0x180, each kept by an order,
  `FUN_6f418660`, the second only while the worker carries nothing; the search's own
  pick is ordered without them): read by tracing the original playing the lumber long
  game back (`trchop.sh`, `treeqr.sh`): five trees felled, each by five full loads of
  ten chops, as the port's; the sixth, 0x1f72, the cheapest from the first (9024,
  2368: 9216, 2496 at cost 7), where the fifth's cheapest was 8896, 2624 at cost 1;
  and the search at the next drop again about the first, its candidates the live
  trees about it.  (An earlier note had the fifth felled short of a full load: it
  was not.)
  The lumber game (`docs/reference/bootybay_lumber_*`) agrees with the original's trace
  and keepalives to its end, 1422 turns, and a longer one
  (`docs/reference/bootybay_lumber_long_*`: three trees, two felled, the clocks set
  back at 300 seconds, dusk, night, and midnight) for all 4120 turns of its trace, and
  with its keepalives for all 7300 turns (six trees).
- A player's order to walk to a point (traced: a Booty Bay peon sent across the
  ground, `docs/reference/bootybay_moves_*`) is the engine's walk, as a wanderer's
  with its way searched 700 nodes deep (`World::start_player_walk`).  Its order has
  the wanderer's objects but one task fewer, 17 (`ordt`, two `tskA`, `tskO`, three
  tasks, `tsk.`, five tasks, `tskA`, `^clg`, `^clu`, `l^pa`): its `^clg` freed at once,
  its two blocks and five tasks as the next turn starts (as a player's harvest
  order's), and the rest as a wanderer's arrival frees them (read: its tree but those
  freed at its goal, its path the next step).  The game agrees with the original to
  turn 1702.  A second such order while the first's walk goes on frees the first's
  rest and path as the next turn starts too: the new order's first block, the old
  order's 7 to 2 and 0, the new order's other six, the old path (read: two orders
  to walk given to a peon in one turn, turn 604).
- A player's order to several units to walk to a point makes the group's two `^clg`,
  each unit's order without a cluster or path (14 objects, its blocks and five tasks
  freed as the next turn starts), then one `^clu` and `l^pa` for them all, the `^clg`
  freed after (traced: five peons, `crate::groupwalk`).  They walk as one cluster
  (CLrCluster: members of 0x2c bytes at +0x28, up to 12; `FUN_6f16bcf0`,
  `FUN_6f16de50`), each straight to its place, its cluster's point (+0x54) plus its
  own offset (+0xc; `FUN_6f16e250`), and each ends its walk there as an arrival, its
  tree but the issue's freed and no path; the cluster's path the step after the last
  (read: turns 1719 to 1732).  The places (`FUN_6f16a5b0`): the members' places
  turned to the formation's bearing (the line from the first member, the nearest the
  point, to it: +0x70) and sorted by rank (the steering object's +0xd8 bits 12 to
  15) and, within a rank, front to back; each rank in rows of the table's width by
  its count (`DAT_6fa91d20`: 1, 2, 3, 2, 3, 3, 3, 3, 3, 4, 4, 4 for small units; with
  more ranks `DAT_6fa91d88`), each row's units, by side, spread from half the
  diameter across the widest row's width (the diameter, twice the largest radius
  +0x90, times the row's count, plus the diameter and 2.0 times one fewer), rows
  stepped back by the diameter and 2.5, ranks by 5.5000005 (the gaps read in the
  running game, `DAT_6fd541c0`, `541c4`, `541bc`); the whole centered on its middle
  and turned back (`FUN_6f169e10`).  Read: five peons' targets, bit for bit to six
  places.  As the cluster's walk begins (its first member's first step, the turn
  after the order) the main generator is drawn once (read: the keepalives agree only
  so; the players' parts of the game module, read under winedbg, did not change).
  The moves game agrees with the original's trace to turn 1726 and its keepalives to
  1725.  Open: the members' starts.  Read in two games: the two ahead start at the
  first step (the nearest swerving 0.03 at it, likely the separation of
  `FUN_6f1702f0`: neighbours within a radius by class, `DAT_6fd54398`, push by
  (1 - d/r) squared), the two behind them ten steps later, and the last, behind one of
  those, six after that; each waits with its heading on its place, its member flags
  (+0x28) 0x100000 throughout.  Each member has a path provider of its own (steering
  +0xa8; read: five of them, their hold counts, +0x94, 0 throughout) with a way of
  its own to its place (+0x70: 2 nodes for the two ahead, 3 and 4 behind); a waiting
  member's way index (+0x74) is -1 until its search succeeds, so the waits are the
  searches' (their budget and the ten steps a refused or blocked one waits,
  `World::path_to_inner`), not holds.  The port's rules for wanderers' searches, given
  the members, start them at steps 4, 3, 3, 1, and 2 where the original's were 17,
  11, 11, 1, and 1; the port so far walks members straight to their places, with one
  draw from the main generator as the walk begins standing in for the searches'
  (which keep the keepalives agreeing to 1725).
- A script's point order (IssuePointOrder and kin) walks as a player's: the same
  objects and the same walk.  Measured with Mafarazzo TD's script (`abtest/mafp.w3x`:
  a timer pinging each creep's GetUnitX, GetUnitY, and GetUnitMoveSpeed at 21, 23, 25,
  and 27 seconds): its 180 creeps, made at 20.0 and each sent alone 2600 to 3600 units
  down its lane, walk straight for a cell's middle 10 to 16 accelerator units on (the
  first, made at (-3872, 2528), bending for (-3856, 1520)), stop a step there, and go
  on for the next, at exactly their speed on the main clock; they start in five
  groups, 2 steps apart, the newest first.  The engine's way (read from game.dll):
  - Every path plans over the accelerator first (+0x88's 0x200000, set by
    `NIpse::CLrPath::vf3_6f166060`, its limits +0x84 0x19002bc: 700 local pops, 400
    accelerator pops), but Neutral Passive's wanderers' (`FUN_6f5a6d40`:
    `FUN_6f0594a0(this, 0)` and a local limit of 32).  A move's cluster (`^clu`) has
    a path of its own (`l^pa`, `FUN_6f169840`: flagged 0x400000, its accelerator limit
    5000, `FUN_6f1678f0`).  The cluster's step (`FUN_6f16c150`) plans it once
    (`FUN_6f16ce10`, `FUN_6f167120`, `FUN_6f166c30`, `FUN_6f162cb0`) from its member's
    place to the order's point, and picks its point (`FUN_6f1697a0`, `FUN_6f165e30`,
    `FUN_6f167ae0`): from the point below its index toward the goal, the first where
    the way's legs from the index, summed (each the engine's root of its squares),
    reach 10 (`DAT_6fd54194`); none before the goal, the goal.  That point times 2
    (cells, `FUN_6f167d70`) is the member's target.  Refused its plan, the cluster's
    members do not step.
  - The member's own path (the unit's `l^pa`, limit 400) plans from its place to that
    target in its provider (`FUN_6f165ae0`, `FUN_6f165b60`) and picks its own point the
    same way, its local search going for that (`FUN_6f165c60`, `FUN_6f167ce0`,
    `FUN_6f166e90`); within 0.49 accelerator units of it (`FUN_6f165f10`) it picks
    again and searches at once.  For the creeps it is always the target itself.
  - Within 0.49 cells of the member's target, its heading within 0.2 of it, the
    member is done and stops (`FUN_6f16fbd0`, `FUN_6f16e910`); as the next step begins
    the cluster, its members done (`FUN_6f16c4f0`, `FUN_6f16b120`), picks its next
    point and drops its members' ways (`FUN_6f16d6a0`), which plan and search anew:
    the creeps' step lost at each point, stopped within 0.49 of it (one at (-3792,
    1531.000) as it was pinged).  At the goal the walk ends.
  - Each plan and search is spaced (`FUN_6f168910`: the path's +0x7c for its local
    searches, +0x80 for its plans, ten steps, `DAT_6fa91c14`, after one that left its
    budget at 64 pops or more, or for a plan, took 32 or more) and budgeted per owner
    (`crate::budget`, below).  The creeps' searches, 21 to 35 pops each, fill the
    budget of 1100 by the 41st: the rest are queued and served in turn as it is
    renewed each other step, 47, 37, 42, and 13 of them; the 37 all those whose
    clusters had planned by then, the clusters' plans (some 12 pops each) filling their
    budget of 800 by the 66th at the first step and the 59th at the fifth, renewed
    every fourth step (in the port's count, steps 667, 669, and 673: so renewed at
    steps 1 + 4k, as the local budget at odd steps).
  `World::cluster_goal` and `World::walk_steps` so walk a unit sent alone to a point
  by a player or a script; each of the 720 pings is the original's to 0.001.  The
  port's AI walks keep their own (`World::coarse_goal`: the straight distance, not
  the legs, at 10; not budgeted), but a unit's attack-move back to its guard post,
  which walks so (below); a group's members walk straight to their places.
- The search budgets (NIpse's): for each player (a path's class, +0x88 bits 16 to 19,
  the owner as `CUnit::vf107` and `FUN_6f698ce0` give it to `FUN_6f168a80`; Neutral
  Passive's wanderers' class 15), four records of 0x1c bytes at `DAT_6fd53a90` + player
  * 0x70, made by the static initializers at 0x6f003d20 (`FUN_6f1656d0`): +0 a pop
  limit, +2 a period, +4 a cap, +8 the use, +0xc a countdown, +0x10 a queue's count,
  +0x14 and +0x18 its head and tail (each path's +0x8c and +0x90 linking it).  The
  four: 5000, 3, 800 (a plan by a path whose accelerator limit passes 400 and not
  flagged 0x4000000: a cluster's); 2000, 2, 300 (one so flagged); 400, 2, 900 (one of
  400 or under: a unit's own); 700, 1, 1100 (local searches, `DAT_6fd53ae4`).  Each
  step (`FUN_6f167310` from `FUN_6f15aa80`) a record's countdown runs down, and at 0
  its use is cleared and the countdown set to its period (`FUN_6f167fa0`), so it is
  renewed every period + 1 steps.  A search asks first (`FUN_6f168310`): over the
  cap it is refused and queued (`FUN_6f165ea0`); under it, allowed while none waits,
  else only the queue's head is (taken off, `FUN_6f1686a0`), others queued behind
  it.  Its pops are added after: a local search's the system's +0x6c, a plan's +0x9c
  (cleared by `FUN_6f164230`, counting one more for the try a limit stops).
  Provisional: the plans' records taken as renewed with the local one, at odd steps.
- GetUnitX, GetUnitY, and GetUnitLoc read a walker's place at its last step carried
  on by its velocity to the main clock then (above): the creeps pinged by a timer at
  21 seconds, as the turn's timers go off before its first step, are where their
  velocities take them by 20.9975 (`World::place_now`; the port reads so only while
  a turn's timers go off, the main clock then its first tick's).
- A region's rect (RegionAddRect, `FUN_6f05fcf0`) is taken in cells from the map's
  corner, its corners over 32 (0xfd800000 added to their bits), each floored, the
  ends one past the greatest's floor (`FUN_6f05ee40`): a rect ending on a cell's
  edge takes the cell past it.  Read: Mafarazzo TD's creeps entering WalkMid3 (to y
  1856) at 1888, the original's turn 304, the port's two turns later before.  Its
  cells are a pathing map region's (`FUN_6f14d960`).
- A walk ordered while one goes on, between steps (a region's trigger, below), frees
  the old order's path (`l^pa`, `^clu`) at the next step, not with its tree: read, a
  creep entering WalkMid3 at turn 304's last step, its next order's objects and the
  old tree freed in that turn and the old path in turn 305 (another in 306 and 307);
  one entering WalkMid4 at turn 308's second step, its old path freed in that turn
  after the trigger's data, at the step after.  The old cluster, its member gone to
  the new order's, is freed as the next step reaches it (`FUN_6f16c150`:
  `FUN_6f16d1c0` finds no member, the cluster's vf 0x10 `FUN_6f1699c0` frees its
  `l^pa`, then itself, `CPresenceTagged::vf4_6f1c5490`).
- A region's enter and leave events come within the movement steps.  Each cluster's
  step (`FUN_6f16c150`) first takes each member's place at the step's clock (its place
  carried on by its velocity, `FUN_6f161040`), the members from the last, to its cell
  (`FUN_6f16bc10`, CPoPosCl's vf 0x54 `FUN_6f16fa00`, above), before it plans or moves
  them; separation's pushes (`FUN_6f1702f0`, `FUN_6f16ffa0`) and placings
  (`FUN_6f170080`) do the same through `FUN_6f05c820`.  `cp&l` and `cp&e` reach the
  region's agent (`CAgentBaseAbs::vf8_6f057590`, passing them to its vf 0x18,
  `CTriggerRegion::vf6_6f05fe00`), which dispatches 0x401900c8 or 0x401900c9 to its
  registrations at once (`FUN_6f05f7c0`, `FUN_6f05f810`; vf 0x14 `FUN_6f071dc0`,
  `FUN_6f071e00`).  A registration (`CRegionEventReg::vf3_6f27cf20`, `FUN_6f277bd0`)
  asks its filter then (+0x38's vf 0x78, the unit as the filter's), queues the event
  (an `AURegionEvent` of the unit and enter or leave, its list at +0x54), and starts
  its timer (+0x3c, `FUN_6f0608d0`) with no wait, event 0x80275 to itself, unless it is
  already waiting.  A timer's wait under 0.0001 is taken as 0.0001
  (`CAgentTimer::vf2_6f060190`, `DAT_6fcd539c`), so it falls due that much after the
  step's agent clock; firing, the registration runs its queue in order
  (`FUN_6f2777c0`: event 0x80205 to enter, 0x80206 to leave, each unit still there)
  and empties it (`FUN_6f2764f0`).  Measured: Mafarazzo TD's 81 walk orders through
  34.01 seconds (IssuePointOrderLoc, its clock and step count read in a LAN run) at
  the clocks and in the order of the port's region entries so found.  Where that
  falls with the step's tick's other events (traced through the keepalives' next free
  slot, the turn's last free, with the objects made the same):
  - In the step's own main tick, the triggers' orders make their objects before the
    tick's frees (the step's walk ends' among them), as a wanderer's order due after
    the step does (above), and free theirs after: read, a creep arriving at WalkMid12's
    middle in the step it entered it (turn 349's second step, its trigger due in that
    tick), the new order's objects in the slots the old walk's tree then freed.
    What a timer let go in that tick before the triggers ran is among those frees,
    after the orders' makings and before their own frees (`World::after_step_events`).
    Traced over LAN (maftr, turns 323 to 346): the peasant's build order's seven
    objects, their release due 32.395412 as turn 325 began (the actions' clock), its
    first step in that tick and a creep's region trigger due 32.399323 in it: the new
    order's objects made, the seven freed, then the old order's; the port had held the
    seven for the next step's tick.
  - In a later tick (a step within 0.0001 of its tick's end: in this game the steps
    from 34.56 to 34.71 seconds, as the main clock trails, before the steps move to
    the tick after), after them: read, two creeps arriving at WalkMid12's
    and WalkMid34's middles in the step they entered them (turn 346's last step), the
    first order's `^clg` in the 17th slot past the two old trees' (the turn's last
    free the second order's `+tex`, in that slot).
  - Not yet seen: one due after the turn's last tick (the port runs it at the next
    turn's first, before its timers).
  An arrival in a step ends its order then (`FUN_6f16c390`, `FUN_6f16d4e0`,
  `FUN_6f171340`: `cp&a` to the unit's agent, 0x40190065 dispatched, the move
  ability's 0xd0196, `CAbilityMove::vf3_6f5fda10`, `FUN_6f5fa7a0`), so a trigger then
  gives its order to a unit without one.  The port so tells its engine walkers' region
  changes in their steps (`World::step_regions`, `World::run_step_region_events`);
  Mafarazzo TD over LAN agrees to turn 358 (from 308).
- A member's own path keeps its ways while the point it steers for stays in its goal's
  accelerator cell: the steering (`FUN_6f16fbd0`, each member's step from
  `FUN_6f16a790`) resets the path to a new point (`FUN_6f168bd0`, `FUN_6f168be0`,
  `FUN_6f168b80`, `FUN_6f168740`: ways dropped, to be searched) only when the cluster's
  key (`FUN_6f16afc0`, none for a point order) differs from the path's +0xa4, or when
  `FUN_6f167e40` finds the point's cell (floored and halved) not the path's goal's
  (+0x1c, +0x20) and both its spacings (+0x7c, +0x80) more than 9 steps old.  So a walk
  ordered anew to the same point searches no more.  Read: Mafarazzo TD's creeps on
  WalkMid3's and WalkMid1's edges, walking for WalkMid34's and WalkMid12's middles at y
  1888 (WalkMid3's edge; y about 251.0 cells, the heading swinging 0.0044 either side),
  leave and enter the region every few steps, each entry ordering them to the same
  point; the port's searching anew, its last search spaced, stood seven steps and gave
  one order fewer by turn 358.  The port keeps the way for a walk ordered to its own
  path's goal's cell while its cluster went for its goal (`World::start_player_walk`;
  provisional: searched anew from where it is, without budget or spacing), and the LAN
  game agrees to turn 396.  Not yet in the port: a new point's cell within the
  spacings, the old way kept until they pass.
- The movement step (`FUN_6f15aa80`) goes through the clusters from the head of their
  list (+0x3b8, each by its +8), and a new cluster is linked at the head
  (`FUN_6f16c880` on the list at +0x3b0, from the cluster's making), so the newest
  steps first; an old cluster its member has left is freed as the walk reaches it
  (`FUN_6f16c150`, above).  So the old paths waiting for a step go newest walk first
  whatever the order their walks were replaced in (`Tags::free_step_paths`).  Read:
  Mafarazzo TD's two creeps given new orders by their regions' triggers at turn 395's
  last step (a fresh LAN capture, the original's objects traced from turn 391), the
  first's old walk made at turn 345 and the second's at 346: the second's `l^pa` and
  `^clu` were freed first at turn 396's first step, the port's in the orders' order
  before (its next free slot at turn 397 the other walk's `l^pa`).  The LAN game so
  agrees to turn 461 (from 396), the old capture likewise, and the upgrade game to 350
  (from 308).
- SetUnitPathing (`jass_SetUnitPathing` at 0x6f215540) sets the unit's pathing types
  through its vf15c: off, its region's types (`FUN_6f05ac60`, +0x34 of its region)
  with 0; on, those of vf160 and vf164.  What its walks' searches and plans then mind
  is not yet read: the port's ignore it for the engine's walkers, and the creeps,
  turned off as they set out, walk open lanes, where either reading gives their ways.
- The data modules are the data store's slots (`FUN_6f06c180(0xd)` +0x10; the
  keepalive's `FUN_6f042dd0` folds each slot's vf0 from the last).  A table's slot
  (CWar3SlkDatabase) keeps `rol(hash, 3)` (`FUN_6f042d80`, `FUN_6f042da0`), the hash
  (`FUN_6f1996c0`) the sum over its cells of `b * 0x7a2d ^ b` for each byte: a
  string's characters, an integer's or a real's four bytes.  Cells are kept as text
  and typed as read (`FUN_6f19a070`): empty none, a string unless the first character
  is a digit, '.', or '-', a real if any of ".dDeE" is in it, else `atoi`.  Reals are
  the engine's own parse (`FUN_6f070de0`): up to nine significant digits as an
  integer, `from_int`, then the engine's divide by (or multiply by) ten to a power
  made by squaring with its truncating multiply (`FUN_6f071180`).  The SLK reader
  (`FUN_6f198a20`) takes nothing unless the first line is "ID;PWXL;N;E"; B gives the
  size (at most 96 columns), C and F move the current cell, a C's K stores after a
  quote up to the next, else to the line's end.  Module 4 is the unit tables
  (`FUN_6f688f70`, CUnitDatabase's `FUN_6f6a8110`: UnitData, UnitAbilities,
  UnitWeapons, UnitBalance chained `h = rol(h, 3) ^ next`, rotated again; unitUI not
  hashed), 5 AbilityData, 7 ItemData (hashed in a temporary table first, so rotated
  twice), 9 UpgradeData.  Modules 12 to 19 are the object data's (CCustomData by kind,
  `FUN_6f69fba0`; vf0 `FUN_6f6a80e0`): its kind's meta table's hash (CObjectMetaDB,
  `FUN_6f6dd470`; `FUN_6f6e5150` chains any tables added to it, `h = rol(hash ^ h,
  3)`), and with the map's own object data `rol(that ^ meta, 3)` (`FUN_6f6e50d0`: each
  object, original then custom, `FUN_6f6fb170`: `rol(rol(old, 3) ^ new, 3)` then each
  modification `u = rol(u ^ m, 3)`, a modification `FUN_6f6e8790`: its field and type,
  its level and data column for kinds with levels, then its value: an integer's or
  real's bits, an unreal's `to_int`, a string's hash (`FUN_6f1de250`, the payload
  hash's form) unless the field's stringExt is set; ids the file's characters as a
  little-endian word).  Read with a custom map (Mafarazzo TD: 83 units with 2011
  modifications, an ability with levels): its data tables are the data set's
  (Custom_V1), the object data not applied to them, and every module as the port
  computes it.  12 units and 13 items (both
  UnitMetaData), 14 destructables, 15 doodads (Doodads\DoodadMetaData; not made in a
  game), 16 abilities, 17 buffs, 18 upgrades; 19 was 0.  Read under winedbg in a game
  of Booty Bay played alone: each table's hash and each module as the port computes
  them (`datahash`).
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
  place (+0x78, +0x7c), a velocity (+0x80, +0x84; once its length passes the top
  speed at +0x88 it is scaled to it by the top times the reciprocal of its length's
  square root, `FUN_6f15fc70`, `FUN_6f071570`, which falls a little short: read,
  peons whose tuned speeds were 5.907512 and 6.486394 walking at 5.907436 and
  6.486344, those at 5.9375, 7.125, and 6.025506 at theirs), and the clock when last
  placed (+0x70 float, +0x74 epoch); a change
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
- A mover's heading (+0x8c) is its velocity's as the velocity is set (`FUN_6f1606e0`
  with its flag → CPoPosCl vf20 `FUN_6f1711b0` → `FUN_6f160060`: the heading of
  +0x80/+0x84, `FUN_6f1d4c80`, when the length squared is at least 4e-7,
  DAT_6fcd5474), not the heading the steering turned it to; the steering takes the
  velocity's heading again while it moves, so only a walker at rest turns from the
  stored one.  Measured (Mafarazzo TD's plain map, its creep 0x6a05e48 at
  `FUN_6f16fe20` each step from 41.695 to 42.145 seconds, with the arc cosine's
  arguments in the step at 41.995): each heading the engine's arc cosine of its
  velocity to the last digit, 0.001983 for one due east (its length's root a little
  short), and the turn in place of its new walk at 42.025 from 6.277347, the last
  velocity's heading (its third `FUN_6f1d4c80` there, from 0x6f1600f6), where the
  port's had taken the bearing it steered by, 6.277749, 5.5e-5 over its 60 degree
  window at its second turn, and walked a step late (`motion::Mover`).
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
    8.369596, two steps after its first, 8.309607) -- when its first step's turn leaves
    it within a step's turn of the bearing, as a stopped walker is judged: one that
    starts twice its turn or more off turns and arrives as any walk, its tree freed as
    its turn ends and its path the step after (read: a pig 1.40 off and a sheep 3.0
    off, their turns 0.5, in a game past 400 seconds).  A new walk whose search finds
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
    times as they fire between).  The movement steps are a timer on it every 30
    milliseconds in seconds by the engine's arithmetic (0x3cf5c290, a bit above the
    single nearest 0.03, which 30 times 0.001, 30 over 1000, and 3 over 100 all give;
    read: its clock at all of 2600 steps, 0.03 to 77.997368, the sums of it, where
    the nearest 0.03's sums part from step 24 by an ulp, as the tuned speeds of
    workers timed by it showed), so a step
    is taken at the last main tick whose clock is short of the step's time (strictly:
    read, step 4390, due 131.695175, at the tick before the one whose clock is its
    due; step 2468, due 74.037529, at tick 14813, 74.037521), and moves
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
  - At 300 seconds the clocks are set back by 300: the main clock's first 0.005s
    sum at or past it (tick 60111, 300.004913) reads 0.004913, the ticks after
    counting on from that (0.014913 at the step after 299.985016); the agent queue's
    events then due are set back as much (the step due 300.018311 at 0.018311), and
    those they schedule are counted from the dues set back, so a chain of events
    reads differently after (a chop cycle begun at 295.938477 over at 6.838403, not
    the 6.838409 of its sums set back at the end).  Read in a game past 300 seconds,
    each step's two clocks.  It is each dispatch queue's (`CSiDispatch`) limit, at its
    +0x48 (`DAT_6fd3c7d8`, from_int(300) by the initializer at 0x6f001c40, given as
    each is made, `FUN_6f156bd0` and kin): a queue moved on past it (`FUN_6f054050`,
    `FUN_6f0540f0`, `FUN_6f054190`) runs its requests due by the limit, takes the
    limit from each due left (`FUN_6f052170`, counting the times at +0x44), and goes
    on from the rest.  The port sets its kept clocks back
    as the first step after begins (`World::plan_steps`, `World::wrap_clocks`), its
    step clocks there bit for bit the original's.
  - An event on the agent queue due after a step but in that step's main tick (the
    queue runs to the next tick's clock, events in time order) comes after the
    step's walkers move and before its end: its objects are made then, its `^clg`
    freed, and its other eight freed after the step's walk ends' (read: turn 1748, a
    wanderer's order due 174.503616, after step 174.503433 in the same tick, made
    before an arrival in that step freed its tree; `World::issue_after_step`).
  - The time of day moves on by the main clock, not the game's time: it is a
    FloatModifier (`FUN_6f28bbf0`: from DayHours over DayLength, wrapped at DayHours
    by a listener with event 0x80262), read against the main clock, which falls
    behind (read: the dusk triggers at turn 2004 and midnight's at 3207, where the
    main clock reaches 200 and 320 seconds, not at 2001 and 3201).  A script's
    game-state event is a FloatListener on it (`FUN_6f27a110`: less than and at most
    wait for it to fall past the value, at least and greater than to rise, equal
    either way as it stands); one firing runs its trigger, and an equal one's is
    replaced by one the other way (`FUN_6f27c5c0`), its `` `rtl `` (read: at dusk the
    Night trigger's `+tex`, then the Dusk event's new `` `rtl `` and its `+tex`).
  - Day and night change on listeners of their own (`FUN_6f283c80`, event 0x80263):
    by day, on the time falling below Dawn less 0.01 or rising past Dusk plus 0.01;
    by night, on rising past Dawn plus 0.01 or falling below Dusk less 0.01 (0.01 at
    0x6fcd53a4).  So night comes at 18.01 (read: the change at main clock
    200.199966, turn 2006, two turns after the dusk triggers), with its one
    `` `rtl `` and each unit's `` `rcm ``, the last change's listeners (two by day)
    freed after them.  Creeps sleep by this night, not the time's (`World::is_night`),
    and each that falls asleep then frees its spells' listeners, the last first, a
    fired one's replacement among them (made as its mana passed the cost: one
    waiting for it to fall), Ensnare's `` `pil `` after the listener made before it
    (read: twenty objects freed at nightfall), all as the change ends, after the last
    change's listeners (read: before a walk's tree in the turn's last step).
  - Through midnight the time rises to DayHours and falls back from there (its
    FloatModifier set back, event 0x80262 at main clock 320): the listeners waiting
    for a fall past their values fire, the latest made first (read under winedbg:
    the change's, replaced by one on the rise past Dawn and 0.01, it still being
    night; the equal Dusk and Dawn events', each replaced by one the other way, their
    triggers not run; then less than's, Day's and Night's, their triggers run), the
    old listeners and the executions freed at the tick's end in that order
    (`World::time_listeners_crossed`).
  - The time of day moves on to each step's main tick as the step begins, its
    listeners passed firing there (read: dawn's triggers after a turn's first step,
    before the walk ends of its second; `World::time_of_day_to`).  The change between
    day and night lets the last change's listeners go as it ends (one set by script,
    as the next turn starts).  A creep woken by the day makes each spell's listener
    on its mana as it stands: one whose cost it is past waits for it to fall (read:
    creeps waking with full mana, none replaced after).
  - An aura reaches only units it serves, as on its bearer: Trueshot those with a
    ranged weapon, Vampiric those with a melee one, War Drums and Command those with
    a weapon, Brilliance those with mana (read: Trueshot's buff on no critter passing
    a bearer; `World::aura_serves`).
  - The objects timed by the main clock (each unit's of a change between day and
    night, three seconds; a mine's FloatModifier) are freed as the main clock passes
    their time: before the step whose tick it reaches, and before an order on the agent
    queue due after them (read: those of a nightfall, due 683.2, freed before a
    wanderer's order due 683.2195, both before a turn's second step;
    `World::timed_frees_to`).
  - A replay's checksum records are the keepalives (read: a LAN game's
    TempReplay.w3g, turn for turn).  A replay played back makes no keepalives (read:
    the module loop never ran through a whole game's replay), so the original does not
    check them.  In a game played alone the first keepalive's time is 0 and, with a
    computer player, 58 more objects were born by then than the port makes (13014,
    presence 13007): the computer's AI makes engine objects the port's does not yet.  The original's record: for a network game flags
    0x8000, game type 9, its players' count, its host's record and name; for a game
    played alone flags 0, 12 players, "Local Game", the profile's name, and game type 1,
    9 for a map Blizzard signed (Glacial Thaw, Booty Bay; a signed copy renamed in
    Maps\Test, bbw4.w3x, is 1, as game.dll's map check, `FUN_6f78b640`, signs the
    SHA-1 of the file to its signature and its name alone in capitals: provisional,
    the check's use for the type not read); the word after the type 0x26fb68 in every
    replay read; then the start
    records 0x1A to 0x1C (each 1), each turn followed by its checksum, a chat line as
    0x20 (the sender, flags 0x20, its recipients, 0 for all in a game played alone,
    and the line) before the turn after it, a leave as 0x17, and the local player's
    leaving at its end: reason 12, its LEAVEGAME word (7 left by the menu, even after
    its victory; 11 by the defeat dialog's Quit Game or the victory dialog's Continue),
    and the program's games so far (one more as each begins and once more at a game
    over, however it is left: 1, 2, 4, 5 for a quit, a quit, a defeat, and a quit in one
    run; 2 for a defeat or a victory in a fresh one, 3 for the game after that victory,
    4 for a second victory; the LAN host's 7 and 2 after its victory).  As the local
    player leaves, `FUN_6f985030` writes one for each player still in the game, by
    network id, each with the local word (the original joined to a LAN game as its
    second player: the host's, then its own, both 7); another's leave
    (`FUN_6f98c680`, after the host's PLAYERLEFT) has reason 1 and its word (1 for a
    guest the original host dropped as out of step, counted before its victory).  A
    network game's record names its host first, then the others, the local player
    among them.  A host writes 0x23 at a desync (`FUN_6f9680a0`: a word, a counted run
    of at most 96 bytes, and one of at most 16; read: 1 and 2, the checksum, and pid
    1), and the original's replays hold 0x21 too (two bytes).  The original's game
    played alone sends Pause (0x01) as its game menu opens and Resume (0x02) as it
    closes, its turns going on at their length meanwhile, and Pause as a script dialog
    shows (Mafarazzo TD's at its turn 12); quit through the menu, its last Pause goes
    unanswered.  The port notes its menu's Pause and Resume for its replay
    (`Game::note`), its menu stopping the game by itself, so both fall in the turn after
    the menu (the last Pause in a turn of no time); not yet a script dialog's.
    Settings: a map loaded directly MAP_LOCK_RANDOM_SEED alone (0x800002), a Custom
    Game the setup's options (0x64802 by default).  Turns of a game played alone are of
    96 to 101 ms of game time.  A campaign mission is recorded nowhere
    (Sentinels chapter one quit through the menu: LastReplay.w3g untouched, no
    TempReplay.w3g, no Save Replay on its score screen), and a record the original does
    not know ends its reading: it would not load the port's replay holding 0x7E ("The
    replay file could not be loaded"; `FUN_6f329230`'s default case).  The game's
    description is coded once (it holds no NUL), with the map's path from the game's
    folder, playable size, and checksums.  The port writes its replays so
    (`Replay::to_bytes`), all in the original's records: a map loaded directly with
    the slots the original's -loadfile makes (`Game::loaded_record`), steps of whole
    milliseconds, its chat and leaves; its replay of the quiet game played from the
    original's turns is the original's record for record (31252 records, all its turns
    and checksums); the original plays the port's replay back (read: at
    1x on the hidden display, its replay panel showing the game's time).  A replay can
    also carry a recorded game's actions in another's record (`w3gsplice`), for tracing
    the original as it plays one back: no keepalives then, but its objects and steps.
    A network game's record copies the game's setup: the game type its host offered
    (1 for a custom map's, 9 for a Blizzard map's), its slot table's slots as they were
    (empty slots' download 255, fixed player settings' races without the selectable
    flag), their count as the players' count and start spots, and its layout as the
    select mode (3 with fixed player settings); read on the original's replay of a
    Mafarazzo TD LAN game (12 slots, type 1, mode 3) against Booty Bay's (2, 9, 0).
    The port's (`replay::NetRecord`, from the host's or the joined game's slot table
    and type) is now the original's record for record but the language word, and the
    original's replay of that game plays on the port (`w3gplay`, which now loads a
    custom map's rule set and its computers as a network game does) to turn 576, as
    its LAN capture does.
  With these Booty Bay without actions (`docs/reference/bootybay_quiet_long_*`) agrees
  with the original's trace and keepalives for all 7300 turns compared, its replay
  (`docs/reference/bootybay_quiet_long.w3g`, `w3gplay`) to turn 9014, and the long
  lumber game to 5663.
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
- A structure without a pathing texture makes no footprint regions (three `^pmr` for
  one with), and a type's `auto` ability that cannot autocast (no Orderon) makes no
  order: Mafarazzo TD's Fantas, structures of collision 31 with `auto` Avul, made with
  neither in the original's census at the game's start.
- A ground unit of collision 0 keeps a cell of its own as it is made, as one of any
  size does: Mafarazzo TD's creeps (collision 0), 30 made at each spawn's point, stand
  the first on it and each other at the middle of the next free cell about it, ring by
  ring (each ring's bottom row from its second cell, its right column up, its top row
  back, and its left column down to its corner; measured by pinging their places in
  the original right after New_Round, all 180 the port's now).
- A structure a script makes where it cannot stand (CreateUnit, `FUN_6f677870`, its
  point snapped first; CUnit vf55 `FUN_6f67f490` with the structure check
  `FUN_6f69b200`) is moved by `FUN_6f6515f0` (`FUN_6f058cd0`, `FUN_6f16ecc0`,
  `FUN_6f14a1e0`): up to 31 rings of pathing cells about the snapped point, each walked
  from side 2 (`FUN_6f14b580`: its bottom row from the second cell, its right column
  up, its top row back, its left column down) whatever the facing (the 2 a constant,
  pushed at 0x6f651748), each cell's corner tried as the build check snaps it
  (`FUN_6f66f050`), and only a cell on the asked point's cliff level (`FUN_6f654060`
  comparing `FUN_6f78bc90`: the layer of the terrain corner nearest the cell,
  `FUN_6f744040` by `FUN_6f750100`, or a higher one laid over it, `FUN_6f782bf0`, not
  yet read); with none it stands where asked.  Read: Mafarazzo TD's Vengence
  requirements, made on the creep path's unbuildable ground, its build ground a level
  up beside it, stand where asked in the original (its Red peasant's detection finds
  the five within reach there, where the port, having moved them up to 600 onto the
  build ground, found four).  Open: the commit that began the ring walk (5489c9b)
  measured structures facing 0 and 270 landing as walked from the side their facing
  names; the code reads side 2 for every CreateUnit, and those maps were not run again.
- A flyer's burrow detection (CAbilityDetector) looks with its in-range listener's
  search (`FUN_6f15f870`, `FUN_6f15fa80`, `FUN_6f15f9b0`) and test (`FUN_6f15eac0`): a
  unit's position other than the listener's own whose collision (CPoPos +0x90, in
  cells) is above 0, nearer than the range (Rng1, CAbilityDetector +0x8c, from
  `FUN_6f41b4f0` by `FUN_6f4319f0`) plus that collision (its distance squared under
  the sum squared).  Those newly in come in the reverse of the order the search found
  them (`FUN_6f15f2a0`), each passed to the detector once while in range (event
  0xd01b4, `FUN_6f407060`, its in-range object's set at +0x84); the unit's one record
  (`Cdet`, at its +0x130) is made by the first detector of any player to find it
  (`FUN_6f688990`), each player's count kept in it (`FUN_6f688940`; leaving, event
  0xd01b5, `FUN_6f43aca0` and `FUN_6f678150`, only counts down).  Measured (a
  breakpoint on `FUN_6f688990`
  from 0x6f4070f8, its ecx the unit found): at 0.5 seconds the Red peasant's look finds
  h00Y, h00X, e000, h00Z, and h013 in that order, the towers where the script asked
  for them (above), the reverse of the units' 256-unit blocks (`spatial_order`); the
  port's look now finds the same in the same order.
  - A unit's position's collision is its type's held to at least 1 (`CUnit::vf91_6f6945a0`
    against `DAT_6fd3c748`, from_int(1) by the static initializer at 0x6f001b80;
    `DAT_6fd3c744`, the test's 0, is from_int(0) at 0x6f001a80), over 32
    (`FUN_6f05c2e0`, `FUN_6f15fef0`), so a unit of collision 0 is found within the
    range and 1.  Read: Mafarazzo TD's creeps (n000, ucol 0) found by Red's flying
    peasant, six at 21.5 seconds in the flyer game (`abtest/mafb_detect.txt`), one at
    46.0, three at 46.5, and on in the LAN game (its trace from turn 391).
  - The test takes the position where its velocity carries it by the look's moment
    (`FUN_6f161040`), not where its last step left it: at 21.5 the six within 901 so,
    where the places after the turn's steps had eight.
  - The look's timer is due each half second from the detector's making (for Red's
    peasant made with the map, on the half second), so it comes among the turn's
    movement steps (on the relation dispatch, after the agent queue's events of its
    main tick, below): at 46.5 after the turn's
    first step (due 46.49865), its region trigger's order and frees, and before the
    paths freed at the second (46.52865); at 47.0 before the first step's (47.00863).
    The port so takes the looks due in a turn among its steps
    (`World::plan_detection_looks`, `World::looks_before`), its walkers placed at the
    look's main tick; provisional: only on a map without aura bearers, where an aura's
    scans (the same queue) would come with them, which the port keeps at the turn's
    end with the looks (`update_auras`).  The port's detector clock had summed tenths
    in f32 and looked a turn late by 46 seconds; it counts whole milliseconds now.
    Mafarazzo TD's LAN game so agrees for all 502 turns of its keepalives and its
    objects through turn 501 (the trace ends in 502), and the flyer game to turn 217
    (from 216; at 217 the original's tower takes up a creep).
  - Each walker is carried on to the look's due, its queue's clock as its timer falls
    due, not the main clock of the tick it falls in (`World::looks_before`; the queue
    the relation dispatch, below).
    Read: a plain Mafarazzo TD game traced from turn 569 (`abtest/mafa5.*`), where the
    original made four `Cdet` at turn 576 and the port three, a fourth at turn 581:
    a creep closing on the second flying peasant at 206.8 a second, 901.47 off it by
    the main clock of the look's tick (57.496616) against its 901, 900.78 by the due
    57.5.  So that game agrees for all 624 turns, the replay game for all 582, and
    the old capture to turn 659 (each from 576).  Not the next main tick's clock:
    by that the flyer game's look at 22.0 found a creep more than the original's
    nine (turn 221).
  - `FUN_6f161040` (the time since a position was placed, `FUN_6f15c6c0` →
    `FUN_6f15cea0`) reads the relation dispatch's clock, `DAT_6fd53a48` + 0x54, for
    the game's positions (+ 0xa8, the local dispatch's, which runs with the frames,
    for a position flagged 0x80000000 at its +0x14, `FUN_6f15c650`).  That queue is
    moved on after the agent queue in each main tick (`FUN_6f04c1a0`), its clock
    reading each request's due as it runs (`FUN_6f052380`): so during the agent
    queue's events (the steps, a damage point) it reads the last main tick's clock,
    during its own requests their dues, and after it the tick's.  The detector's look
    is such a request: an in-range listener's (NIpse::CPoPoInterfLis vf8
    `FUN_6f15ef40` → `FUN_6f15f210` → `FUN_6f15fa80` → `FUN_6f15f9b0` →
    `FUN_6f15eac0`).  Measured (breakpoints on `FUN_6f15eac0` from 0x6f15fa34 and on
    `FUN_6f15ff40`): the plain game's look at 57.5, its 41 tests with + 0x54 at
    57.500000 exactly and + 0xa8 at 57.288067; the Slow Poison's landing in mafq's
    behavior dispatch with + 0x54 at 12.603886, + 0xa8 at 12.674256.  So a look comes
    after the agent queue's events of its main tick, the steps due in it among them:
    the port takes one before a step only when it falls due by the main clock of the
    step's tick (`World::looks_before`).  (No game here has a step and a look in one
    main tick the other way round: all agree as before.)
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

- Units' collision regions stand at the middle of the cell the unit is in, not at its
  exact place: on each axis the cells floor(c - 0.5 - r) to floor(c - 0.5 + r), c =
  floor(p) + 0.5 and r the collision in cells (read: the original's cells at a
  sheep's search, a forest troll at 378.28, 122.25 covering 377 to 379 and 121 to
  123 from the game's start, another at 383.77, 121.60 covering 382 to 384 and 120 to
  122).  The searches and the walkers' holds both see them so; the quiet game's
  replay then agrees to turn 13709.
- A walk to a goal within reach starts in place only when the goal lies near the
  walker's heading: a sheep 0.81 off did, sheep 0.915 and 3.0 off and a pig 1.40 off
  did not (each turn 0.5).  The engine's test between 0.81 and 0.915 is not yet read;
  the port takes 0.86 (not 2 turns, 1.0, which the 0.915 sheep contradicts).  One whose
  first step's turn leaves it on the bearing (a sheep 0.636 off, 0.419 away, turned
  0.5 in its first step and done in its second, read in the original's steps) frees
  its objects as an arrival does, at its end, its path as the next step starts; the
  port takes "on the bearing" as within 0.2 after one turn (provisional).  With these
  the quiet game's replay agrees with the original's for all 15624 turns.
- A trigger's sleep ends by the network's word (`w3_game::world::Execution`).  The
  sleep runs on its execution's own timer (CTriggerExecution +0x34, a
  CAgentTimerExtended started once by `FUN_6f2998f0` on the timer queue +0x44), and
  that queue moves by real time: CNetEventAsyncIdleTick 0x40090094 carries the
  milliseconds since the last tick (`FUN_6f32d670`, its stamp from
  QueryPerformanceCounter or GetTickCount, `FUN_6f092a20`) to `FUN_6f36ab40` and
  `FUN_6f04c0d0`, so each computer times a sleep by its own clock.  The timer's end
  (event 0 in CTriggerExecution vf3, `FUN_6f298c80`; a sound wait's poll, event 1,
  `FUN_6f299020`, ends the same way) sends action 0x62 (CNetCommandResumeTriggerExec,
  `FUN_6f270430`): the execution's id (tag and birth) and its count, +0x90, which is 1 as
  the execution starts (`FUN_6f298250`) and is written nowhere else but on a load
  (`0x6f298885`) and by `FUN_6f299380`.  Every computer sends its own: the filter that
  holds back most actions of a player not playing lets 0x62 through (`FUN_6f32bbd0`,
  from the sender `FUN_6f32c920`).  Carried out (the handler at 0x6f26f360 looks the
  execution up by the id and passes only the count), `FUN_6f299380` lets the thread go
  on (`FUN_6f2975c0`) and counts one more when the count is the execution's: so the first
  0x62 to come, whoever sent it, ends the sleep on every computer at the same turn,
  and the rest, their count gone by, do nothing; one that comes before this computer's
  own timer is done ends the sleep all the same.  The script's run returns state 4 when a
  native set the thread's +0x3c (TriggerSyncStart, `jass_TriggerSyncStart`;
  `FUN_6f91b460` after a native call: state 3 for a sleep, +0x34, first): the execution
  then notes the players whose slot state is playing and controller user (+0x8c,
  `FUN_6f299c00`), holds the thread with no timer, and sends its 0x62 at once.
  TriggerSyncReady (`jass_TriggerSyncReady`, nothing outside an execution) sends 0x63
  (CNetCommandTriggerSyncReady, `FUN_6f2711a0`: the execution's id, read by
  `FUN_6f331280`) and sleeps 0 on the execution; each 0x63 carried out (handler
  0x6f26fce0, passing the sender) takes its sender off the noted players, and those
  neither user nor playing, and with none left lets the thread go on, the count
  unchanged (`FUN_6f299b80`).  Read in a LAN game of Mafarazzo TD whose host
  (`crates/net/examples/host.rs`, then sending its backlog of turns at once) ran no
  game: the original sent 0x62 with count 1 at turn 198 and count 2 at turn 909 for
  its Initialization trigger's execution (0x560), the first and second of its sleeps
  (2 and 4 seconds of its real time; the game time ran far faster), each ending as its
  own 0x62 came back, and the execution lived on (its third sleep, 2 seconds, not yet
  over at the capture's end).  A game played alone sends them too (a replay of one,
  `Replay/victory.w3g`, holds a 0x62 with count 1).  The port does this in a network
  game and in turns from one (a replay's, the tags example's), timing the sleep by its
  own sleep clock and sending its 0x62 and 0x63 to the host; alone it still wakes its
  sleeps itself.  Open: whether an ExecuteFunc's execution (`FUN_6f298060`, which
  leaves +0x90 as its constructor `FUN_6f295130` does) counts from 0 or from what its
  memory held; the port's ExecuteFunc and timer callbacks keep no execution of their
  own, so their sleeps wake as alone.

## Units and structures

- A structure's acquisition range lives with its attack (CUnit `+0x1e8`, the range at
  its `+0x244`; GetUnitAcquireRange `FUN_6f2039a0`, SetUnitAcquireRange `FUN_6f214ea0`,
  GetUnitDefaultAcquireRange reading the type's), so an upgrade into another type
  acquires at the new type's range: a Scout Tower, weaponless, upgraded to a Guard
  Tower attacks what comes near, and Mafarazzo TD's 200s Missile Tank (acquire 1200)
  is not held to its Arrow Tower's 700.  The port re-reads it, and the model scale, as
  the upgrade finishes (provisional: whether a range a script set is kept).
- An attack's times are its weapon's as the attack reads them (CAbilityAttack
  `FUN_6f49cab0`, for each weapon whose record's word 14 is set: provisionally, each
  weapon the unit has; the port reads every weapon so): the damage point and
  backswing at least 0.01 (0x6fcd53a4); the cooldown at least 0.1 (0x6fcd53c4) and at
  least the damage point and 0.02 (0x6fcd53a8); the backswing, if the damage point,
  backswing, and 0.01 pass the cooldown, the cooldown less the damage point and 0.01
  (at least 0.01); and the range no farther than the acquisition range (the attack's
  +0x244, set from the same record just before), if past it by 0.001 or more.  So a
  weapon's cooldown never falls below its damage point and 0.02: Mafarazzo TD's ten
  towers of cooldown 0.2 down to 0.01 (and the three of 0.3), all of damage point
  0.3, attack every 0.32 seconds (measured: its Plague Tower, cooldown 0.2, every
  0.32), and its 100s and 200s Missile Tanks, ranges 1250 and 1500, reach 1100 and
  1200, their acquisition ranges.  Melee data has none of these but backswings cut short (the
  Watch Tower's 0.3 to 0.29, the Spirit Towers' 0.5 to 0.49, a few creeps') and the
  Locust's 0 times.
- The attack cycle runs on three timers of the attack: its cooldown (+0x1d8, event
  0xd01b0), its damage point (+0x1ec, 0xd01b1), and its backswing (+0x200, 0xd01b2).
  An attack starts only once the cooldown has run out (`FUN_6f495180`); it starts the
  cooldown and the damage point together (`FUN_6f49d130`), the damage point at most
  the cooldown less 0.01.  At the damage point (`FUN_6f49a5b0`, `FUN_6f4946b0` with
  `FUN_6f49d050`) the cooldown, if it has less than 0.02 left, is made 0.02, and the
  backswing (at least 0.01) is held to end 0.01 before the cooldown does.  The
  cooldown's end (`FUN_6f49a4f0`) tries the attack again at once (`FUN_6f499310`,
  `FUN_6f495180`: the target still valid, in reach, and in sight).  No attack starts
  while a damage point is pending, and the animation limits nothing (`FUN_6f49c440`
  restarts it with each attack).  With the times as read above the timers' limits
  matter only to an attack sped up (each time over the attack's speed factor,
  `FUN_6f498670`, between 0x6fd6bf70 and 0x6fd6bf6c), where the cooldown can come
  within 0.02 of the damage point; the cooldown's other factor (+0x1c0, below 0) slows
  only the cooldown.  A cancelled attack whose damage point is pending loses its
  cooldown too (`FUN_6f497e20`).
- A structure that uses food (fused) needs it free to be ordered built, as a unit
  trained does (`Nofood`); custom maps limit structures so (Mafarazzo TD's HSBC uses
  1 and its Castle makes 1: one HSBC a player).  Melee data has no such structure but
  the neutral `nfgo`.  Provisional: whether the construction's start checks again.
- Pillage (`Asal`, CAbilitySalvage) pays on what its level's targets allow: its
  damage hook (`FUN_6f5431a0`) has no structure test, only the ability's target mask
  (`FUN_6f6a9ef0` filling an empty type class with ground, air, and structure), and
  reads the cost of any unit type.  Its own data's `structure,enemy,neutral` keeps it
  to buildings, but a map may widen them (Mafarazzo TD's banks, A000, `enemies`, pay
  on the runners; their tooltips' "~4-5 dollars per hit" is the cost share on a
  13000-life runner made from hhdl, 240 gold: 400 / 13000 × 0.6 × 240).
- Pillage counts the life a hit took, and pays once for each attack made ready.  The
  damage (CUnit vf72 `FUN_6f69b380`) calls `FUN_6f69ca80` with the hit's final
  damage before it takes the life (vf74 `FUN_6f670560`, after): that fires the
  damaged event (`FUN_6f67b740`, 0x80234) and gives the attacker's damage hooks (vf92,
  `FUN_6f66a350`) the damage, at most the target's life as it stood (vf75), when the
  damage is an attack's (vf72's third argument, the damage's flag 0x100).  So a
  killing hit pays, for what was left of the target: an HSBC's 501 on a 180-life
  runner counts 180.  CAbilitySalvage's hook (`FUN_6f5431a0`) runs only while its
  flag 0x200 is set and clears it first, whatever it struck; the flag is set at each
  attack's damage point, when the attack's target passes the ability's targets
  (vf77 `FUN_6f560c20`, called with the other class 0xb to 0x1a abilities by
  `FUN_6f68f180` from the attack's `FUN_6f4946b0`).  Of two missiles in flight at
  once, the first to land pays and the second nothing.
- Moon Glaive (`Amgl`), Slow Poison (`Aspo`), and Pillage (`Asal`) are passive: their
  art is a passive button's (`PASBTN...`) and their buttons give no order.
- Slow Poison and Poison Attack stack by their DataD (CAbilitySlowPoison vf211
  `FUN_6f623170`, CAbilityPoisonAttack vf211 `FUN_6f622ee0`, its low three bits;
  the base Aspo and Apoi have 1).  Without a stacking type a hit puts or refreshes
  the plain buff (`Bspo`, `Bpoi`).  With one, each caster has its stack info on the
  target (`Bssi`, `Bpsi`, the engine's own codes), refreshed by its later hits for Dur
  (`FUN_6f625a40` finds it by its caster); a new info joins the shared strike (the
  info's +0xf4, else `Bssd` or `Bpsd`; provisionally its data's second BuffID, as the
  port takes it): the first makes it with all of its own
  DataA a second and slows (DataB and DataC), each later one adds the parts its type
  stacks (bits 1, 2, and 4: DataA, DataB, DataC) and takes them away as it runs out,
  and the strike goes with the last (CBuffSlowPoisonStackInfo vf213 `FUN_6f6236e0`,
  vf214 `FUN_6f62de00`).  Measured: a Plague Tower's first hit puts `Bssi` then `Bssd`
  on its target, and its later hits take nothing more.  Bit 8 is passed to the strike
  and not read; Orb of Venom's and Envenomed Spears' classes are not read (the port
  keeps their plain buffs).
- An attack's missile is an agent of its own, a CBullet (CBulletBase, CWar3Image),
  made at the damage point (`FUN_6f4946b0`, by the weapon's kind: 2 missile, 3
  artillery, 5 msplash, 6 mbounce, 7 mline, 8 aline; 1 normal and 4 instant strike at
  once).  The weapon's own class: CMissile `B-Mi` (`FUN_6f497520`), CArtillery `B-Ar`
  (`FUN_6f4973c0`), CMissileSplash `B-Ms` (`FUN_6f497730`), CMissileBounce `B-Mb`
  (`FUN_6f4975d0`), CMissileLine `B-Ml` (`FUN_6f497680`), CArtilleryLine `B-Al`
  (`FUN_6f497470`).  But first the attacker's abilities are asked, by their place
  (CAbility vf78, 0x1b to 0x29, each place in turn, `FUN_6f67fbd0` for a missile and
  its kin `FUN_6f67f950`, `FUN_6f67fb30`, `FUN_6f67f9f0`, `FUN_6f67fa90`,
  `FUN_6f67f8b0`), each by its hook for the weapon's kind (vf84 missile, vf80
  artillery, vf83 msplash, vf82 mbounce, vf81 mline, vf79 aline; the base's return
  0); the first to take it makes its own.  Read: Slow Poison, Poison Attack, and
  Envenomed Spears (place 0x28, vf84 `FUN_6f632790`, the missile only): while not
  disabled (+0x3c below 1, vf154 the tech tree) and at a unit its targets allow
  (`FUN_6f672140`), its art put in (`FUN_6f415ff0`), Slow Poison's CMissileSlowPoison
  `Mspo` (vf213 `FUN_6f62b870`), the others' CMissilePoisonAttack `Mpoi` (vf213
  `FUN_6f62b460`; Orb of Venom's CAbilityPoisonDamage, place 0x1a, is never asked).
  Frost Attack and Frost Breath (place 0x29): an artillery's CArtilleryFrostBreath
  `Rfrb` (vf80), a splash's CMissileFrostBreath `Mfrb` (vf83, no target check), a
  missile's CMissileFrost `Mfro` at a unit its targets allow (vf84, after
  `FUN_6f671e70`, not read).  Not yet read: the hooks of Bash, Critical Strike, and
  Drunken Brawler (place 0x1b), the orbs (CAbilityDamageBonusBase and its kin, place
  +0xa0 + 0x22), Black, Searing, Cold, Poison, and Incinerate Arrows, Incinerate,
  Corrosive, Freezing, and Frost Breath's kin, Liquid Fire, Feedback, Frag Shards,
  Plague Toss, Balls of Fire, Lightning Attack (a CRope, `Mlit`), and Spider Attack;
  Barrage (place 0x28) takes nothing but sends a CMissile at each other target first
  (vf84 `FUN_6f58f700`, `FUN_6f6d6920`, `FUN_6f6d3ec0`).  The classes' codes, from
  their vf7: `B-Mi`, `B-Ar`, `B-Ms`, `B-Mb`, `B-Ml`, `B-Al`, `Mspo`, `Msss`, `Mpoi`,
  `Mpas`, `MIpb`, `Mfro`, `Mfss`, `Mfrb`, `Rfrb`, `MHca`, `MNba`, `MOcr`, `MIcb`,
  `MIsb`, `MIlp`, `Mliq`, `Mfre`, `Mfrz`, `Mcor`, `Mbof`, `Rbof`, `Rfsh`, `Mpts`.
- A missile's objects, as it is made and sent (traced: Mafarazzo TD's Plague Tower's
  every attack; a test map's Arrow, Cannon, Ice, and Cutter towers'): the missile, its
  position `` `pbh `` (NIpse::CPoPosBh, the CBullet's +0x78, made with it), that
  position's two regions `^pmr`, and its flight `>prj` (NIpse::CBhPoProjectile,
  started by `FUN_6f6d3e50` → `FUN_6f89d1b0`).  One an ability sends (CBuffMissile
  vf52, `FUN_6f6d2c50`; a splash's `FUN_6f6d2db0`, an artillery's `FUN_6f6d2b30`)
  counts itself on its target: the target's CAbilityTarget (`Atrg`) for that ability
  (its +0x34, the data's id) and the attacker's owner (vf 0xec), made with the first
  in flight (`FUN_6f48dfa0`, `FUN_6f489580`), counted at +0x74, let go as the last
  lands (`FUN_6f48c7f0`, `FUN_6f589120`, `FUN_6f48e8a0`); traced, `Atrg` after the
  `>prj`.  A bounce's (CMissileBounce vf50 `FUN_6f6d3790`) marks its target with a
  buff, `BTAR` (`FUN_6f6d4700`; parent `buff`), after the `>prj`, and each next target
  as it bounces on; the same missile flies on (vf47 `FUN_6f6cfbc0`).  Traced: the two
  Cutter `BTAR`s kept 2.7 seconds and freed with their unit's removal, the newest first,
  after its AI's objects (provisional: whether they end sooner).
- A missile's flight (NIpse::CBhPoProjectile, `FUN_6f173450`) falls due at the
  distance to its goal across the ground over its speed (`FUN_6f1737d0`), at most 60
  seconds (0x6fd3c7b0).  An attack's missile at a unit (CMissile's launch,
  `FUN_6f6d3470`) is aimed where it meets its target: the target's place carried on
  to the relation dispatch's clock (`FUN_6f05a970`: `FUN_6f161040`, `FUN_6f05bdd0`,
  in world units) less the start, R, and its velocity, V (`FUN_6f05aaa0`), the
  meeting the later root of (V·V - s²) t² + 2 R·V t + R·R (`FUN_6f9ac7f0`, a stable
  quadratic), at the target's place and V t on (with no root, at its place), the
  start the attacker's place and its launch offsets turned by its facing
  (`FUN_6f68c4e0`).  It then flies straight to that point (`FUN_6f89d1b0` with no
  unit, `FUN_6f173450`; not `FUN_6f173690`, the homing launch), not aimed anew.  Its
  due is a request on the behavior dispatch, that queue's clock (`DAT_6fd53a48` +
  0xfc, kept with the main clock) plus the flight's time (`FUN_6f171870`, at least
  0.0001), so it lands in the first main tick whose clock reaches it, after that
  tick's agent events (vf18 `FUN_6f173cf0` from the dispatch, 0x6f054336), wherever
  its target is then.  Measured (the flyer game's Arrow Tower, 1800 a second;
  breakpoints on `FUN_6f1739e0`, `FUN_6f171870`, `FUN_6f1737d0`, `FUN_6f173690`, and
  vf18): sent at 22.062298 on the main clock, the delta -12.986168, 13.593445 cells
  (its R -418.046631, 507.154297 and its V 7.448327, -215.923859 in the caller's
  frame), its time 0.334218, so due 22.396516, landing in the main tick 22.397242;
  no `FUN_6f173690` nor `FUN_6f1739e0` (what calls vf17 `FUN_6f173dd0`, a signal
  `` `bpe ``, is not read).  The port so aims and lands a structure's missiles
  (`World::bullet_aimed`, `World::fly_bullets`), its landing's main clock that first
  tick's, its objects going 5 seconds and a tick on as before; the other units'
  still fly its old way, a leading intercept from the target's place at its sending
  and at each tick's end (`World::bullet_flies`; provisional).  With this the Plague
  Tower's trace agrees to turn 181 (from 140), its 151 pings as before; at turn 182
  the original frees a missile's objects (sent at 12.846) a turn before the port.
  A structure's damage point is counted from its look's due, as its attack's timers
  on the agent queue are (the look's due, the first attack 0.1 on, the damage point
  its damage point on), its missile sent by the main clock of the last main tick
  short of it (`World::look_time`, `World::bullet_aimed`): the flyer game's look due
  21.662787, its damage point 22.062787 (the port's 22.062786), sent by 22.062298, its
  R and V the original's to the last digit, its time 0.334214, landing in the main
  tick 22.397242.  (The port had counted from the game time of the look's main tick,
  21.660011, its flight from 22.052299.)  The flyer game then parts at turn 274: the
  Arrow Tower's eighth missile kills its creep (60 life, 8.25 a hit), whose death the
  original follows with ten tasks (a block, eight tasks, a block), an effect made and
  gone (`+EIP`, `` `pos ``, two `^pmr`, `pudd`), the bounty's `+tex`, 28 of its objects
  freed, and the tower's take-up of the next creep; the port makes only the `+tex`,
  and its tower's swing begun before the death sends a missile at the dead creep.
  Neither the death's objects nor an attack ended by its target's death (the
  reaction's wait, task 0xd0163, `FUN_6f49ea80`) is yet in the port.
- A missile landed starts its removal timer (CBullet vf42, CMissile `FUN_6f6d0cf0`,
  CArtillery `FUN_6f6d0c70`, then `FUN_6f6d0c90`: MiscData's BulletDeathTime, 5, event
  0xd019f at +0x64), at whose end it destroys itself (vf3 `FUN_6f6d4e30`, vf23
  `FUN_6f0557b0`) with its objects: its flight, its regions in the order made, its
  position, itself.  A missile whose attacker is removed meanwhile is not affected
  (traced: the Arrow tower's last, landing 0.4 seconds before the tower's removal,
  freed 5 seconds after it landed).  Destruction is on the next frame: read with a
  main tick (0.005) after the due, the Plague Tower's first missile, landing about
  0.8994 and its `Atrg` let go then, has the `Atrg` freed in turn 10 and its objects
  in turn 60, and the second's `Atrg`, let go about 1.239, freed before the third
  missile, sent at 1.255, made a new one (provisional: the frame a main tick).  The
  removal timer runs from the main clock of its landing's tick, and the missile goes
  in the main tick that timer falls due in (`FUN_6f6d4e30` from the agent queue's
  dispatch, 0x6f060d12), as does a buff laid by its landing, its timer started first
  and so gone first.  Measured (breakpoints on vf18, `FUN_6f6d0c90`, and `FUN_6f6d4e30`,
  mafq, 5.1 to 18.3 seconds): the Plague Tower's missiles landing at 5.424845,
  12.603886, 13.193787, and on, each `FUN_6f6d4e30` at the landing and its
  `FUN_6f6d0c90` with the 5, then `FUN_6f6d4e30` again from the queue in the main
  tick reading 10.424252, 18.192947, and so on.  The port so frees an aimed
  missile's objects as the main clock passes its landing's tick and 5, a laid buff's
  as it passes its end, and a damage point frees nothing past a laid buff's end not
  yet let go (`World::bullet_landed`, `World::update_buffs`, `World::swing_lands`);
  with these the Plague Tower's trace agrees to its end, turn 200 (from 181).  What
  falls due after a turn's first damage point sending a missile waits for it, though
  the port's movement steps, which free what is due by their clocks, run before the
  units' damage points (read: the Arrow tower's first missile's objects, due 6.833,
  freed after the Cannon tower's missile sent at 6.815, and the Cannon's after the
  Ice tower's so; `World::first_missile_point`, which weighs the turn's laid buffs'
  ends too).  A removal falling due in a step's main tick, after the step, goes after
  the step's region trigger's objects and before its end's frees (traced: the second
  flyer game's Arrow Tower's missile, sent in turn 285, its five freed so in turn
  338; `World::step_tick_frees`); the port's had waited for the next step, and that
  game now agrees for all 341 turns of its keepalives and its trace (from 337).  A
  structure's damage point is an agent queue event too: what an earlier main tick let
  go is gone before it (traced: the upgrade game's take-up's block at turn 507, its
  slot then the h00X's shell's).
- A line artillery weapon (`aline`) sends a shell as an artillery does, to where its
  target stood (CArtilleryLine, `B-Al`), not an instant blow: its objects `B-Al`
  `` `pbh `` `^pmr` `^pmr`, then two more positions with their regions, `` `pcu ``
  `^pmr` `^pmr` twice, then `>prj` (traced: the upgrade game's h00X, weapon 1 `aline`
  at 2200, its first shell at turn 507, where the port had struck its target and the
  one beside it dead at once).  (Provisional: what the two positions are and when
  they go, its landing not yet traced; the port frees them with its removal.)  The
  upgrade games now agree to turn 510 (from 507), the trace to the end of its window
  (509).  Its shell flies to where its target stands as it is sent and lands there,
  its flight due by its way at its speed from the main clock then, as a missile's but
  with no lead (traced, `abtest/mafv5.*`: the first, sent at 50.607, landing in turn
  512 in the main tick of 51.120121, its due 51.116651; flown at the creep closing on
  it, the port's had landed in 511, and aimed at where they would meet, as early).
  What an artillery's splash kills bursts: its death's course is a burst's four,
  0xd015a, 0xd0157, 0xd0154, and 0xd0156 (`FUN_6f678280`), the kill's objects nine
  (traced: the five creeps the shell killed, each `tskA`, seven tasks, `tskA`,
  `pudd`, the death trigger's `+tex`).  It bursts as a unit KillUnit bursts
  (`World::burst_begun`, `World::burst_end`): its objects all go together 0.0001 on,
  before the next step, for each in turn the towers attacking it giving it up (in
  the order they took it up), its AI's, its kill's first task, its walk's tree, its
  kill's ninth to seventh, its `` `rtl ``, its death trigger's execution and `pudd`,
  and its kill's sixth; then each's end.  The splash takes its units by 512 square,
  west to east, the newest first in each (traced: killed 1879048381 and 380, then 367
  and 355, then 378).  A Tower of Vengence walking into range is none of its target's
  attackers (its attack done).  A structure's event due after a step and before
  that step's region triggers' timer comes between them (traced: the Tower of
  Vengence's reaction ending at 51.178535, after the step due 51.178459, its walk's
  objects before the step's trigger's `+tex`).  The upgrade game's trace
  (`abtest/mafv5.*`) now agrees to turn 529 (from 511), and its keepalives for all
  536 turns (from 512), with the tower's blocked count drawn at its walk's first step
  (below).
- An aimed missile lands in the behavior dispatch of the first main tick at or past
  its flight's due, so before a later main tick's step (`World::aimed_landings_before`,
  after that step's damage points, which land those due before them): traced, that
  shell's five deaths between the turn's second step's region trigger and its third
  step's.  A structure's damage point due after the agent queue's reach in its tick
  comes in the next, among its steps (traced: the second h00X's first, due 50.99565,
  in turn 511, where the units' logic had sent it at 510's end), as does its look
  (its due its time, though run a tick on).  Its try as its cooldown runs out is an
  agent queue event too, among the steps (`World::first_try`; traced, the third flyer
  game, `abtest/mafb3.*`: its Arrow Tower's attack on a creep past its reach given up
  at turn 344's start, its nine tasks' slots the step's order's; the port's had
  waited for the units' logic at the turn's end).  The third flyer game now agrees for
  all 358 turns of its keepalives (from 344), its trace to 352.
- An attack's swing begins as its cooldown runs out (`FUN_6f49a4f0`): read, the Arrow
  tower (cooldown 0.7, damage point 0.3, missile at 1800 to a target 384 away)
  sending one in turns 17, 24, 31, and so every seven, its objects freed 52 turns on.
  A structure's first on a target it took up begins 0.1 after the take-up (the attack
  task, `FUN_6f49e510`, starts the cooldown at DAT_6fcd53c4 for a unit that cannot
  move, `FUN_6f49d130(0.1, 0)`; one that can tries at once, event 0xd0196 to
  `FUN_6f499310`).  Read under winedbg on Mafarazzo TD's Plague Tower: `FUN_6f49d130`
  at the main clock's 0.215 (the take-up, its look due 0.2157 on the agent queue),
  then 0.315, 0.635, 0.955, and so every 0.32; the missiles' dues (`FUN_6f171870`) set
  at 0.614999, 0.935, 1.255 (each damage point's main tick, the agent's due 0.6157).
  So a missile's flight starts from the main clock of the main tick its damage point
  falls in (the last whose clock is short of the due, `World::main_tick_time`), the
  attack's timers running on the agent queue's dues: the first missile lands at
  0.8994, in turn 9, where one sent at 0.6157 would land in turn 10.  (For a unit
  ready since an earlier turn the port's swing still starts three main ticks into
  the turn, provisional.)
- A structure's attack is a stack of tasks on the unit (CUnit +0x174, each task's
  +0x24 the one below, `FUN_6f691e60`; run top first by `FUN_6f67df00`, each popped
  and destroyed, `FUN_6f691260`, unless it waits by clearing the unit's flag 1).  The
  take-up (`FUN_6f4943f0`, from the look `FUN_6f493ff0`) pushes them by the unit's
  state (+0x194; a block, `tskA` `FUN_6f6927a0`, sets it as it runs, event 0xd0162):
  for state 0 a block back to 0, task 0xd014b (`FUN_6f691f20`), the walk back to
  where it stood (`tsk.` 0xd016c, `FUN_6f692120`), a block to 7, tasks 0xd0148 and
  0xd016a, the walk into range (`tskt` 0xd016f, `FUN_6f6926b0`), task 0xd0163, the
  attack (`tskt` 0xd0168), a block to 1 (other states push fewer: from 1 only the
  tasks 0xd0148, 0xd016a and the two `tskt`).  The last block runs and goes at once;
  the attack waits.  With a target already, the old attack, its task 0xd0163, and its
  walk into range are popped (`FUN_6f497190`) and a new walk and attack pushed.  The
  task codes from their makers: `task` `FUN_6f684af0`, `tskA` `FUN_6f684b00`, `tskg`
  `FUN_6f684b10`, `tskO` `FUN_6f684b20`, `tk.2` `FUN_6f684b40`, `tsk.`
  `FUN_6f684b50`, `tk.t` `FUN_6f684b60`, `tskR` `FUN_6f684b70`, `tkt2`
  `FUN_6f684b80`, `tskt` `FUN_6f684b90`.  Traced: the Plague Tower's ten in turn 3
  and again in turn 119, the test towers' each in their fourth turn.
- A structure takes up only what it reaches: a candidate for a unit that cannot move
  (`FUN_6f66f030`, no move ability or its move disabled) must be within its weapon's
  range (`FUN_6f4959f0`), the search's radius the least of its acquisition range and
  its longest weapon's (`FUN_6f498e50`); and it gives a target up as its try finds it
  out of reach (`FUN_6f495180` → `FUN_6f49d280`; one that can move walks after it,
  `FUN_6f49a240`).  The reach (`FUN_6f05b580`): the range over 32 (its exponent less
  5) plus both units' position objects' +0x90 (their collisions over 32), squared,
  against the squared distance between their places carried on to the main clock then
  (its last argument set: `FUN_6f161040`, `FUN_6f05bdd0`), a thousandth over still
  within.  The giving up pops the attack, its task 0xd0163 (not run), and its walk
  into range (`FUN_6f497190`), then the rest run and go, top first: task 0xd016a
  (`FUN_6f49e6b0`, which waits only for a backswing still under way), task 0xd0148
  (`FUN_6f49ed20` → `FUN_6f49c3a0` → `FUN_6f49d000`: the look's timer restarted at
  DAT_6fcd53a4, 0.01), the blocks and walk back and task 0xd014b (`FUN_6f498ff0`).
  Traced: the Plague Tower's fourteenth try at 4.7957, the runner 33.54 cells off
  against its 31.25 and 2.25 (the runner's last step had it 33.42 off), its nine tasks
  freed in turn 48, and its next look due 4.8057, each half second on from there, the
  one at 11.8057 taking the runner up again in turn 119.  Task 0xd0163 when it runs
  (an attack done, as a target dies; `FUN_6f49ea80`) waits ReactionDelay times a
  real drawn on stream 9 (`FUN_6f6936a0(9)`, event 0xd01af to `FUN_6f49b3d0`), not
  yet modeled.  (Provisional: the port's tasks for structures only, the same take-up
  for a unit that walks not made; a structure's tasks at its death not freed.)
- An attack's damage is drawn at its damage point on the attack's own generator,
  stream 2 (`FUN_6f498960` → `FUN_6f695c70(out, 2, dice, sides, base)` →
  `FUN_6f1ec180`): up to 16 dice a draw each, `r × sides >> 32` and one, their sum with
  the dice and the base (more dice a single draw from a normal spread,
  `FUN_6f1d12e0`; fewer than one die logs "Bogus request to roll 0 dice" and makes
  the damage 0).  A missile's miss from below is drawn as it is sent, on stream 2 too
  (`FUN_6f6d48b0` with its third argument set: from a missile's launch,
  `FUN_6f6d3470`, when its info's +0x78 is set, as the damage point sets it for a
  missile, a splash, a bounce, and a line; from an artillery's, `FUN_6f6d2530`, at a
  unit): first the target's abilities' and the attacker's own misses
  (`FUN_6f6d6cc0` → `FUN_6f48cae0`, `FUN_6f6cd570` → `FUN_6f48c690`, not read), then
  a real under ChanceToMiss, then the attacker on a lower cliff level than its
  target (one flying, `FUN_6f4995d0`, never).  So each attack moves stream 2 on, and
  the next unit made draws its first look for a target from where it stands: read
  with Mafarazzo TD's towers made one after another against paused runners, the St
  George Bank's and the HSBC's hits (lives 2000, 1734.0, 1147.4, 866.7, 287.1; then
  1649.3, 1298.6, 597.2, 246.5 at the half seconds) and the Ice and Cutter towers'
  first looks (turns 115 and 165) as the original's once the port drew so; it had
  drawn the dice and the miss on the main generator.
- `+fgt` (parent `+fgm`, `FUN_6f675460`): a fogged attack's reveal of the attacker
  (`FUN_6f67fc70`, the attacker's +0xf0), FoggedAttackRevealRadius wide and as long as
  the most of FogFlashTime and the time given: at an attack's damage point its weapon's
  cooldown and 1 (`FUN_6f4988d0`, from `FUN_6f4946b0` by `FUN_6f67fde0`, only for a
  struck unit whose owner does not see the attacker), at a splash's landing none (the
  splash's players hit, `FUN_6f67feb0` from CMissileSplash vf47 `FUN_6f6d00e0`, with
  no test of their sight).  An existing one is moved and its timer restarted when the
  new time passes what it has left or the players are new (`FUN_6f244150`); it is
  destroyed as its timer ends, its attacker gone or not.  Traced: the Cannon tower's
  first landing (turn 74, freed in turn 134) and the Ice tower's (turn 123, with its
  `Bfro`, freed in turn 183), each 3 seconds after its last landing, the towers removed
  meanwhile; none for the Arrow tower's, which sends no splash.  The port makes the
  splashes' (`World::splash_revealed`; provisional: the players taken as the struck
  unit's owner), not yet the damage points'.
- A structure does not walk, whatever its speed, unless an uprooted ancient: a tower
  made from the Ancient Protector (speed 40) without its Root (Mafarazzo TD's Towers of
  Vengence) stays where it was built.  But it takes up what comes within its
  acquisition range, out of its weapons' reach: its move ability (+0x1ec,
  `FUN_6f66f030` → `FUN_6f5fb040`) makes it one that can move to its look's radius
  (`FUN_6f498e50`, from 0x6f498ec3) and to the reach test (`FUN_6f4959f0`, from
  0x6f495ab4, which then skips the weapon's reach).  Measured (the upgrade game,
  `abtest/mafv4.*`; breakpoints on the look, the take-up `FUN_6f4943f0`,
  `FUN_6f66f030`, the reaction wait `FUN_6f49ea80`, and the give-up `FUN_6f49d280`,
  47.85 to 48.75 seconds): the Mega Tower of Vengence (e002; acquisition 2000, its
  weapons 1500 and 700) looks in the main tick of clock 47.906887, its look due
  47.909035 on the agent queue (after the turn's first step, due 47.908592), takes up
  a creep 1875 off edge to edge, and starts its reaction wait at once (from
  0x6f49a6b8).  The other towers look each half second and take up none.  Its look's
  phase: its upgrade, due 34.897661 on the main clock, is done in main tick 6980,
  whose agent queue has already been brought to the next main tick's clock,
  34.899036 (`FUN_6f04c1a0`: the agent queue before the dispatches), and its look is
  restarted 0.01 on from that (`FUN_6f49d000`; then each half second).
- Such a structure's take-up is a cycle that ends in a give-up (`crate::tasks`,
  `World::reach_tried`, `World::reaction_ended`, `World::reach_walk_step`).  Its look
  is an agent queue event among the movement steps by its due
  (`World::structure_events_before`).  Its tasks are a structure's ten; the last block
  runs, and the attack task, for a unit whose move ability is not disabled, sends
  event 0xd0196 through the unit (`FUN_6f49e510` → `FUN_6f0561f0`, message
  0x40190065: the move ability's stop, `FUN_6f5fa7a0`) instead of starting the 0.1
  wait (`FUN_6f49d130`), and, leaving the unit's flag 1 set, is done at once, near or
  far; both go as the main tick ends.  (The attack's own 0xd0196, its try
  `FUN_6f499310` → `FUN_6f495180`, comes from the move ability as a walk arrives.)  Its reaction wait runs then: ReactionDelay (0.25) times
  a real drawn on stream 9 (`FUN_6f49ea80`, `FUN_6f6936a0`), from the look's due.  As
  it ends (event 0xd01af) its task goes as that main tick ends, and its walk into
  range (`tskt` 0xd016f, its range 1500) begins with a walk's `^clg ^clu l^pa`, the
  `^clg` let go at once.  From inside its footprint it is blocked from its first
  step: its count, 7 or 8, is drawn as that step searches, on the main generator
  (`FUN_6f167290` → `FUN_6f1689d0`, `FUN_6f1d62b0(2)`), not as the walk begins (from
  the keepalives: the upgrade game's tower's walk begun at 51.178535, after that main
  tick's step, due 51.178459; the original's keepalive at turn 513 is the port's
  state with one main draw fewer, the next step being in turn 513), and it walks in
  the walkers' order, ending in
  the step after its count runs out (CAbilityMove's event 0xd0198, `FUN_6f603110`):
  its task popped, a block and two tasks 0xd0144 pushed, all freed top first as the
  step ends, its path and cluster at the next step, and its look again 0.01 after the
  step's agent clock (`FUN_6f49d000`).  Measured (the upgrade game; the take-up, the
  reaction, `FUN_6f1689d0`, `FUN_6f1d62b0`, and the task pushes and pops, 47.85 to
  48.75 seconds): reaction ends due 48.028042 and 48.427876 (draws 0.476029 and
  0.717201), the walk begun at 48.026821 with the only main draw through 48.25 (a
  count of 7), its ends in the steps due 48.238579 and 48.658562, the eighth after
  each start, its looks due 48.248577 and 48.668560 taking the creep up again.  The
  The attack goes at once at every take-up traced (turns 480 to 506), the creep at
  last 1559, 1485, and about 1360 off center to center against the 1500 weapon, the
  tower's collision 144; no stop at `FUN_6f495180`, at its reach test
  `FUN_6f05b580` (from 0x6f4952e1), or at `FUN_6f49a240` from 49.7 to 50.13
  seconds.  The upgrade games now agree to turn 507 (from 480), the trace to 506.
- A structure's take-up's last block, run at once, goes as its main tick ends: freed
  by the next step's timed frees, or with the turn's releases after the steps
  (`World::free_as_tick_ends`; traced: the upgrade game's Arrow-kind tower's take-up
  at turn 503, its block's slot the next step's region trigger's `+tex`; the second
  flyer game's at turn 330, freed before the step's walk ends' objects; the flyer
  game's after its creep's death as the turn ends, freed in that turn).  Its first
  try (`FUN_6f49d130`'s timer, 0.1 after the look) is an agent queue event too, among
  the steps by its due (`World::first_try`): on a target out of reach it gives up
  there (`FUN_6f49d280`), its tasks freed as the main tick ends and its look again
  0.01 on (traced: the second flyer game's Arrow Tower, its take-up at turn 330 given
  up early in 331, the nine tasks' slots taken by the next step's order).  A look
  due after the agent queue's reach in its tick comes in the next, before its first
  step (traced: the upgrade game's take-up at turn 507's start, the port's at 506's
  end).
- A dead unit whose corpse decays (its type's deathType 2 or 3) has five tasks of its
  death's course kept: 0xd0155 and 0xd0154 go as its death has played (its type's
  death time), 0xd0158 DecayTime after that, its timer started then and so going
  after a timer started before and due with it (traced: the second flyer game's
  creep, killed at 27.381405 by the Arrow Tower, 0xd0158 freed in turn 324 after
  the killing missile's objects, both due 32.381405).  (Provisional: 0xd0159 and the
  removal, the bones' decay, not yet read.)
- RemoveUnit (`jass_RemoveUnit`: the unit's vf 0x84, then `FUN_6f694690(1, 1)`) clears
  its orders (`FUN_6f673610`): its current order's tasks (`FUN_6f673e80`), then a block
  made and destroyed and its task stack destroyed top first (`FUN_6f673fe0`), then its
  orders; then it pushes tasks 0xd015a, 0xd0164, 0xd0156, 0xd0178, 0xd0144 and a block
  and runs them.  Traced on the test map's towers and runners, removed 4.5 seconds
  after they were made: the tower's first block, its attack's nine tasks, then the
  removal's three last; the runner's pause order's six, then its own; then, all in the
  turn of the removal, the units' own objects (the port had them in the next turn), a
  buff laid on the runner since (the Ice tower's `Bfro`) after its AI's objects.  Its
  food is given back at once: a player state event on food used fires inside the
  removal, after its tasks (Mafarazzo TD's End_Round), its event data a `psld`
  (CPlayerStateLimitEventData, made by the registration as its comparison passes,
  CPlayerStateEventReg vf3 `FUN_6f27cd00` → `FUN_6f276bb0`, kept by it, `FUN_6f224310`;
  traced: never freed).
- PauseUnit (`jass_PauseUnit`) on an unpaused unit (`FUN_6f697770`: its flag 0x200000)
  counts it paused (+0x54, `FUN_6f688d30`) and the first issues order 0xd0005
  (`FUN_6f69bd80`, `FUN_6f680320`): an `ordt`, two blocks, seven tasks, and a block, the
  second, the last, and the three tasks before it freed with the turn's releases, the
  rest kept while it is paused (traced: the test runners, paused as made).  Its end
  (`FUN_6f6977c0` → `FUN_6f678590`) is not read: the port frees the rest then.
- A buff laid with its own object (CBuff) is destroyed the frame after its timer runs
  out, before what fell due with it on a timer started later (traced: the fort
  runner's Slow Poison `Bssd` and `Bssi`, renewed by the last missile's landing, freed
  in turn 105 before that missile's objects, its removal timer started after; the port
  keeps each laid buff's end, `Buff::ends_at`, from its landing).  (Provisional: for the
  buffs other than the poisons' and the frost attacks', and a buff's end as its unit
  dies.)
- A computer's unit hurt with no order and no attack runs (above, CAbilityMove's
  `FUN_6f5fa400`): a touch of nothing, as a poison's or a frost attack's before its
  damage, is no hurt (the runner's buffs made before its walk away).  Its run is the
  engine's walk (`World::flee_walk`): its move order's objects as a player's, the seven
  it lets go at once destroyed on the frame after the missile's landing that made it
  run (traced: the runner's first run's in turn 10, after the missile's `Atrg`; its
  second's, from its post, in turn 141 itself), then the `+ord` of the order 0xd0006
  queued after it
  (`FUN_6f693450`) in the freed `^clg`'s slot.  As that walk ends, the look runs in its
  step (`FUN_6f9bd6e0`, its `task` made): a unit keeping a guard post more than 50 off
  attack-moves back (`FUN_6f9c0b00`): `ordt`, the `+ord` of its own look, a block, a
  `tskO`, two tasks, a `tsk.`, four tasks, a block, `^clg` (freed at once), `^clu`, and
  `l^pa`; then the walk's order's tree, the look's task and `+ord`, the new order's
  last block and four tasks before it, the last first, and the old path go as the step
  ends (traced: turn 84; at the post, turn 139, the walk back's tree, the look's task
  and `+ord`, its path).  With these Mafarazzo TD's Plague Tower trace agrees with the
  original's, slots and all, to turn 113 (but two frees of map objects at turn 110).
- The walk back is a cluster's of one, as a script's walk to a point (above): its
  order's `^clg`, `^clu`, and `l^pa` a move's, its cluster's coarse way planned over
  the accelerator and its point picked 10 on, its own path's planned to that.  Read:
  Mafarazzo TD's fort runner (`abtest/mafqp.w3x`, its place and facing pinged each
  tenth of a second), walking back from (-3390.18, -124.73) at 8.4 seconds bent east
  of the straight line to its post at (-3392, 960), heading 86 degrees to (-3345.20,
  542.41) at 11.8, then 96; the port's went straight up x -3392.  The port so walks it
  (`World::cluster_walker`), each ping the original's to 0.001 through 12.6 seconds.
- A buff run out takes its speed's modifier off at its timer's moment, and a walker
  walks at its new speed from its next step (`Buff::ends_at`; `World::move_speed_at`;
  the position's top speed set by `SmartPosition::vf6_6f05c5c0` through
  `FUN_6f15ff40`): read, the runner's Slow Poison running out at 10.44, its next two
  steps before the 10.5 ping at its full 280 (the port's had waited for the turn's
  buff count, a turn late).
- A slow laid by a missile takes the walker's speed at once.  The buff's slow
  (`FUN_6f63d510`, from `CBuffSlowPoisonStackInfo::vf213_6f6236e0` for the stacking
  poison's strike) changes its move ability's factor (`FUN_6f5fb7c0`, the change
  added to +0x78), which then sets its position's top speed (`FUN_6f6005d0` →
  `SmartPosition::vf6_6f05c5c0` → `FUN_6f15ff40`): a velocity past the new top is
  held to it (`FUN_6f15f7e0` with nothing added: the place carried on to now,
  `FUN_6f1603d0`, the velocity scaled, `FUN_6f1606e0` → `FUN_6f15fc70`), by the main
  clock of the tick it lands in.  Measured (breakpoints on `FUN_6f15ff40` and
  `FUN_6f16c880`, Mafarazzo TD's runner, `abtest/mafq.w3x`): both at 12.603886 under
  the missile's landing (`FUN_6f6d4e30` → `FUN_6f6cd2a0` → `FUN_6f6cf940` →
  `CMissileSlowPoison::vf51_6f6227b0` → ... → `FUN_6f63d510`, from the projectile's
  `NIpse::CBhPoProjectile::vf18_6f173cf0`), the only such stops in 12.55 to 12.75.
  The port lands its missiles after the tick's steps, so it walks a slowed walker's
  steps after the landing again from there, when they only moved it on, and holds a
  walker slowed after its last step for its next (`World::slowed_among_steps`,
  `Mover::lower_top`, `UnitExtra::placed_late`).  Read: each of the runner's 151
  pings (`abtest/mafqp.w3x`) the original's to 0.001 (from 126); held by the port's
  landing's own moment, 12.60506, they were 0.15 off.
- A walker slowed from unslowed (its factor under 1 from 1) leaves its cluster: with
  its count at +0x80 0 (`FUN_6f5fcd50`, the count up with each such slow and down
  with each end, `FUN_6f5fc0e0`), its position's member is taken from its cluster
  into a new group of its own with the old one's goal (`FUN_6f058bc0` →
  `FUN_6f16e7c0` → `FUN_6f1694f0` → `FUN_6f169550`: `FUN_6f16dd70`, `FUN_6f058430`,
  `FUN_6f169620`, `FUN_6f16db30`), started as a walk's (`FUN_6f16bcf0`): its cluster
  linked at the head (`FUN_6f16bdb0` → `FUN_6f14ef80` → `FUN_6f16c880`), the group
  freed, then the old path and the old cluster, their ways planned anew as its next
  step begins (`World::leaves_cluster`).  This is the walk back's new `^clg`, `^clu`,
  and `l^pa` at turn 127 after the strike's `Bssi` and `Bssd`, the `^clg`, the old
  `l^pa`, and the old `^clu` freed; the runner's first slow, at turn 9, found it
  standing at its post, in no cluster.  Provisional: a group's other members are not
  read.  With these the Plague Tower's trace agrees to turn 138 (from 126; to 140
  with its damage points among the steps, below).
- A structure's damage point falls among the tick's movement steps by its due.  It is
  a timer on the agent queue (CAbilityAttack +0x1ec, event 0xd01b1 → `FUN_6f49a5b0` →
  `FUN_6f4946b0`, started with the attack by `FUN_6f49d130`), as the steps are, and
  each main tick moves the queues on in turn (the handler at 0x6f36aba0 →
  `FUN_6f04c1a0`, by the tick's time at its event's +0x14): the agent queue
  (`DAT_6fd3c82c` + 0x40), then the Ipse world's relation dispatch (`DAT_6fd53a48` +
  0x14, its clock at + 0x54 the main clock), its behavior dispatch (+ 0xbc, its clock
  at + 0xfc), and its maps' optimizer (+ 0x164), each running its requests due by
  then in due order (`FUN_6f0522e0`, `FUN_6f052330`, the clock reading each one's due
  as it runs; the local queues likewise, `FUN_6f04c0d0`).  Read: the Plague Tower's
  damage point at 13.804, between the turn's first step (due 13.7999) and its second
  (13.8299), its missile's objects made before its runner's walk ended in the third
  (13.8599); the port's had come after the turn's steps.  The port so takes a
  structure's damage point due before a step as that step begins, in due order, its
  missile's target seen as it is first flown, after the steps (`World::points_before`;
  provisional: only for a swing begun before the tick, not the other units', and the
  missiles landing before it flown among the steps, `World::fly_bullets`, though the
  port's flights are flown after them).  A projectile's flight is a request on the
  behavior dispatch (`FUN_6f171870`: its due that queue's clock plus the flight's time,
  `FUN_6f159d50`, `FUN_6f171cc0`), so it lands after the main tick's agent events.
  With this the Plague Tower's trace agrees to turn 140; at turn 141 the original frees
  the `Atrg` of the missile sent at 13.804 in that turn, the port, landing it at
  14.0965 on the main clock, in the next.
- A structure's look finds its candidates by the proximity map's radius search
  (`FUN_6f493d20` → `FUN_6f67e790` with case 10 → `FUN_6f059c10`, `FUN_6f05ce60`):
  the radius the least of its acquisition range and its weapon's (`FUN_6f498e50`)
  plus its own collision (`FUN_6f05aa20`, the position's +0x90 times 32), each
  candidate's place its position's +0x78 and +0x7c, as its last step set it, not
  carried on, within that and its own collision; its reach after
  (`FUN_6f4959f0`, `FUN_6f05b580`) carries both on.  Read: the Plague Tower's look
  due 11.3057, the runner's place at its step at 11.279 1075.5 off against 1000, 72,
  and 1 (carried on to 11.3057 it was 1068.2 off, in reach), so not taken up until
  the look due 11.8057, in turn 119.  The port so judges a walker for an idle
  structure's look (`World::find_target_inner`; provisional: by its place at the
  step before the look carried back along its velocity, for units the old test), and
  the Plague Tower's trace agrees to turn 126 (to 138 with the slow's cluster, above).
- The look's timer (event 0xd01c1, `CAbilityAttack::vf3_6f49a5f0` → `FUN_6f493ff0`)
  starts itself again as it falls due, before its search: a second on with a target
  held (`DAT_6fd6bf44`), else half a second (`DAT_6fd6bf40`).  So a structure's look
  that takes up a target looks again half a second on, then each second, swinging
  or not (the port's had waited for the swing to end).  Its search begins with the
  target held (`FUN_6f493d20`: its score `FUN_6f49d680`, which gives the target held
  in reach 0x1000000 over its in-reach 0x4000000), a find taking its place only by
  a higher score, or an equal one and nearer (`FUN_6f493b20`), and none returned
  when the best is the target held; the port so keeps its target unless a find of a
  better rank (one going for the structure, a unit over a structure) comes
  (`World::structure_look`; provisional: the score's other parts, the damage
  factor's and the life's bands, are not the port's).  Measured (the flyer game,
  `abtest/mafb.w3x`, `FUN_6f493ff0`'s entry, its return 0x6f49a7c2): the Arrow
  Tower's looks at 21.662 (taking up a creep), 22.162 (its +0x20 flags 0x24084115,
  the target held), and on; the original's went on with that creep (its missiles at
  turns 228 and 235 with no new tasks), the port's had turned on a nearer one at 22.2.
- An armed structure looks for a target as it is done: its look's timer started
  0.01 on (`FUN_6f49d000`, `DAT_6fcd53a4`) from the agent queue's clock as the
  construction's end falls due on the main clock (the clock of the first main tick
  at or past the end), its making's wait forgotten (`World::look_when_built`).
  Measured: the flyer game's Arrow Tower, done at 19.147955, its first look in the
  main tick 19.157785 (its flags 0x84010), then 19.662701, 20.162617, and on each
  half second, its dues so 19.162787 and on; the port's had kept its making's phase
  and looked at 19.4, 19.9, and on, taking the creep up at turn 219, not 217.  Which
  event starts it there (`FUN_6f49d000` is reached from 0xd0148 and 0xd0149 through
  `FUN_6f49c3a0`, and from 0xd014f) is not read.  With these the flyer game agrees to
  turn 274 (from 217).

## Where the port departs from the original

Formats the port made its own, to be replaced by the original's:

- Saved games: the original's .w3z alone, which the port writes (`Game::save_files`)
  and loads (`Game::restore_original`), with nothing of its own beside it; what its
  writer and its load do not yet cover is below.  The original's (`FUN_6f1e8630`, its SaveGame native `FUN_6f211360`; read with a
  1.27b save made by a test map's script at 5 seconds, `Save\Profile2\CustomSaves`,
  and 1.22 ones): the replay container, its record a data store (bytes, words,
  dwords, NUL-ended strings) holding the map's path, the network game's name (empty
  alone), its GAMEINFO's name, password, coded description, and two words
  (`FUN_6f968cf0`), the W3GS slot table (word length first), a word of the objects
  (`FUN_6f324930`), the map's xoro and SHA-1 (as a LAN game's description has them),
  the local player's slot index and number, its name, two preferences (6 and 0x38,
  `FUN_6f0425d0`); then the object manager (`FUN_6f04dd30`), the game's objects (its
  vf 0x10), and each subsystem's state.  Its object table is every tagged object, 16
  bytes each (its two codes, byte-reversed, and two tags): 12403 at 5 seconds of
  Booty Bay.  After the header: the game time (ms) and a word (`FUN_6f32fa00`), then
  the object manager (`FUN_6f04dd30`): five size-prefixed sections (`FUN_6f15b3e0`: a
  116-byte header, the two tag tables, their free lists), the 1077 classes it
  registers (a word count, then code, parent code, and two tags each), the main and
  agent clocks (time, a word, the wrap 300, two words), 256 bytes, a word, and six
  size-prefixed sections (`FUN_6f15b4f0`: regions `FUN_6f15c750`, pathing
  `FUN_6f167500`, then each table's objects' first records `FUN_6f15c8b0`, vf 0x24, and
  second `FUN_6f15c930`, vf 0x2c: each record size-prefixed and ended by "espi", one
  per entry in order), two words, and the map's bounds.  The regions section
  (`w3_formats::w3z::PathingMaps`, written back byte for byte, and now written by the
  port from its own state, all 5499548 bytes of the quiet test map's agreeing but for
  the two search marks, the running game's) is the pathing maps:
  references to two `^pma` and four `^ama` (the proximity maps and the grids), the two
  proximity maps (`FUN_6f14d620`: a frame, its size in cells, bounds, and cell scale,
  `FUN_6f14d590`; three words; and each cell with flags or objects, in rows, its
  column, row, flags in the top byte, and the objects in it, `FUN_6f14d4c0`), the four
  grids (`FUN_6f163120`: the frame and a word for each cell, at 2, 4, 8, and 16 times
  the pathing cell), and references to an `l^sy` and an `l^ac`.  On Booty Bay the
  first proximity map (99 by 51, 8 cells each) holds nothing, the second is the
  pathing map (768 by 384, every cell written): its flags are the map's own (.wpm)
  with 0x01 set, 0x10 where 0x02 is (not walkable), and 0x20 where blighted (5824
  cells by an undead player's start at 5 seconds); its objects are only `^pmr`
  (a pathing footprint's region, listed in each cell it covers: 71616 at 5 seconds).
  A cell lists its regions newest first (23236 of 23300 cells with more than one).
  A region's record (`FUN_6f14d430`) is its map and owner, a word (+0x34: its kind in
  the top byte, the pathing flags it sets below, e.g., 0xc2, 0x10, and 0x08 for a
  tree's three), its bounds (+0x1c to +0x28: first row, first column, and one past
  the last of each, a row and column beyond the cells it lists: a tree's 16 within
  5 by 5), the map's search mark when last met (+0x38, below the map's own, its
  first word), the cells listing it (+0x3c, in its low bits), and a word (+0x40).
  On the pathing map 4254 regions are listed in cells and 1491 of kind 1 in none;
  those on the first map (1597) and 106 of kind 0 are listed nowhere.
- Forestwalk ((3)Forestwalk.w3m, read against the original's save at its first timer,
  `fws.w3x`, and its first network keepalive): a tree placed dead (life 0 in
  war3map.doo, its "D" model) takes its `` `pos `` and that's two `^pmr` but not its
  footprint's three, and blocks no pathing; at the first dawn (MeleeStartingVisibility
  setting 8:00) a melded creep's meld ends (Shadowmeld's day event, `FUN_6f508ae0`,
  `FUN_6f509170`, `FUN_6f694b20`: its fade-in's `` `rcm `` and `` `rtl `` after the
  unit's own `` `rcm `` of the change, both freed DataB, 2.5 seconds, on), and a
  sleeping flyer waking makes its detection's in-range object again (`Abdt`'s
  `` `pil ``, `FUN_6f40bb00`, `FUN_6f3f8640`, after its spells' listeners; the old freed
  as the next turn starts).  With these the port's births agree with the original's
  first keepalive (0x73d5) in a game of two people.  The main generator's draws there
  (traced at `FUN_6f1b7130` in the network game): a random race for each player but
  one the map has whose slot is closed (11 with Forestwalk's third closed), three for
  the start locations, then each wanderer's first wait as it is made (Awan, from
  0x6f5aa925 by `FUN_6f9a9d90`: the thirteen deer, each within its making), and again
  for a wanderer removed (Blizzard.j's MeleeClearExcessUnits taking each start's
  deer, the draw before its removal's last `tskA`).  The units a range search finds
  come in the original's spatial order (each start's deer, made last, removed first:
  by 256-unit blocks, each block's latest first, as its trees).  With these the
  keepalives agree to turn 6.  At 0.5 seconds the original then makes 48 objects to
  the port's 15: each Unholy Aura buff (`BUau`) with a timer (`` `rcm ``, from
  `FUN_6f4bb8c0`, `FUN_6f41def0`), and 15 `Cdet` from the flyers' burrow detection
  (`Abdt`, CAbilityBurrowDetector of the CAbilityDetector family: its in-range object,
  event 0xd01b4, makes a detection, `FUN_6f407060`, `FUN_6f688990`, for each unit
  come in range whose +0x164 list it is not yet in).  Read further (`detfw.sh`): the
  detectors' first look is at 0.5 seconds, each finding every unit within its Rng
  (900) but itself, the dragons first, then the whelps; a unit gets one record a
  player (18: fifteen whelps and three dragons; the births between 0.5 and 0.6 are
  15 `BUau`, 15 `` `rcm ``, and 18 `Cdet`).  The port does so (`update_detection`,
  each detector's clock started at its first look, as an aura's): its births agree
  at turn 7; the free list's order then differs.  Read against the original's frees
  from the load on (`freesfw.sh`, 1650 of them): a whelp made asleep frees its
  detection's `` `pil `` with its sleep's releases (after its autocast's order, before
  its spells' timers), and makes it anew as it wakes; the aura scans and detection
  looks are one queue, in the order their timers were made (a bearer's with it, a
  detector's with its `` `pil ``); and a creep RemoveUnit takes frees, after its fourth
  task, its abilities' in-range objects (an aura's `` `pil ``), its AI objects (`uref`,
  `AI-U`), its buffs' objects (each its timer, then itself), the spells' listeners it
  made waking (in the order made), a meld's fade-in not yet over, its regeneration
  timers then its listener, the third task, then its abilities' other objects, the
  last first, those it released as it fell asleep left out.  The order units are
  removed in is the group's: a range search (`FUN_6f22f0a0`, `FUN_6f04c5a0`) finds
  units by the 256-unit blocks from the map's corner that the corner of each's
  collision box falls in, row by row, each block's latest made first (its callback
  `FUN_6f233f20` traced at both starts); a unit added to a group (`FUN_6f28d7c0`) goes
  before the first there of its type in a later slot, else last; and ForGroup walks a
  copy of the group's list (`FUN_6f22ed70`, `FUN_6f2939f0`).  With these the first
  1601 frees agree, and the keepalives to turn 482.  There a deer walked within a
  Neutral Hostile Hellcaller's Unholy Aura (`friend`): the original gives it nothing,
  Neutral Passive's units counting as neither friend nor enemy to another's aura that
  does not name `neutral`.  With that, all 530 turns of the capture agree
  (`docs/reference/forestwalk_keepalives.txt`).
- A destructable's records (`+w3d`, 1408 trees of Booty Bay at 5 seconds, read against
  its war3map.doo): its first the agent's (its class's `^adf`, its `` `pos ``); its
  second its own (a word, 6 or 0x106, then life, 0, and maximum), handlers (one
  fixed, the same for all), the widget (its sprite: model by variation, the doodad's
  scale as its first real; owner 0; its type; two shorts; three of its five `^pmr`,
  the footprint's, and its position; 230 as its last real), then its variation
  and two bytes, and three words: its z (as a real), -2, and 1.0.  What varies
  besides: the sprite's color (0x202030 for most, 0xffffff for 19 in the local
  player's sight, 0x404060 and 0x2e2e46 for three), a sprite word (0x38c0025,
  0x3ac0025, 0x3a40025) and short (-2 or -1), the widget's shorts (4095 or 4094,
  32769 or 0); and for 101 trees by the east start the own word 0x106, its first
  short 4094, and its first byte 0, in every save of the map: not yet understood.
  The own word is CDestructable +0x20 (`FUN_6f6c39b0`; after the handlers and widget,
  `FUN_6f6c3940` writes +0x54 the variation, +0x55, +0x80 the z, +0x84, and +0x5c).
  Its 0x100 is set by `FUN_6f6c3b30` (its vf 0x98 called first, then
  `FUN_6f6c44a0(0)`), which a chop calls (`FUN_6f405110`), `FUN_6f48ffc0`, and the
  destructable's making (`FUN_6f6c0d90`, where `FUN_6f04e060(0x20, 0)` is not 0; else
  `FUN_6f6c44a0(1)`).
  The classes registered and their definitions (`^adf`, the second table's first
  1077) are alike in every save (`docs/reference/w3z_classes.txt`).  The second table
  holds the local player's own objects besides (the camera `+cam` and its reals, the
  cursor `+ccu`, `+ccp`, effect images `+EIm`, `rlyp`, fog ghosts `+wgh` with their
  positions and regions): a save of the quiet test map with them left out crashes
  the original as it loads (`w3zedit drop1`), the same save written back unchanged
  loading and playing.  But all of them except the camera's may go: with the fog
  ghosts (103, each with a `` `pos `` and two `^pmr`), effect images, the cursor's,
  the rally indicators, and the flag listeners left out (`w3zedit dropkinds`; 1096
  objects left: the definitions and the camera's 19), the original loads the save and
  plays on, the main table's references to them dangling; their slots put on the
  second table's free list as tags (high bit set) crash it.  The camera (`+cam`, slot
  1077, which the game state's second part names): an agent with fourteen
  properties, eleven `` `rel `` camera fields (each value, a rate, its least and most:
  the distance 1700 within 100 and 10000 first), two positions with their regions,
  and one `` `rel `` more, its second record its matrices, its target (the start
  location) and eye, and the camera bounds.  The port writes saves from its own state piece by piece
  (`w3_game::w3zsave`, checked by playtest's `w3zcompare` against the original's save
  of the same moment): so far the first table, its free list, the classes, and their
  definitions, each byte for byte on the quiet test map at 5 seconds; then the
  destructables' positions and regions (8448 records, the regions' search marks set
  aside: the running game's, written 0): a tree's position its cell as reals, its
  type's fixed rotation in radians (fixedRot 270 saved 0x4096cbe3, which 270 times
  0.01745329 gives; war3map.doo's angle, 0x4096cbe5, is not what it keeps), its two
  regions; the regions as noted above.  The original loads the save so made (with
  the object manager's header from the port as well) and plays.  The trees' own
  records (`+w3d`) agree too, all 1408, 357 but for what the local player sees of them
  (the sprite's tint, word, and short, the widget's shorts and first byte, the own
  word's 0x100): written with the values most trees have, the original loads the
  save and draws the trees as before.
  The triggers' objects agree as well (238: `+trg`, its two flags, `+tac`, `hdlr`):
  a trigger's agent names its two flags, the second linked; its own (CTriggerWar3,
  `FUN_6f2998a0`) its word +0x20, evaluations +0x58, executions +0x5c, +0x60, +0x74
  (1 for the trigger that ran, provisional as its executions), and whether it waits
  on sleeps +0x78; then its events', conditions', and actions' lists.  Its flags are
  2, then 1 (enabled; which flag is enabled is provisional, every trigger read being
  so).  Registrations of timers' expiry, dialogs, their buttons, and trackables are
  not on their triggers' lists: their natives make them without `FUN_6f296670`, which
  every other registration native calls.  An action names its function's handler; a
  handler (`hdlr`) saves its function's number, the count of functions given a
  handler till it (timers' own handlers counted too: read, 1 to 46 by birth) and
  holds the game's registration of event 0x80267, which a timer's own handler lacks.
  Every object outside its family's own records (agents, the trees' widgets) holds
  that registration, it seems: the trees and handlers read so.
  The event registrations agree too, with the filters and locations (83): each
  starts its own with four words (0, -1, -1, 0), then its kind's: the engine's event
  id, the script's plus 0x80200 (the natives add it: `FUN_6f217cf0`); the unit or
  player it names by slot and birth (`FUN_6f27aa90`: the agent's +0x14 and +0x18);
  its filter; and the list of its trigger with the event.  The players' objects are
  the first made, 40 slots each from 0 (Neutral Passive's at 600).
  TriggerRegisterPlayerEvent for an alliance change makes an alliance's registration
  (`alvt`, by `FUN_6f217510` for ALLIANCE_PASSIVE), its player, alliance type, and
  flag listener kept.  A time-of-day registration keeps its op (0x80101 on), value,
  state, and its listener, one for equal in its first place while the time is past
  its value, its second while short of it.
  The players' objects agree too (644: each player's first record, flags,
  alliances, and flag listeners).  A player's 26 flags hold its states by number,
  gold and lumber in tenths (500 gold saved 5000), the upkeep rates as derived from
  food used (Neutral Hostile's 60 at 97 food); its alliances' 11 its number, then
  each alliance type's mask of the players it is set toward.  A player is allied to
  itself in types 0 to 7 only, not rescuable nor forced shared vision.  Flag
  listeners (`` `fcl ``) chain twice, by owner and by flag, newest first: a node's
  first link names the one made after it, its second the one before, the owner's
  agent's fourth link or the flag's second the newest (each player's own on its food
  used, word 6; an alliance registration's, owned by the alliances, on the type's
  flag, word 0x80268; a player state registration's, owned by the player, on the
  state's flag, word 0; then the local player's resource bar's six in the second
  table).  The alliances hold no game registration: their handlers are each alliance
  registration's on event 0x80268, keyed by its listener's slot.
  The forces agree as well (21, with their flags): a force's flags are its players'
  mask (+0x24) and the count its last enumeration added (+0x40); its three references
  its last ForForce's function's handler (+0x34, `FUN_6f22fe30`), its last
  enumeration's filter (+0x38), and a player.  An enumeration (`FUN_6f22ef20`, for
  ForceEnumPlayers, its Counted, Allies, and Enemies kin) empties the force, then
  adds each of players 0 to 11 whose slot is not empty (+0x270, GetPlayerSlotState's)
  if the filter passes it and then the kind's test, till the count reaches the limit:
  Blizzard.j's `bj_FORCE_ALL_PLAYERS` holds only the players in the game (mask 3 in a
  two-player game, its count 2).
- Agent timers (a `t` field: CAgentTimer, `FUN_6f060ba0`) save their node
  (`FUN_6f060520`): its due and period, the count it took as it was started, its event,
  and whether periodic.  The count is the timer queue's (`[0x6fd3c82c]`, picked by
  group; its +0x50), taken by every start (`FUN_6f0602a0` and its kin, at 0x6f060210,
  0x6f06034d, 0x6f06039c, 0x6f060458): 698 in the quiet test map's first 5 seconds,
  by about 30 callers (`docs/reference/bbq_timer_starts.txt`).  At the start: six by
  `FUN_6f9c7b20` (three each, twice), three by `FUN_6f41c010`, eight pairs by
  `FUN_6f41baf0` and `FUN_6f41ae70`, then five for each creep (`FUN_6f49d220`,
  `FUN_6f4993b0`, `FUN_6f524e00`, `FUN_6f517be0`, `FUN_6f4e36d0`), the wanderers' waits
  (`FUN_6f5aa910`, 14), the script's four timers (224 to 227: Blizzard.j's 120-second
  one, the map's two), then the players' units'.  Thereafter the most by
  `FUN_6f493ff0`, re-armed as its timer fires (335 from 0.025 seconds).  A periodic
  timer keeps its count as it repeats (creep sleep's 30 to 208 still at 5 seconds, due
  6.0 every 2.0).  The port counts them (`Tags::start_timer`, logged with W3_TIMERLOG,
  playtest's `timerlog`), named by the function starting each; the first 286 of the
  quiet test map's agree:
  - Each `AI-1` three as it is made (`FUN_6f9c7b20`).
  - A unit's making, in its objects' order (with a census of every unit type made for
    Neutral Hostile at night): blight regeneration's (CAbilityBlightRegen vf33's
    `FUN_6f436780`); a sold unit's restock each, at most twelve (CAbilitySellUnit
    vf33); its attack's, a creep's first (CAbilityAttack vf57's `FUN_6f49d220`), then
    its first look for a target (vf194); an aura's (CAbilityAura vf57); a buff's, an
    aura's on its bearer or a morph's (CBuff vf211); blight dispel's or growth's
    (Abds, Abdl, Abgl: `FUN_6f491250`); fire's (CAbilityOnFire vf33); creep sleep's
    check and, falling asleep, its sleep's (vf32); the alarm's (CAbilityAlarm vf32);
    then, for an armed creep that does not fall asleep, two more of its attack's
    (`FUN_6f49e880`).  Neutral Passive's structures (Avul, no blight ability) have no
    blight start.
  - A player's tech tree change (a unit made, `FUN_6f227b40`, and production's start,
    finish, and cancel) tells the player's listeners (event 0x80261, `FUN_6f260550`),
    among them Command, War Drums, and Trueshot Aura's buffs (CBuffAuraCommand,
    CBuffAuraKotoBeast, CBuffAuraTrueshot vf3, `FUN_6f4e36d0`), which start a timer
    each.  A unit made registers in the tech tree after its abilities: the port's
    `tech_add` follows `tag_unit` now.
  - A wanderer's each wait (CAbilityWander's `FUN_6f5aa910`), the script's TimerStart
    (`FUN_6f249ca0`).
  - Waking with the day: each auto-target spell (Heal, Inner Fire, Repair,
    Restoration, Renew, Replenish: CAbilityAutoTargetSpell vf267), the type's autocast
    ability (vf270), then the attack's, the last made first.
  - A unit removed: its animation's (its orders cleared, `FUN_6f694690` to
    `FUN_6f69c620`; the gold mine the undead melee setup replaces).
  - A unit's AI joining (`FUN_6f9b9230`, a computer's units and creeps awake): each
    auto-target spell's and Cannibalize's (vf50).
  - The game's two periodic timers as its setup ends (events 0x80269, 0x8026a).
  - The auto-target spells' checks, at the step after their timers start and every 3
    seconds (`FUN_6f42f8b0`: the quiet test map's 17, its 11 vf267 and 6 vf270).
  - A buff laid (CBuff vf211), and at its aura bearer's sweep every 2 seconds each
    buff it keeps (eight at 2.0 and 4.0 seconds).
  With these the first 354 agree in order, and in each half second every kind but one
  in number.  That one: each armed unit's look for a target (`FUN_6f493ff0`), re-armed
  half a second on (DAT_6fd6bf40; a second, DAT_6fd6bf44, by its flags), a worker's
  look starting no timer (37 a half second of the 45 looks).  The original's looks
  each fall once a half second, the creeps' first within the first half second after
  they wake.  The first look's wait is a draw on stream 2 times DAT_6fd6bf40, 0.5
  (`FUN_6f6936a0` from `FUN_6f4993b0`; DAT_6fd6bf44's 1.0 only for a unit with a
  target), not up to a second; an autocast toggle (heal off, say), or an order to
  harvest gold a unit without Ahar cannot take, does not end the wait (the port's
  `order_unit` had, its casters and the computer's ghoul looking twice a half
  second).  With these every kind agrees in number in each half second through 4.5
  seconds, and the first 478 in order; after, starts in one port tick come in the
  port's order (its units' looks, then the auras' sweep), not by their dues.
- The script's timers agree in the save too (10, with the counts their starts took):
  CTimerWar3's own (a word, its timeout, a real); its CAgentTimerExtended
  (`FUN_6f0602a0`): while running itself, its flags, and its node (due, period,
  count, event 0x80204, a periodic node); then two shorts and a real.  A timeout of
  120 seconds or more runs in chunks of DAT_6fd3c894 (set to 120 at load by
  `FUN_6f070d80`): a periodic node of 120, flag 0x10, the count of chunks in the
  shorts, the timeout beyond them in the real (Blizzard.j's stock timer, 120 seconds
  one-shot, so saved: flag 16, node periodic, shorts 1 and 1, real 0); a shorter one
  a plain node, its shorts 0, its timeout in the real.
- Properties' and listeners' timing blocks (an object base's two `Timing`s, the
  `FUN_6f15de50` node: due, period, count, word, periodic) count by the property
  module's own counter (its +0x50, taken at 0x6f15d735, 0x6f15dc58, 0x6f15e127): one
  sequence, 3 and on at the quiet test map's start, taken as listeners and modifiers
  are made (`FUN_6f058090` and `FUN_6f062180`, `FUN_6f0616e0`, `FUN_6f062c80`,
  `FUN_6f15ecb0`) and twice as each unit's tags are made (`FUN_6f68a060`'s at
  0x6f15e127): a shop and a fountain seven, a gold mine four; a Gnoll's life
  regeneration's `` `rcm `` (due 150, period 150, count 41).  They follow the objects
  made: a modifier `` `rcm `` takes one (`FUN_6f1613d0`), a threshold listener `` `rtl ``
  two (`FUN_6f058090`, `FUN_6f0616e0`), an in-range listener `` `pil `` three
  (`FUN_6f15ecb0`, `FUN_6f062c80`, `FUN_6f0616e0`, its node the third's), an order or
  task one (a unit's making's two tasks, its AI's nine, its autocast's order), a flag
  listener none; a creep falling asleep as it is made lets each spell's listener go
  before its making's tasks, a count each (`FUN_6f48c8c0`).  The game's own `` `rtl ``
  takes 1 and 2.  The port counts so (`Tags::prop_count`), each object's kept: the
  quiet test map's agree through its 373rd (the shop's and fountain's `` `pil `` 7 and
  14, a Gnoll's `` `rcm `` 41).
- A unit's five reals (`` `rel ``, listed after its `` `pcu ``, its properties) agree in
  the save (351 of 355): life, mana, two of -0 (bounded 0 to 2048, and 0 to 1), and
  sight, each its value, rate, and bounds.  Life's heads are its regeneration's
  `` `rcm `` (or an undead's on blight, made at its check and stamped then) and its
  death's `` `rtl `` (0.4); mana's its `` `rcm `` and its newest listener on mana (a
  woken creep's made anew, else its spells' own, a peasant's Repair's).  Regenerating
  mana is kept as its value at its stamp (the creeps' 0 at the start), mana without a
  pool as -0.  Sight is stamped with the end of its last day change's 3-second ramp
  (3.0 for those made before the melee setup's 8:00), a sight of none never changed
  -0 (the haunted mine's hidden gold mine).  Not yet: a Granite Golem's rates, 1.5 in
  its data, held as 1.5000001.
- A unit's threshold listeners (`` `rtl ``) agree but for the running game's counts
  (88 of 107 whole, the rest their re-armed nodes' counts): two words (the event,
  0x20090400), the threshold, and whether rising; its death's on its life (0xd019f,
  below 0.4); its spells' on its mana as made (0xd01b7, below -0.1) or as a creep
  woke (0xd01b6, rising past each cost and 0.1, unless its mana is past the cost
  itself: a Trapper's spell of no cost waits for it to fall, the port's wake having
  compared with the cost and 0.1).  A rising one's node checks again at most 4
  seconds on or as the mana reaches it: due the time it reaches, reckoned from 0 in
  single precision (5.1 at 0.75: 6.7999997), its period the time left at its last
  check (2.1 / 0.75: 2.8).  The unit chains all its listeners, its in-range ones
  (`` `pil ``) among them (its abilities', a flyer's, its AI's item pickup, Ensnare's,
  and a computer's structure's alarm's as it starts to watch), newest first; each
  real its own.
- A unit's agent record (`+w3u`'s first) agrees for 65 of 71: its properties (its
  position and five reals), the last its second link; its first link the newest on
  its abilities' chain (its abilities, an AI's unit list `uref`, buffs laid on it,
  abilities the script added: a town hall's Call to Arms from the melee setup; not
  the AI itself), its third its newest modifier, its fourth its newest listener.
  Not yet: which unit each of an aura's buffs is laid on at a scan, which follows the
  aura's in-range listener's own list (read, 2 seconds in: neither the port's order
  of making nor its search of the map).
- A unit's own record (`+w3u`, CUnit's `FUN_6f696490`): its widget, its event lists
  (0x80234 to 0x80258, 0x80319 to 0x80329), then 109 fields from CUnit's +0x130 on
  (words, references, a short, timers).  Across the quiet test map's 71 units 79 are
  constant (-1, -1, 0 triples; zeros; 0x0f12 at field 50; 1.0 at 102); those that
  vary, as read: 8 a reference (most units'); 9 a short (3 to 5); 20 and 21 a slot
  and birth of its last ability before its making's tasks (its creep sleep, a
  structure's alarm); 38 and 39 its unit AI's (`AI-U`) or -1; 41 its death's
  `` `rtl ``; 42 its life's rate; 43 its life's `` `rcm ``; 44 its mana's rate; 45 its
  mana's `` `rcm ``; 46 a real (0, 1, 5); 48 its owner; 49 flags (517, 0x80201, 513);
  51, 67 a word of 0 or 1; 72 to 74 flags; 80 to 83 reals (60, 120, 80; 0 or -20; 0 or
  60; 60, 40, 220); 92 its tint (ARGB); 100 and 101 its move speed and its type's (as
  single precision bits: 270, 200, 250); 104 to 106, 108 flags.
- A unit's own record (`+w3u`'s second) as the port writes it agrees with the quiet
  test map's for every part but what the local player sees (its sprite's animation
  choice and time, flags, and fog tint, its widget's shorts), four keyed tables, and
  the golems' 1.5000001s; the original loads and plays it.  Read:
  - Its leading word is the agent's flags (+0x20, `FUN_6f03edf0`): 0x600, shown 0x2 or
    hidden 0x1, 0x4, a unit 0x40, invulnerable 0x8, a worker 0x100000, it trains (a
    queue) 0x200000, it harvests gold 0x8000, a haunted mine's 0x10000, lumber
    0x40000; and 0x1000 for a unit whose collision is an odd count of 16 (16, 31, 48
    have it, 32 and 47 not, on the quiet test map and Forestwalk; provisional).
  - Its handlers: the agent's registry (`FUN_6f0725b0` appends; one already there for
    an event keeps its place) walked by event (`FUN_6f0564e0`), so each event's list is
    its listeners in the order they first listened.  Traced as made: the unit itself
    (18 events, a structure one more after its abilities), each ability in its order
    with its kind's set, then its AIs (a creep's camp's and Neutral Hostile's; a
    computer's; a gold mine's town site and each computer's, a computer's town hall
    its site's after its own), the game's.  A creep woken from sleep makes its
    abilities with orders listen anew, the last made first (all but 0x166); a buff
    listens on its bearer (Devotion Aura's not for 0x1a2 on another) and its aura's
    bearer.  The keyed lists (each listener's observers) are a hash of four buckets
    by slot, each in the order made (all but the computer's structures, whose alarm
    comes first, and two golems' emptied listeners left in place).
  - Its triggers' events (lists 0x80234 to 0x80258, 0x80319 to 0x80329): the uevts.
  - Its fields (CUnit offsets in `FUN_6f696490`): its day change's object (+0x110,
    kept after it goes); its sight's radius in fog cells of 128, rounded, a computer's
    or neutral's no farther than its acquisition range (`FUN_6f24f7e0` of
    `FUN_6f699ef0`); its newest ability or buff; its AI's; its death's listener, life
    and mana rates and modifiers (on blight, its blight's); its armor with its buffs';
    owner; flags (+0x5c, from CUnit's init `FUN_6f68a060`: deathType's raise bit clear
    0x1000 and decay 0x200, an AI's 0x4, hidden 0x8, a structure 0x10000, a flyer
    0x20000000, a gold mine 0x20000, a neutral building of Neutral Passive's 0x40000,
    its critters 0x80000, Return Resources' DataA 0x4000 and DataB 0x8000); its type's
    classes, targets, armor type, launch and impact; its tint; its walk's speed; a
    spell immune unit's two words; isBuildOn 0x100; elevation points 0x8 in +0x280.
  - Its sprite: its model's stands (sequences named "Stand" with a variation's number
    alone), the first played (provisional: the running game's choice and time).
- The engine reads its data's decimals as their digits times a single precision
  tenth, hundredth, or thousandth by their fraction's count (0x3dccccce, 0x3c23d70b,
  0x3a83126f: the first two one above the nearest), the product cut toward zero to
  single precision.  Read in the quiet test map's save: "1.05", "1.55", "0.9", "1.3",
  "1.5", "1.6", "1.8", "0.39", "0.44", "0.4" as 1.0500001, 1.5500001, 0.90000004,
  1.3000001, 1.5000001, 1.6000001, 1.8000001, 0.39000002, 0.44000003, 0.40000004
  (models' scales, regeneration, attacks' cooldowns and points); "1.2", "1.7",
  "1.35", "1.1", "0.3", "0.5", "0.75", "1.25", "0.25", "0.433", "0.567" as the
  nearest.  No single rounding of the decimal gives them all.  (Not the map script's
  reals: its creeps' facings fit neither this nor the nearest with any of the
  conversions tried.)
- A unit's ability chain (its abilities, buffs, and AI's unit list, `uref`, by
  birth; a structure's rally point, the second table's `rlyp`, right after its rally
  ability) links each to the one made after it and the one before, the unit in its
  first link set, its properties (the `` `rel `` objects made right after it): all 326
  of the quiet test map's agree.  Their own records begin with CAbility's
  (`FUN_6f42e830`): flags (0x10, Creep Sleep's 0x400, Invulnerable's 0x80000010; an
  AI's 0x2), the ability made before it (not a buff), five words, its targets (+0x4c:
  its data's targs1 as bits, ground 0x2, air 0x4, structure 0x8, ward 0x10, tree
  0x40, bridge 0x400, self 0x1000, friend 0x6000, neutral 0x8000, enemy 0x10000,
  vulnerable 0x100000, invulnerable 0x200000, alive 0x1000000, dead 0x2000000, organic
  0x4000000, mechanical 0x8000000, nonancient 0x40000000; its defaults for what is
  left out not yet read), its code, a word, a timer, a word.  Written so far, all
  agreeing: Amov (references, its speed and 1.0, words, reals; two timers), Avul,
  Aalr (its in-range listener), ACsp (0.1; its sleep's periodic timer, event 0xd01bf,
  every 2 seconds, else its check's), and fire (a reference; its look, event 0xd01c1,
  every 1.5 seconds and a two-thousandth of its maximum life, at most 3, in the
  engine's arithmetic: `FUN_6f41baf0`).  A periodic timer keeps its count from its
  start, its next due time stepped from its start by its period.  The word after the
  targets is the ability's id (a shop's Ane2, a creep's ACvs and ACmi, a town hall's
  Argl), not its kind's code.  Also written, all agreeing: regeneration on blight
  (0x80 as it stands on blight; its look every 2 seconds, event 0xd01c1), passives
  with an order (Aven, Amim, Andt: order 852290), a shop's Aneu (its customers none,
  its listener), Artn, ARal (its rally point), Amic (its last word 1), Amil (its
  unit's type and the militia's), Aupg, and Aque (its seven places empty).  Spells
  (CAbilitySpell): its cooldown left, its unit's cast point and backswing (Repair's
  and Restore's none and the largest real, with more references), references; its
  order 852290, words, its listener on its unit's mana, two timers; an auto-target
  spell's look every 3 seconds (event 0xd01c2), its first 0.02 after its start;
  Ensnare's in-range listener; Cannibalize's reference, two target masks, and its look
  for corpses every 2 seconds.  A woken creep starts its auto-target spells' looks the
  last in its list first, Abolish Magic among them, its type's autocast spell's apart
  and last (read: a High Priest's Abolish Magic 572, Inner Fire 573, Heal 574).
  All of the quiet test map's agree.  Building (AHbu, AUbu: the undead's one more reference and a short of 1), harvest
  (Ahar, Ahrl: DataC and DataB, its unit's attack's point and backswing, DataA, eight
  places, its Area), a haunted mine's gathering (Aaha: its range), and a gold mine
  (Agld: its gold, 15 and 2.0, the haunted mine over it, its shaft) agree too.  Not
  yet: auras, buffs, a shop's selling, blight, which name the second table's effects.
- A map loaded directly (-loadfile) draws on the main generator as the game starts,
  before its script: races and start locations' shuffles (see below: not a weighted
  choice's real, as first read); traced on the quiet test map, the 13 before its
  structures' alarms' and wanderers' first waits.  The wanderers' waits then agree
  but for six one unit in the last place low in the original's (traced under
  OpenGL, whose processor keeps extended precision: lo + r * 5 rounded once; the
  port's rounding twice agrees with the Booty Bay captures; provisional).
- Wander's record (CAbilityWander): its wait's timer (event 0xd01bf), once, due at
  its start and the wait drawn, its count from its start.
- An attack's record (CAbilityAttack, `FUN_6f49bd60`): its flags (0x80010; an AI's
  0x2, two weapons 0x100000, not a worker's 0x20004000, one never asleep 0x2000000),
  then per weapon (two each, an absent second's zeros, attack type 1, targets 1) its
  dice, sides, damage, an aura's bonus (a ceiling of the factor times the least
  damage: Trueshot's on trolls, provisional), its kind (normal 1, missile 2), attack
  type (common.j's order), sound (common.j's WEAPON_TYPE order), cooldown, damage
  point and backswing (each with 0 and the largest real); its targets (as bits), its
  acquisition range, ranges, range buffers, 600, its periodic timer (event 0xd01ad,
  every 2 seconds, last started as it woke) and its look for a target (event 0xd01c1:
  first as it was made, after the wait drawn, then half a second after each look's
  due time), its place.  31 of the quiet test map's 45 agree; the rest swap counts
  with a neighbor started in the same moment (the original fires its looks in due
  order).
- The regions section as the port writes it: the coarse map (8-cell blocks, the map's
  size over 8 and 3 more each way) writes no cells; the pathing map every cell, its
  flags (the map's own with 0x01, 0x10 where 0x02, 0x20 where blighted) in the top
  byte, and the static regions over it, the newest first: the destructables' and
  structures' footprints (types 0xc2 and 0x10 over the blocked cells, 0x08 over where
  none may build), not the units' regions nor the places' of no type.  Each map's
  words: its search mark, 0x11223344, 0xb8a008f0.  The accelerator's grids (2, 4, 8,
  and 16 cells each) a word a cell of two bits a layer (ground, then the 0x80, 0x40,
  and 0x04 layers, from bit 14 down), open 0, refused 1, mixed 2, a cell refusing by
  its static byte or a footprint's types (`crate::accel`).
- A unit's two regions (`^pmr`): the pathing map's cells under it (a square of one
  more than its collision's sixteens, at most four, the cell it is in ⌊n/2⌋ from its
  start; kind 1 and flags 0xca, a structure's none) and their count; the coarse map's
  8-cell blocks of the cells its collision's circle reaches (⌊c - r⌋ to ⌊c + r⌋),
  their count's high byte 0x10 and its owner's number.  A hidden unit's coarse one
  marked 0x0c000000, its fine one emptied (bounds 0xfffe0c00, no cells).  All 142 of
  the quiet test map's agree but for the maps' search marks, the running game's.
- A unit's long-range path (`l^pa`, idle): no waypoints; its first word its owner's
  number, with 0x8 for the local player's, 0x2 for one that moves and is not a
  critter, 0x1 an AI player's that moves (nibbles of its high half); a unit's word
  0x2000002; limits 700 (a critter's 32) and 400; its fine region.  All 71 agree.
- A unit's in-range listener (`` `pil ``, CPoPoInterfLis): its check's period (an aura's,
  Ensnare's, an inventory's half a second, a shop's 2, the alarm's 3) and handler,
  0x20190400; linked on its unit's listeners' chain and on its position; a periodic
  timing from its making with its property count; its range in cells (an aura's
  Area, a shop's DataA, the alarm's Rng, Ensnare's Rng and DataB, an inventory's 180);
  the units in range with their collisions, their positions in the order a search of
  the map finds them.  7 of the quiet test map's 9 agree, the alarms' counts past
  1030.  The save falls between the agent timers and the property module at its
  moment: a script timer due at 5.0 has fired, the auras' checks due then not.
- A unit's AI (`AI-U`): fourteen words, a computer's workers' and structures' naming
  their town site's AI (`AIts`); empty handlers.  All 42 agree.
- A unit's AI list (`uref`): four words, the second and third its owner's AI list
  made before it; empty handlers.  All 42 agree.
- A unit made by script faces its literal (read by the script's compiler as the
  original does) times pi / 180 (0x3c8efa35) in the engine's arithmetic, two pi
  (0x40c90fdb) on for a negative angle: every creep of the quiet test map's, 57 of 57.
- An aura's scan lays its buffs in the reverse of the order a search of the map finds
  the units (`spatial_order`): read, the trolls' and ogres' at 2 seconds.
- A unit's clustered position (`` `pcu ``, CPoPosCl) holds its place in pathing cells
  ((x - the pathing origin) / 32), two zeros, its speed in cells a second (none
  moving: unbounded), its facing in radians, and its collision in cells; its two
  regions; -1, -1, 0; 0.8, its speed again (a structure's 0.75), its turn rate (a
  structure's 0.001), pi / 3 as 1.0471975, two zeros, its facing, 0; its whole cells
  and a flag word; two flags; its path; its property's second head its newest
  in-range listener.  Structures agree (13 of 71).  Not yet: the flag word (0x2000,
  0x2400, 0x2500, 0x100, 0x4000, 0x5000 among the units read), and a script-made
  creep's facing, a bit off: neither degrees times 0.01745329 nor over 57.29578 gives
  every creep's (each gives some), nor through atan2 or a stepwise parse of the
  script's real.
- A unit's modifiers (`` `rcm ``: its life's and mana's regeneration, and its life's on
  blight) agree for 56 of 69: two words (-1, 0x60050044), its rate and no limit, a
  timing node due 150 seconds after it was made (its stamp's clock plus 150), every
  150, with its property count; chained on the unit newest first (a creep's mana
  `` `rcm `` naming its life's as older) and on its real.  The property counts beyond
  the units' making: a script's setting a golem's mana remakes its listeners, each a
  count of its spell's event (`FUN_6f4256d0`) before and of the old's letting go
  (`FUN_6f161800`) after; the melee setup's 8:00 a count for the Day trigger's run
  (`FUN_6f298250`), one for Dawn's callback before its listener remade
  (`FUN_6f27c820`), one before the day change's two listeners (`FUN_6f2891b0`); each
  woken creep's listener a count after it (not Ensnare's, its in-range object made
  instead).  A script agent destroyed lets its reference go, a count
  (`FUN_6f060fe0`, from CListener's and RangeListener's vf2): DestroyGroup and
  RemoveLocation in the melee setup (its clearing around the starts and its search
  for the nearest mine, whose callback makes and removes a location; found by
  reading the native the VM called at each count, `*(ebp-0x80)` in its frame).  A
  unit removed takes four (`FUN_6f673610`, then three by `FUN_6f694690` as its
  orders clear), not one for each of its removal's seven tasks.  With these the
  quiet test map's counts agree through 1022, the players' units included.  A
  trigger's execution from an event queues its actions (`FUN_6f298250`), a count
  (the Day trigger's at the melee setup's 8:00); TriggerExecute runs them at once and
  takes one as its execution is let go (0x6f297997: the melee setup's, at its end).
  The setup's end, as `FUN_6f28bc9d` makes the engine's own `` `rcm `` and `` `rtl ``,
  takes three before the modifier and one after the listener (`FUN_6f161800`).
  Through 1030 they agree; then the running game's first tick, its timers due at 0
  firing (`FUN_6f060ca0`), units' order and animation events (`FUN_6f69c510`,
  `FUN_6f690490`, `FUN_6f48e860`), a native, thirteen counts in all before two
  in-range listeners, the undead's blight modifiers at 1052 to 1055, and each tick's
  reschedulings (`FUN_6f15dfc0` from `FUN_6f15d7a0`), are not yet followed: past 1030
  the port's counts are provisional.  They only order the property timers due at one
  moment, so a save loads all the same.
- A property's base real and word (+0x70, +0x74) are a time stamp, its clock group's
  time and word (`FUN_6f161090`, from the group `FUN_6f15c650` picks; `FUN_6f161040`
  measures from it): flags stamped as made (0 at the map's start; forces made by a
  timer at 2 seconds 1.9999981, the main clock at tick 400, the start of the port's
  tick), positions as their units move (0.866, 1.027, ...).
- Setting the time of day (SetFloatGameState) passes the time-of-day events'
  listeners as the clock does, but an event for equal runs its trigger only if the
  time is its value to 0.001 (`FUN_6f27c5c0`: its listener replaced the other way,
  then `|time - value| < DAT_6fcd53a0`, 0.001); the comparisons run theirs as they
  become true and keep their listeners.  Read in the quiet test map's save at 5
  seconds: melee's start, setting the time from night to 8, replaced Blizzard.j's
  dawn event's listener (= 6) without running its trigger and ran its day trigger
  (>= 6) once, the day event's listeners its first.  Booty Bay's object traces could
  not tell these apart, each making one listener.
  An agent's entry is its
  CAgentBaseAbs: its first record `FUN_6f146480` (its base's, a reference, a counted
  list of references, four more), its second its agent's vf 0x38 (CAgent's
  `FUN_6f03eda0`: the agent base's `FUN_6f0562b0`, then a word for each event id
  0xd0000 to 0xd03ff, vf 0x50, the 4 KB every agent's record has).  Classes are
  decoded one by one from here (587 game classes' names alone in the agent families).
  `w3_formats::w3z` reads the header and the object manager, pairing each object's
  records with its entry, and decodes every record of the 1.27b save (13939 objects,
  both records each, byte for byte: `First`, `Second`, the agents' layouts in `AGENTS`
  from each class's vf 0x38 and vf 0x48, build-gated fields as 1.27b writes them all).
  After the objects (`SaveGame::state`): the game's (CGameWar3 vf 0x10, `FUN_6f1e8460`:
  the hero names and ids, its CMapSetupWar3 and CFogOfWarMap, start locations, player
  lists), the terrain's (`FUN_6f78c900`), the script data (`FUN_6f90d350`,
  `ScriptData`: the code blocks, each machine's globals by name, frames, stack, and
  strings, and its threads, every part size-prefixed), the hashtables
  (`FUN_6f246430`), a counted list of references (`FUN_6f9ca1e0`), the sounds
  (`FUN_6f359b50`), and the game UI's, to the record's end.  Each part writes back
  byte for byte from what was read (`First::to_bytes`, `Second::to_bytes`,
  `GameState::to_bytes`, `ScriptData::to_bytes`, `SaveGame::to_record`), and the
  original loads a save so written in the port's container (`to_file`, blocks of
  0xffff bytes; read: a test map's script calling LoadGame on it, after which the
  saved game's own 20-second timer saved again, `bbsv20.w3z`, 14029 objects, also
  read and written back whole).  `-loadfile` with a .w3z shows only a black screen,
  the original's own saves too.  Named so far (`SaveGame::units`, `player_states`,
  `handles`): a unit's type and owner are its widget's word and +0x24; its agent
  wrapper lists its properties: the position cluster (`` `pcu ``: x and y in pathing
  cells from the map's origin, facing in radians, move speed and collision radius in
  cells, the tables' 190 and 16 over 32) and indexed reals (`` `rel `` 1 life and 2
  mana: value, regeneration, a real, maximum; 5 day sight); a player's `` `flg ``
  properties are its JASS player states by number, gold and lumber (and gathered) in
  tenths (5000 for `MeleeStartingResources`' 500); CGameState's record holds the
  handle table (agent, tag, handle value 0x100000 + n).  The port loads the
  original's saves (`Load::Original`, `Game::restore_original`): it starts the map
  as a new game, then replaces the script's units with the saved ones and takes the
  players' gold and lumber, the time of day (CGameState's `` `rel `` 2), the map
  script's integer, real, and boolean variables, and the handles its units had.  Not
  yet: the time, construction and training, orders, items, trees, the other
  variables, timers, and threads.
- JASS bytecode, as a save keeps each code block (`ScriptData`, `w3zhead` with
  W3Z_CODE): 8-byte instructions, three register bytes, an opcode byte, and a 32-bit
  argument, with a string table from 1 ("main" first, then the type names); the map
  script's block opens with Blizzard's types, each an 0x09 with its parent's string
  and an 0x0a with its own (`agent extends handle`: 09 handle, 0a agent).  The
  original compiles common.j, blizzard.j, and the map's script in `FUN_6f9146f0`
  (called from `FUN_6f20d2f0`); its compile errors are a table of (code, message)
  at 0x6fce4b28 (`FUN_6f1fde30`, reported by `FUN_6f1fdd60`).  Its VM's switch
  (`FUN_6f91b460`) gives the opcodes: 2 and 0x2b jump, 5 to 8 local, global,
  constant, and function argument, 0xb pops arguments, 0xc a literal (byte 1 its
  type), 0xd to 0x12 register, variable, code, and array moves, 0x13 and 0x14 push
  and pop, 0x15 a native, 0x16 a function, 0x17 integer to real, 0x18 to 0x26
  operators, 0x27 return, 0x28 a label, 0x29 and 0x2a branches.  `w3_jass::bytecode`
  compiles as the original does, read in its code word for word: the test map's
  script (common.j, Blizzard.j, war3map.j; all 37289 instructions, and the 4029
  strings the compile makes) and the undead AI's (common.j, common.ai, undead.ai;
  12227 instructions, 3132 strings).  As read: a value takes the next register (1
  to 255 round, 0 a call's result); a binary operation pushes its left, computes its
  right, pops the left to a new register; an integer beside a real is converted as
  computed; `and` and `or` short-circuit, a chain without parentheses nested to the
  right; a negative literal is the literal and a negation; an empty `if` is its
  condition alone; a function's arguments are popped after it (`popn`), a native's
  not; a `debug` statement is jumped over (0x02), so never run (the port's VM now
  skips it too); string ids go by the sources' first names and literals as written,
  then what the code first uses that the sources did not have so ("handle",
  "<init>", a literal with its escapes resolved).  Not yet read: `and` with `or`
  unparenthesized, and the strings scripts make as they run (the table's last two).
- The port runs that code as the original's machine does (`w3_jass::vm`, the regression
  suite unchanged by it).  Read in `FUN_6f91b460` and its helpers: a context (the
  machine, `FUN_6f904570`, or a thread) has 256 registers of 0x28 bytes (+0x68: a
  type, the type given, a word), its frames (`AUJassFrame`, +0x2868: a parent, a
  locals table, a stack of pushed values at +0xc counted at +0x8c, +0xb8 set on a
  run's first frame), its place (+0x20, a pointer; saved and pushed as a word offset),
  and its operation limit (+0x44, 300000 a run, counted per instruction).  The loader
  (`FUN_6f917a20`) enters a function after its `function`, a label after its `label`.
  A call (`FUN_6f91b250`) pushes the return place (type 1) on the caller's stack and
  starts a frame; `funcarg` takes argument b from the caller's stack below it (and
  marks the local an argument, +0x24); `return` from a run's first frame ends the
  run, else frees the frame (`FUN_6f9161b0` releasing its arguments only) and pops the
  place.  A native (`FUN_6f918fd0`) takes its arguments from the stack's top by its
  signature ('R' by pointer, 'S' its string, 'C' a code reference registered in the
  machine's table +0x2888, `FUN_6f916880`) and pops them after, its result in register
  0.  Variables set with the machine (`FUN_6f916080`): the old string released and the
  new counted (`FUN_6f916370`, `FUN_6f9174f0`), the type becoming the declared one;
  registers set without it.  Strings (`FUN_6f91af80`): interned in the machine's table
  (+0x2874, 16 bytes a place, the count at +0xc), each literal, concatenation, and
  string result counting one more; a new one also joins the code's own string table
  (`FUN_6f908500`; so the test map's "Build Town Hall" and "Build Necropolis" from
  GetLocalizedString); at a count of none its place freed (+0x2878), the last freed
  taken first.  Arrays (`FUN_6f91ae10`): an element set grows the array to its index
  (at most 8192); a read past it is 0, never an error.  Operators: `==` on reals within
  0.001 by the soft-float difference (`FUN_6f05cb40`), `!=` bit for bit; a string's
  `!=` by place; `+` of strings only when both are strings (else the left register
  stays: `"a" + null` is "a", `null + "a"` null); a division by zero (a real's ±0) or
  an unset variable's read ends the thread (results 7, 6).  Every callback the engine
  runs (`FUN_6f1dd7b0`: conditions, filters, enumerations, timers, actions) takes a new
  thread (`FUN_6f90c950`), freed unless it sleeps; `config`, the globals' initializers,
  and `main` run on the machine's own context (`FUN_6f91c310`), so its saved registers
  are main's.
- The script data the port writes (`Game::saved_script_data`, `w3zscript.rs`): the
  code block (as compiled, then the strings interned since), the machine (its place,
  op limit, place in the code, string table with counts, globals in their declaration
  order, registers, and code references by name), and its threads, the one that called
  SaveGame as it was within the call (the original saves there: its arguments still on
  its frame's stack, its place past the call).  The contexts' places are one table for
  every machine and thread (`FUN_6f9107f0`; 0 unused, the map script's machine 1, an
  AI's machine as StartMeleeAI makes it), a thread's taken as it first runs and given
  back as it ends (`FUN_6f9100c0`), the last given back taken first; the saved count is
  the most ever taken.  On the quiet test map at five seconds all 426090 bytes agree
  with the original's but one word: a thread's base frame's return place, the
  original's code address taken from none (a heap address: 0xfdedc9fc in the quiet
  test map's saves), the template's where it has one (provisional).  The original
  loads and plays the save written with it.  Not yet: the AI scripts' machines and
  threads (the template's).
- SaveGame keeps the calling thread within the call for good: the save is written in
  the call (its argument still pushed, its place past it), and the thread never goes
  on, nor in a game loaded from the save (read: bbst10.w3z, saved at ten seconds by a
  game that loaded bbst5.w3z, holds both saves' threads, each just past its SaveGame,
  places 8 and 9).  A timer keeps its callback's thread should it stop
  (`FUN_6f2975c0`, its +0x50: a sleep or SaveGame), where conditions' and filters'
  threads are freed (`FUN_6f90d310`).  The port does both (`NativeResult::Suspend`,
  `Thread::kept`): bbst's threads then agree.  A load re-counts every string the
  script holds (each global's, each thread's register and pushed value, each sound's
  label: Booty Bay's "QuestCompleted" two more for its two sounds) and gives a
  handle global never set its type (bj_stockPickedItemType 0 to 7), as read in the
  saves made after a load (bbq20, bbst10, bbsv20); not yet modeled when the port
  loads an original's save.
- The game state's other parts as the port writes them (`w3zstate.rs`), all the same
  as every test save's: CGameWar3's first (`FUN_6f1e8460`: 1, 1, the `+gam` object,
  three name tables in the order made, `FUN_6f1d6c60`: UnitUI.slk's unit names with
  their spaces dropped, none, and the 348 orders; then 0 and 1 as shorts).  The unit
  names follow UnitUI.slk's rows but for two (the rocket engine after the siege
  engine, the sea elemental after the revenant of the depths; not explained by any
  SLK's order: provisional).  The map setup (`FUN_6f2465b0`): the map's name and
  description, its scripts' xoro checksum (the header's), 1, its path from the game's
  folder, and constants.  The second part: the twelve start locations
  (`FUN_6f1dbec0`: place, SetStartLocPrio's priorities by index, whether its player's
  slot is playing (a computer's is, an empty slot's not: Forestwalk's third), 0, -1,
  whether defined, 0), the players count, 12, 2, the sixteen `+ply`, 12, each team's
  playing players, 1, a second-table object, and 3, 0, 0, 0, 0.  The third: none, the
  game cache manager (`gccm`), and twelve words of uninitialized memory (Wine's
  addresses among them; the template's).  The references part (`FUN_6f9ca1e0`) is the
  AI's town sites (`AIts`) and their count; the sounds' (`FUN_6f359b50`) the eight
  volume groups' levels, "Music", -1, and the looping sounds on objects (Booty Bay's
  haunted mine's MineDomeLoop in bbst10; not yet); the hashtables' none (not yet
  written when a game has some: they keep their entries' order, the port's maps
  none).
- A haunted mine's work slots and their circles (Abgm's objects after its two reals:
  five `rslt`, then five of `+EIP`, `` `pos ``, two `^pmr`): a slot keeps its place
  (`FUN_6f4d7e40` in the engine's soft floats: the turn times k over n, x the mine's
  plus the radius times its sine, y plus the radius times its cosine), a quarter turn
  less that angle, its circle, and the acolyte at it; its circle (the Effectart's
  sprite) the slot's place floored to quarter units, in pathing cells for its
  position, its coarse region flagged 0x02000000 and 0x60000001, and the ground's
  height there.  The port writes them, all agreeing on the quiet test map but the
  circles' heights by 0.01 (the port's corners interpolated bilinearly; provisional).
- A player's agent's own record (CPlayerWar3, `FUN_6f269d70`): its own word 0 for the
  local player, 2 for the others; the game's handler and, keyed by each listener on
  its flags (its own at slot +27, word 6; a state registration's, word 0; the local
  player's interface's six `` `fcl `` in the second table, worded by the state each
  watches: gold, lumber, the two upkeeps, food cap, food used) in four buckets by
  slot, the main table's first; its 63 event lists (0x8020b to 0x80233, 0x80305 to
  0x80318, 0x80260, 0x80261), the script's registrations by event, and on 0x80261
  the tech-change listeners: as made (queues, repair, restore, inventory, creep sleep,
  the auras and an aura's buff on its own caster), then the creeps' autocast spells as
  they woke (the last creep first, its spells from its last), then the auras' buffs
  given later, as made; its name and number; its selection (empty); its alliance
  object; its food cap and used, 96, race (an unused slot's: beyond the map's players
  4 for the first six, then 3; an empty one of the map's 3; provisional), color,
  controller, whether playing (the neutral players too), team and start or -1; its
  `` `fcl `` and the interface's (by birth); handicaps; its tech table (`tech_regs`,
  an unlimited type's limit 0x7fffffff); its structures' and other units' counts; its
  AI twice; its bit.  All sixteen of the quiet test map's agree.  A unit removed takes
  itself back from its owner's tech table (`FUN_6f23b030`): the port did not (read:
  Neutral Passive's gold mines, six made, one removed for the haunted mine, its
  hidden one added: six present); now it does, the regression suite unchanged.
- The game's object (`+gam`, slot 641) has five properties: flags 0 and 1 (0), the
  time of day (a continuous `` `rel ``: the time as last set and the main clock then,
  8 at 0 on the quiet test map, 15.5 at 0.99999905 after bbst's SetTimeOfDay at one
  second; its rate, 0.05 hours a second times the scale; 0 and 24; linked to the
  time's monitor and listener, the `` `rcm `` and `` `rtl `` the engine makes after
  main, `FUN_6f28bc9d`, which the agent's last two links name too), the scale (1,
  unbounded), and flag 4 (1).  The port writes them, all six agreeing.  The pathing
  system's objects: `^pma` and `^ama` have no records, `l^sy` names the pathing map
  and a short counting its work, `l^ac` its four grids, an empty queue of 256, and a
  word counting its work (48 and 0 on the quiet test map; 268 and 10 by bbst's
  five seconds: not yet counted, the template's).
- The fog of war's part (CFogOfWarMap, `FUN_6f250d00`; made in `FUN_6f250ae0`): two
  bytes, its width and height in cells (128 units), its rows' stride (the width's
  next power of two: 256 on Booty Bay) and that power, then four grids of shorts,
  rows from the map's bottom: +0x2c the players who have not explored a cell (a bit
  each; the neutral players' always set, the stride's spare cells 0xffff), +0x30
  those who see it now, +0x34 a reveal mask (`FUN_6f250f00` ORs a player's bit over a
  row's span, `FUN_6f251120` clears spans, `FUN_6f251590` merges it into the seeing
  grid), and +0x38 the sight heights; then its bounds (bottom, left, top, right),
  1/128 twice, three shorts (1), and a word.  The quiet test map's agree but for the
  computer player's: the original's +0x34 holds that player's bits over a disk of
  about 1400 around its base (364 cells, unchanged from 5 to 20 seconds; 156 on
  Forestwalk's computer; none for the user), and it sees that disk clipped by line of
  sight alone, not its workers' vision as the port does (97 cells more; not yet
  understood: the port writes the template's reveal mask).  Units of a type with an
  occlusion height (UnitUI's occH, 200 for the neutral buildings) raise their cell's
  sight height as trees do (read: the Goblin Merchant's cell 584, its level's 384 and
  200); the port now does so, the regression suite unchanged.
- The terrain's part (`FUN_6f78c900`): 1; 0 and a byte a terrain corner, 255 where its
  ground texture is the blight's tile (`FUN_6f757c80`), the tile `FUN_6f75c7f0` takes
  as the terrain's +0x1dc, the map's ground tile of the lowest Terrain.slk blightPri
  (Booty Bay's rough dirt, Ldro, 1: its 386 corners), which a load paints back over the
  corners so marked (`FUN_6f74cc20`); 2, the doodads' set animations (`FUN_6f778f90`:
  those of kind 5 or 6, none here) and a byte a doodad (its flags' bits 26 and 27: ten
  of Booty Bay's 603, among them two fog doodads, a hay cart, and rocks; not yet
  read, the template's); 1, 10000, 0, and the deformations (none); 0 and none.  The
  quiet test map's agrees whole, and the original loads and plays the save.
- The engine's objects, read in a save's object table and free list (a quiet test map
  on Booty Bay without the computer's AI, saved at five seconds; `w3zobj objtable`,
  `free`, and playtest's `objtable`): the port's live objects (`Tags::live`) agree
  with the original's for 12290 of 12314 (slot, kind, birth), and its free list
  agrees at its top.  A structure the script removes (the gold mine
  BlightGoldMineForPlayerBJ replaces; traced with `slotfree.sh`) frees its footprint
  at once, then on the first frame its own objects with the removal's seven tasks:
  the first and last tasks, the sixth and fifth, then (after what the haunted mine
  made meanwhile released) the fourth, its listener, the third, its abilities'
  objects from the last, its five reals from the last, its path, its two regions in
  order, its position, itself, and the second task; its object of the day change
  (each unit's `` `rcm ``, freed 3 seconds on) goes on the frame after.  The port does
  so (`Tags::removed`, `day_rcm`): 12297 agree, and its free list's top is the
  original's.  The creeps' auras first apply at 0.5 seconds (each a `+EIm` of the
  second table, then the buff).  A computer player's structure's alarm, its wait
  over (a real from 0 to 1 on stream 21, `FUN_6f6936a0`; read as drawn: 0.723 for the
  Necropolis, 0.746 for the Haunted Gold Mine), starts its watch, a position listener
  for event 0xd01c8 (`FUN_6f436510`, the ability's event 0xd01c1); the user's Town Hall
  and the neutral gold mines make none.  A unit regenerating on blight looks at the
  ground two seconds after its making (event 0xd01c1 in the unit's handler at
  0x6f423c40, `FUN_6f43a200`: on blight against its flag 0x80, `FUN_6f69d360` making
  its `` `rcm ``; the undead start's ghoul and three acolytes at 2.0).  A map loaded
  directly runs with its random seed locked ('war3'): its alarms' waits are that
  seed's draws, whatever its slot table's seed.  With these the port's object table
  at five seconds is the original's, all 12314 (slot, kind, birth), and its birth
  count too (provisional: the alarm's condition, read as the owner a computer; the
  blight look's period after the first).
- With the computer's AI (the test map's undead computer, its 5-second save against
  the port's at five seconds): births agree to 12867; then StartMeleeAI, before
  MeleeInitVictoryDefeat's timer dialog, makes in the original (traced, `births.sh`;
  all at clock 0): the AI's thread (`AItr`), two captains (each `AI-C`, `` `pcu ``, two
  `^pmr`, `l^pa`, four `` `pil ``), an order with its tasks (`+ord`, `tskA`, four `task`,
  `tskA`), a unit AI (`uref`, `AI-U`), another order with its tasks, a `uref`, and an
  order with its walk (`+ord`, `task`, `ordt`, `tskA`, `tskO`, three `task`, `tskt`, five
  `task`, `tskA`, `^clg`, `^clu`, `l^pa`, a task freed, a `` `pil ``): 55 objects.  The
  port now makes the thread and runs the AI's main at once (StartMeleeAI compiles
  and runs it until it first sleeps, `FUN_6f9cbc00`), the captains in CreateCaptains
  (`FUN_6f9c5360`, twice): births agree to 12886.  The rest are Blizzard.j's
  `RecycleGuardPosition(bj_ghoul[index])` after the undead AI's start, refreshing the
  ghoul's AI (`FUN_6f9bde20`: `FUN_6f9be910`'s order, its abilities' AI stopped, then
  `FUN_6f9b9230` rejoining the AI, `FUN_6f9c3660` its unit AI made anew, its old
  `uref`, `AI-U`, and item `` `pil `` freed, and a worker's order with its walk).  Its
  objects, in order (traced, `births.sh`, `frees2.sh`): a stop order (`+ord`, `tskA`,
  four `task`, `tskA`), a new `uref` and `AI-U`, an idle worker's order (the same
  seven), a second `uref`, a walk to a target (`+ord`, `task`, `ordt`, `tskA`, `tskO`,
  three `task`, `tskt`, five `task`, `tskA`, `^clg`, `^clu`, `l^pa`; its `^clg` freed at
  once), and a new item `` `pil ``; freed on the first frame, after the ghoul's own
  creation-time order objects: its old `uref` and `AI-U`, the stop order's (first
  `tskA`, last `tskA`, the tasks from the last, the `+ord`), its old `` `pil ``, the
  worker order's (so), the walk's `task` and `+ord`, then its last `tskA` and five
  tasks from the last; the walk's others (and the second `uref`) later.  The port's
  AI gives the ghoul its orders later and by its own logic, and tags only orders
  that come over the network: not yet modeled.  Read further: the second `uref`'s
  making and the walk are the AI making the ghoul's unit AI anew (`FUN_6f9c3660`: its
  `uref`/`AI-U`, its order stopped, `FUN_6f6803f0(1)`, its lists, then its AI order,
  `FUN_6f693450`), the order's task an idle unit's (`FUN_6f9bd6e0`, task 0xd0194,
  `FUN_6f9cd390`): a worker by its harvest ability (Ahar, Awha, Aaha with the AI's
  gold, Ahrl with its wood, `FUN_6f9c9470` and the like), else a walk to the AI's
  target at +0x30 of its data (`FUN_6f9d8a90`, the `ordt` and the walk's `tskt`): for
  the recycled ghoul, `FUN_6f9d87d0` with order 0xd0012 and target a captain (traced
  at the order's making, `FUN_6f69bd80`: its target's class CCaptainAI, the `AI-C`
  CreateCaptains made), so the ghoul walks to its captain.  The port makes the
  recycle's objects and frees (`World::recycle_objects`): Forestwalk with an undead
  computer, played alone (`fws.w3x`), agrees with the original's object table at its
  first timer birth for birth, and the network game with the computer to turn 2.
  At 0.1 seconds the original's AI then acts (traced, `birthsai.sh`): its main
  thread starts a thread (`AItr`, `FUN_6f9c01e3`); HarvestGold sends three acolytes
  to the haunted mine (`FUN_6f9c73f6`, `FUN_6f9c7892`: each an order, its idle task,
  the harvest's `ordt` from `FUN_6f9c9431`, the harvest ability's tasks, `tskA`,
  three `task`, three `tskt`, three `task`, `tskA`, and its walk's three); three
  threads more; and a build order (`FUN_6f9cafe3`, `FUN_6f9cb791`, `FUN_6f9cb885`:
  a worker's harvest stopped, `` `rcm `` and `` `rtl `` of its harvest ability, an order
  with a point, `ord.`, the build ability's tasks, two `tsk.`, a `tskO`, and its walk):
  83 objects the port's AI does not yet make, nor at that time.  StartThread makes the
  thread's object and runs it at once until it first sleeps (Forestwalk's 5-second
  save: the peons thread's `AItr` at birth 29862, its acolytes' orders, then the
  attack, set_vars, and build loop threads' together; each `AItr` keeps its sleep
  as an extended timer, its period, and its place in the script data: peons 3.0,
  GetRandomInt(1,3)'s draw, attacks 2.0, set_vars 1.0, the build loop 2.0, main
  SleepForever's 100.0).  The port now does so (its first `AItr` at 29862 too).
  HarvestGold (`FUN_6f9bbdb0`, `FUN_6f9c7330`) assigns and sends its workers within
  the call (`FUN_6f9c7410`, then `FUN_6f9c7700` for those still wanted); the port's
  records the request for its engine's next pass and makes no order objects for it:
  next.  In the network game with the undead computer (`fw.w3m`, the port hosting,
  W3HOSTCOMPUTER=8,1; `abtest/birthsfree.sh`: each take's code, birth, and slot, each
  free's slot, and a mark at each keepalive's first module), all 83 of the wake's
  objects come between keepalives 2 and 3: the peons thread, three acolytes' harvest
  orders (each `+ord`, `tskA`, `task`, `ordt`, `tskA`, three `task`, three `tskt`, three
  `task`, `tskA`, `^clg`, `^clu`, `l^pa`, then one free), three threads, the build
  order (`+ord`, `tskA`, `` `rcm ``, `` `rtl ``, `+ord`, `task`, `ord.`, two `tskA`, three
  `task`, two `tsk.`, three `task`, `tskO`, `task`, `tskA`, `^clg`, `^clu`, `l^pa`, a
  free, `+ord`), 43 frees, and a `` `pil ``; between keepalives 3 and 4 two frees,
  `` `rcm ``, `` `rtl ``, `` `pil ``, and two frees.  The take (`FUN_6f1cbf40`: the
  object, the table, the free list's head, the birth counter) gives the head's slot,
  else the table's count (+0x1c).  Named by slot, the wake's frees are the orders'
  own: each acolyte's harvest order frees its `^clg` at once and, with the tick's
  orders, its first `tskA`, `task`, `+ord`, its last `tskA` and the three tasks before
  it; the build loop's first order is the necropolis's acolyte (the AI's train order:
  `+ord`, `tskA`, the training's `` `rcm `` and `` `rtl ``; its `tskA` and `+ord` with the
  tick's orders), then a crypt begun by the newest acolyte, the first given a harvest
  (HarvestGold's workers go newest first, the AI's list): its 19, its `^clg` at once,
  a last `+ord`; with the tick's orders its `task`, `+ord`, and first `tskA`, the
  stopped harvest's target tasks (last first), tasks, block, and target task, the
  build's last `tskA` and the five before it, then the harvest's path and cluster.
  The tick's orders' frees come after the AI and before the structures' alarm watch
  (the necropolis's `` `pil ``).  The port does all this (`AI_HARVEST`, `AI_BUILD`,
  `World::ai_order_objects`, `ai_build_objects`, `train_by`): the keepalives with the
  computer agree to turn 4.  An acolyte's harvest walk ends as it comes within Aaha's
  Rng (200) of the mine between their collision circles (288.6 from the mine's center
  as it began, 200.6 after its first step, in range after its second): its last two
  target tasks go; the mine's payout makes its `` `rcm `` and `` `rtl `` for its first
  worker (`FUN_6f4e9650`, `FUN_6f4e9560`, read in the backtrace); its path and cluster
  go as the next movement step starts (the third acolyte's in its own turn after the
  alarm's `` `pil ``, the second's, ending in turn 8's last step, in turn 9).  The port
  ends the walk so (`World::acolyte_in_range`), its path and cluster at the tick's end
  for now: the keepalives agree to turn 8.  Turn 9's keepalive then differs by the
  free list's head: the second acolyte's walk, in the original ending in turn 8's last
  step, its path freed in turn 9; the port's straight walk to its ring slot crosses
  the range in turn 8's second step of three (a straight walk to the mine's center,
  tried, brings the third acolyte a turn late).  Next, in order: the AI acolyte's
  walk by the movement steps (`Unit::steps_walker`, `World::walk_goal` the mine, the
  range checked as each step begins, the path freed by `Tags::walk_ended`); the
  structures' alarms fired by their agent time among the steps (their wait drawn at
  the making, `natives/units.rs`, kept as a due time; `World::steps_before`); and the
  AI threads' wakes by their agent time among the steps too (the wake's objects come
  before the alarm's `` `pil `` in turn 2, the third acolyte's path after it in turn
  4: both orders follow from the events' times).  Tried together (AI threads run as
  the steps reach their time, `ai_threads_to`; the acolyte's walk by the steps, its
  range checked after each step's move): turn 2 still agrees object for object, but
  the port's walks begin only in the second tick after the order (its positions at 0.2
  and 0.3 the same; the original's third acolyte in range mid-turn 4, so walking from
  turn 3), its alarm `` `pil `` comes after that acolyte's path (alarms not yet by agent
  time), and its second acolyte's searched way detours east then west around the base
  (gap 207.8 at 1.1, in range in the original by turn 8).  Not kept (turn 4); next:
  why a new walk waits a tick (`walk_from`, the step's searches), and alarms by time.
  Tried again (`abtest`'s `acolyte_steps.patch`, the search to the mine's footprint as
  a worker's, `gold_run_search`): the ways are found, but the walks do not start late,
  they turn first, the acolytes made facing south turning in place 0.6 a step for five
  steps (to clock 0.33) before moving, and the third acolyte comes in range after the
  original's (in range mid-turn 4 there).  Next: the original's acolytes' places by
  turn (a single-player save from a script on the map, `abtest/savemap.sh`, read with
  `w3zobj`).
  Read so (`abtest/fw3.w3x` to `fw9.w3x`, Forestwalk loaded directly, its script saving
  once at 0.3 to 0.9 seconds; `fwa2` at 0.2): the acolytes' states match the port's
  step engine bit for bit (places, velocities, headings: the first turning 0.6 a step
  from south, the second moving at 6.6952, 1.5620), three steps sooner than the
  port's: their walks step at 0.12, 0.15, and 0.18, in the tick of the AI's order.
  With the AI's threads run as the steps reach their due times (`World::ai_threads_to`)
  the walks start so.  Then, in turn by turn order: the structures' alarms likewise
  fall due among the steps (their wait drawn at the making is a due time on the main
  clock; the Necropolis's `` `pil `` before the arrived acolyte's path, freed as the
  next step begins); an acolyte in range stands at once on the nearest free ring slot
  of the mine, facing it (the first at 0.4 on its slot 4.4 cells from where it walked
  at 0.3, the second between 0.7 and 0.8; the slots 200 from the mine, a pentagon,
  `World::mine_slots`); a walk aims at the waypoint furthest along a clear line, the
  second acolyte's at (1072, 6064) by its heading 1.5440 at 0.4, which the first
  acolyte, left where it stopped, had blocked in the port.  The keepalives with the
  computer agree to turn 11.  There the original's third acolyte, the builder, reaches
  its site (a new path, its build order's tree freed, an order after); the port's
  walks there by the old movement (`Action::Build`), not yet the step engine.
  The builder's way there, read in the saves: at 0.2 it had turned three steps to its
  harvest walk's bearing (2.9124, moving at -6.6952, 1.5620), toward the mine; at 0.3
  on 3.2792, at 0.4 on 3.5162, the site's bearing.  Yet its build order comes in the
  same run of `main` at 0.1 as the harvests (call stacks under winedbg at the births
  of the first harvest's `+ord`, the three `AItr`, the train's `+ord`: each from
  `StartThread`, `FUN_6f9c8ea0`, which runs the new thread at once, as the port's,
  `peon_assignment` the harvests and `BuildLoop` the train and build through
  `SetProduce`, `FUN_6f9bfa8a`).  So a unit walking keeps its walk until the tick's
  orders are taken up at the tick's end (its stopped harvest's objects freed with
  them), turning to the new order from the next tick; the idle acolytes started at
  the next step.  Next: the builder so, its walk by the steps, and its arrival.
  The builder still walks at 1.2 seconds (saves at 1.0 to 1.2), so turn 11's objects
  are not its arrival: they are `peon_assignment`'s second pass, its `Sleep` of
  `GetRandomInt(1,3)` (1) over at 1.1, now with `set_vars`'s wood gatherers.  Their call
  stacks run from the AI engine's worker code (`0x6f9d9153`, `0x6f9d4638`,
  `0x6f9d18f9`), not the script's natives: a path search (`^clg`, `^clu`, `l^pa`, the
  `^clg` freed), a `^clp` (`0x6f89c873`), then a move order of 17 (`^clg`, `ordt`,
  `tskA`, `tskA`, `tskO`, two `task`, `tsk.`, five `task`, `tskA`, `^clu`, `l^pa`) and its
  `+ord`, an older order's tree freed with them (`tskt`, three `task`, `tskO`, `tskA`,
  `ordt`); in turn 12 a harvest order of `AI_HARVEST`'s shape and a `uref` freed: the
  ghoul sent to the trees.  The port sends it with `order_unit` alone, making none of
  these.  The older tree is the ghoul's walk to its captain, made at the load
  (`recycle_guard_position`, `FUN_6f9d87d0`), which the port takes too but never walks:
  in the saves the ghoul, made facing south at (352.51563, 500.875), faces 1.7124 from
  0.2 on (five turning steps of 0.6 from the game's first steps), never moving, until
  1.1; the bearing to the gold mine from it is 1.735.  So its walk found nowhere nearer
  and it turned to face its goal, done within 0.2 of it, its order's objects kept until
  turn 11's order replaced them.  The port's ghoul faced south throughout, so its walk
  at 1.1 started from the wrong heading.  Now it walks so (`World::captain_walk`): its
  goal the defense captain's home, midway between its town hall and mine, 101 away;
  within 300 it only faces its way's first point (1040, 5808, bearing 1.5958), 0.5 a
  step, done within 0.2: six steps to 1.712389, as read (facing the home itself, at
  1.734, would have ended on 1.734; a whole step's turn past a nearer bearing, tried,
  broke the wanderers).  Provisional: the radius, and the point faced.  Next: the wood
  order's objects at 1.1.  Its shape, from the original's turn 11 and 12: the AI's own
  path query first (`^clg`, `^clu`, `l^pa`, the `^clg` freed, then a `^clp`, all but the
  goal kept to turn 12); a move order of sixteen (`^clg`, `ordt`, two `tskA`, `tskO`, two
  `task`, `tsk.`, five `task`, `tskA`, `^clu`, `l^pa`), its `^clg` freed and its `+ord`
  taking that slot; freed with it its second and fourteenth objects and its tasks
  ninth to thirteenth, the captain walk's tree (its ninth back to its third) and path.
  At 1.2 a harvest order of `AI_HARVEST`'s shape, its `^clg` freed, the `uref` of the
  ghoul's captain, and the move order's rest.  In the saves the ghoul moved 0.497 cells
  on bearing 1.78 by 1.1 (two steps at its 0.253), then turned east by 1.2.
  Read further: the move order is not the wood order but the captain's look at its
  unit a second after taking it in (an agent timer at 1.0 sending event 0xd01c9 to
  the captain's dispatcher, `FUN_6f9d4aa0`'s table, to `FUN_6f9d9020`, which sends an
  idle unit home through `FUN_6f9d4600`); saves at 1.03, 1.06, 1.09 show the ghoul
  turning and walking from the turn's first step (`World::captain_checks_to`).  The
  wood order comes in turn 12: `peon_assignment`'s wake due at 1.1 runs at the next
  tick's first step (the AI update at a tick's end runs threads only up to its last
  step's clock), and HarvestWood orders at once, as HarvestGold does (the ghoul on
  its tree's bearing by the 1.2 save); its objects twenty (`AI_WOOD`).  With these
  turns 11 and 12 match the original's slot for slot and the keepalives agree to turn
  13.  The ghoul's way home differs a little (the original's first point the cell north
  of it, the port's search going straight to the home).  In turn 13 the captain's path
  query's path and cluster go at the turn's end, after the blight dispel's objects
  (`Tags::next_turn_end`).  At 2.0 the crypt is begun; the game module then differs by
  the computer's tech tree hash: a structure begun is in production, not yet made or
  present (`World::tech_begun`, `tech_built` as it finishes; `FUN_6f249cf0`), one bit of
  two.  The keepalives agree to turn 20.  The other bit is the birth count: as the
  crypt is begun (the build ability's `FUN_6f54d0f0`) the original makes, after the
  AI's objects of the new structure, the construction start's event data (`pucs`,
  `FUN_6f2609b0`: only when something listens for event 0x8021a, as the melee
  defeat trigger does), the trigger's `+tex`, a Build In Progress ability (`ABnP`,
  `FUN_6f533490`) and its `` `rel ``, then as the build ability starts it
  (`FUN_6f41b380`) the rise's `` `rcm `` and `` `rtl `` (`FUN_6f436b00`, event 0xd01c4)
  and a regeneration `` `rcm `` (`FUN_6f431e50`).  The build order's second `tsk.` is
  freed first at the turn's end, then the turn's objects, the `+tex`, and the `pucs`;
  its path and cluster at the next turn's end, after that turn's AI objects
  (`Tags::next_turn_end`, as the captain's query's).  The four undead's blight looks
  (due at 2.0) run at the first step of turn 21: the main clock is 1.98 at turn 20's
  last step and 2.01 at turn 21's first, so the looks now fall due on the main clock
  and run at steps (`World::blight_checks_to`).  The keepalives agree to turn 22.
  Then (traced by keepalive, `abtest/tf22k.sh`; the turns' births and frees): nothing
  in turns 22 to 25; the satyrs' fades' objects in 26 (as the port, their threshold
  listener's node due 2.5000002 on the main queue, read in memory); in 30 the crypt's
  alarm `` `pil ``, then the acolyte's order ending (the unit's event 0xd019d,
  `FUN_6f691260`, `FUN_6f67df00`) with a task from its AI, then its build order's
  objects at the turn's end (its `tsk.`, three tasks from the last, a block, its
  `ord.`, the new task, its last order); the day's objects in 31.  The order ends a
  second after the build begins: `FUN_6f54d0f0` sets the unit's animation timer
  (`FUN_6f69c620`, event 0xd019d) for 1.0, the build begun as the acolyte's walk
  arrives in a movement step (its caller the step engine's arrival).  Read in a
  single-player save at 1.9 seconds (`abtest/fw19.w3x`): the acolyte at cells
  (339.3957, 501.3349), the port's place after turn 19's last step to the bit; its
  build order's `ord.` holds the site, (512, 5760), the crypt's middle.  With the
  alarm's wait (0.9489) and the order's second both in turn 30, alarm first, the
  build began at turn 20's second step (1.95), the acolyte 103.56 from the middle
  then: the port's AI builders walk by the steps (`World::build_step_arrive`), in
  range by their place a step on (provisional: half the collision would fit too),
  and what is made then counts from that step's clocks (`World::making_clock`: the
  alarm's and blight look's due; the order's end, `World::build_ends_to`).  A
  structure's alarm due after a tick's last step waits for the next tick's steps (the
  pass at the tick's end reads the tick's last main tick, not the next one's first).
  The AI's build loop's second pass (StaggerSleep, 2.5 seconds) orders nothing in the
  original: its builder choice (`FUN_6f9cb230`) takes only a worker whose AI state
  (+0x194) is 0, 5, 6, or 3 but for a peasant, none of the acolytes then (one
  summoning, two mining; the crypt's acolyte had been walking to the mine); the
  port's skips a worker whose order has yet to end or that is at its mine
  (provisional: the states' meanings).  The keepalives agree to turn 31.
  In turn 31 the haunted mine pays (CAbilityPossessedGoldMine's event 0xd01c4,
  `FUN_6f4efa20`): with gold left it makes a new `` `rcm `` and `` `rtl ``
  (`FUN_6f4e9650`), the pair made at its first acolyte's arrival freed right after;
  its period is DataB over the share of its ring slots at work, by the engine's
  division (`FUN_6f4e8c30`: 1 / (2 / 5), 2.5000002, saved in its record), counted
  on the main clock from the acolytes' arrival steps (`World::mine_workers_changed`):
  2.5 + (0.4 + 0.8) / 2 brings it in turn 31.  In turn 32 the ghoul's lumber
  harvest (CAbilityHarvestLumber, event 0xd0184, `FUN_6f4377d0`) finds its walk
  ended short of its tree and walks to reach it (`REACH`): an AI's wood order is the
  AI's three objects then the worker's own lumber order, so the port's AI ghoul now
  harvests by the engine's lumber run (`World::start_gold_run` taking the AI's
  objects, `run_order_made`; Ahrl's fields as Ahar's).  Its reach falls a turn early
  in the port: the original's ghoul, by single-player saves (1.0 to 3.1 seconds),
  walks home from 1.0 at 113 degrees and turns for the tree by 1.2 (heading 0.10989),
  where the port's way home differs (the open item above); at 2.5 the two are 0.1
  cells apart.  Read step by step (single-player saves at 1.03 to 1.18,
  `abtest/fw103.w3x` to `fw118.w3x`; the ghoul at cells (352.5156, 500.875) facing
  1.7124 at 1.0): at the step of 1.02 it heads 1.5958 for (1040, 5808), the middle of
  the cell north of it (352, 501), reaching it at 1.05; then it heads 1.9731 for
  about (1008, 5872), the middle of (351, 503); the AI's wood order stops it at the
  main clock 1.10, two thirds of a step past 1.08 (its place moved on by its velocity
  for the time since the step, as a change of velocity does), at (352.3441,
  501.5162); it turns 0.5 a step from 1.11 and walks from 1.17.  So the original's
  way home at 1.0 was searched with a refused cell (walking node by node,
  `FUN_6f166e90`), x = 352 at rows 502 and 503 closed to it; the port's search then
  meets no refused cell (the acolytes gone from there) and walks straight to the
  home, at 1.7343.  Read in the network game at that search (`abtest/srchway.sh`,
  the break after `FUN_6f148100` in `FUN_6f166e90`, the main clock 1.019999): the
  search context's +0xd0 is 1 (node by node), the way's five points (+0x40, last
  first) (351.5, 504.5), (351.5, 503.5), (351.5, 502.5), (352.5, 501.5), (352.515625,
  500.875), toward the goal (352.0, 504.0), the captain's home on a cell's corner.  So
  its search (class 1, the ghoul's 31) ends at node (351, 504), not the goal's
  (352, 504) as the port's does, and the walk aims at the furthest node in a clear
  line after the first (351.5, 503.5: heading 1.9731).  The search (`FUN_6f14ad50`)
  takes its start's and goal's cells by floor, as the port's, so its way ending at
  (351, 504) is a nearest way: the goal's node refused for class 1, as are (352, 502)
  and (352, 503) but not (352, 501), which points at cell (353, 503) (in the rings of
  the three, not of the fourth or of (351, 502..504)).  Yet at 1.0 the save's pathing
  map (its proximity map, read with a probe of `PathingMaps`) holds no object at or
  near (353, 503), and its flags there are its neighbours' (0x61); the units' regions
  (`^pmr` bounds [ymin, xmin, ymax, xmax]) are where they stand.  At the search
  (`FUN_6f1489a0`, its object refusal at 0x6f148aaa, `abtest/refuse2.sh`) the refused
  cell is (352, 503), its object the defense captain's second region (the captain,
  `AI-C`, a pathing object with a position cluster and two regions: its +0x34
  0x01000002, +0x40 0, bounds [503, 352, 504, 353]); the search's own region
  (+0xa8) is none.  The captains stand still from their making (CreateCaptains at
  0.0, before the first Sleep): the attack captain at its home exactly, (355.54123,
  482.75253) in cells; the defense captain at (352.5, 503.5), its home (352.0, 504.0)
  placed as a ground unit of collision 16 among the start's units (ring by ring, the
  first clear cell's middle: the port's `ground_place` gives (352, 503)).  A region's
  cells in the original are floor(x - r) to floor(x + r) of its exact place (the
  ghoul's at (352.5156, 500.875), r 0.97: 351 to 353, 499 to 501); the port's
  `regions_taken` takes them from the cell's middle less half a cell, a cell lower
  where the fraction is past a half.  With the captains' regions added (either way)
  the port's searches part from the original's: at 0.12 the acolyte from (352.9375,
  505.1328) walks to (353.5, 505.5) and then north up x = 353 in the original (eight
  nodes, the search's +0xd0 1), where the port's search, the captain's cells in the
  start's class-1 ring, stays.  So the ring check of a class-1 move (`FUN_6f148d00`'s
  masks) or the regions' cells in the port's searches are not yet the original's.
  Resolved by a watch on (352, 503)'s word in the map (`abtest/cellwatch.sh`): the
  captain's region enters the map's cell lists at 0.0 (`FUN_6f9d3047`, as
  CreateCaptains runs), but only the cells whose middles it covers: at 0.12 the
  acolyte's search finds (353, 504) empty (its word 0x61ffffff), so the region, half
  a cell about (352.5, 503.5), holds (352, 503) alone.  The port's captains so stand
  (`Captain::place`, placed as a ground unit's of collision 16; their cells in
  `World::regions_taken`): every other search of the game is unchanged, and the
  ghoul's at 1.0 ends at (351, 504) with the original's way.  Its wood order then
  stops it between steps, its place moved on to the AI thread's time on the main
  clock (`World::stop_walker_at`: at 1.12 at (352.3441, 501.5162), the original's to
  the bit); and its lumber walk searches at once, not ten steps on: its captain
  walk's search keeps to 32 passes (a local search's limit), its budget not spent
  past 64 (with 700 the port's nearest search spent it, spacing the next).  At 1.9,
  2.5, and 3.1 the ghoul is within 0.1 cells of the original's (its way's waypoints
  aimed at a little differently).  A haunted mine's payout falls due among the
  movement steps (`World::mine_payouts_to`): turn 31's after the day's objects freed
  at its second step.  The keepalives agree to turn 32.
- An AI's workers are put to the harvest only within HarvestGold's and
  HarvestWood's calls: the assignment (`FUN_6f9c7330`) is called from those natives
  (`FUN_6f9bbdb0`, `FUN_6f9bbe20`) and two wrappers no code refers to, and from
  nothing periodic.  The port's once-a-second pass sent Forestwalk's undead
  computer's acolyte, idle after summoning its ziggurat at 2.95, to the mine at 3.2
  (turn 32's extra 18 objects); the original orders it in the script's call in turn
  42 (its `+ord` from `FUN_6f9c7330`'s order, under the native).  Without the pass
  the keepalives agree to turn 45.
- An AI's train order (`FUN_6f9cb530`, from `FUN_6f9cafa0` for a type that is not a
  structure, `FUN_6f34b6c0`) ends with the AI's issue (`FUN_6f693450`), which makes
  an order and a task (`+ord` and `task` under `FUN_6f9cafe3`); its build order
  (`FUN_6f9cb7c0` and `FUN_6f9cb230`) begins with its `ord.` and ends with the
  issue's `+ord` alone.  The port had taken the train's pair for the head of every
  build (the crypt's, traced at 0.1, came right after the acolyte's train); the
  altar's in turn 45 has no such pair.  The keepalives agree to turn 77.
- An AI's builder, read step by step in the original's network game (each velocity
  change, `FUN_6f15f7e0`, of the position objects about the base from 4.4 to 7.8
  seconds; `abtest/velk.sh`): the ziggurat's acolyte, walking to the mine, is
  stopped at the main clock 4.4299 by the build order (as the harvest order stops a
  walker, `World::stop_walker_at`) and walks to the site from the next step; the port
  had let its old walk end in that tick, which took the build's walk for one stopped
  short and fell back to the old per-tick build.  As a structure is begun its
  builder stands where it would be a step on (the arrival rule's "place a step on"),
  and from there one standing on the footprint is placed as a unit made there is,
  with its size class's room (`natives::units::off_footprint`: the crypt's acolyte,
  in cell (338, 501), stands at (341.5, 498.5), the first cell of the rings with its
  2 by 2 cells below and left clear of the crypt's 8 by 8; the port's rings about the
  structure had put it at (340.5, 496.5), and every later walk of it from there).
  With these the ziggurat is begun in turn 77, as the original's, its acolyte at
  (361.5, 508.5) (as in the original's single-player save).
- The build walk's path and cluster are freed by the movement step
  (`FUN_6f15aa80`, under the agent queue) two steps after the begin: the ziggurat's,
  begun at 7.62, last in turn 77; the crypt's, begun at 1.95, in turn 21
  (provisional: the two steps).
- A haunted mine's payout due after a tick's last movement step but by its last
  main tick pays in that tick (Forestwalk's undead computer's, due at 8.07, in turn
  81, its pair renewed then).  The keepalives agree to turn 152.
- An AI's train order counts its time from the next turn without the wait a
  player's order has: the AI's thread gives it after the structures' update in its
  tick (Forestwalk's undead computer's acolyte, ordered in turn 2, is made in turn
  152, the born count then the original's; the port, waiting, made it in 153).  And
  150 turns of 0.1 summed in single precision fall short of 15, so the time is met
  within a thousandth.
- A trained unit of an AI's: as it is made the training's `` `rtl `` and `` `rcm `` are
  freed at once, and its making's two tasks (the second first) and the `ptcf` last in
  the turn's frees, after the objects of the orders its AI gives it (read: the
  acolyte made in turn 152, those three freed after its orders' twelve).  The
  keepalives agree to turn 162.
- A worker's swings at a tree take its first weapon's damage point and backswing,
  whatever its weapon against trees (read, breaking on `FUN_6f436a90` and
  `FUN_6f436ca0`: Forestwalk's undead computer's ghoul swings from 3.15 every 1.35
  and stops full at the main tick 16.128294, which its first weapon's 0.39 and 0.44
  give; its second's 0.433 and 0.433, which the port had used, give 16.166, after a
  wanderer's order due at 16.153 that the original gives after the ghoul's walk
  home).  A peon's two weapons are alike, so its reads did not tell them apart.
- An AI thread run at a main tick before the step it is run for frees its orders'
  objects as that tick ends: before the path of a walk ended in the step before,
  which goes at the step (read: the new acolyte's harvest order in turn 172, a
  wanderer's walk ended in that turn's first step).  The keepalives agree to turn
  182.
- An AI thread's sleeps count game time, and it runs at the main tick whose 5 ms of
  game time first reach its due, not at the first main clock reading past it (the
  main clock lags the game's time, 0.999832 a second; read, breaking on the AI's
  Sleep, ClearHarvestAI, and GetRandomInt: Forestwalk's undead computer's threads
  run at the main ticks 17.098131, 18.097963, 19.097795, and 20.097628, 200 ticks
  apart).  Its peon_assignment, due 17.1, so draws before the turn's first step
  and a wanderer's draws, getting 3 for its sleep; compared with the main clock it
  ran a tick later, after them, and got 1.  The keepalives agree to turn 207.
- A haunted mine's payout is a modifier's value run down to a threshold listener
  at 0 (`FUN_6f4e9650` makes the pair: a FloatModifier at the ability's +0xb4, a
  listener for event 0xd01c4 at +0xb8; the period 1 over the rate at +0xa0, the
  rate 0.2 a worker, `FUN_6f4e95e0` setting it at each count change, the gold
  left at +0x6c).  Read, breaking on `FUN_6f4e95e0` and `FUN_6f4efa20` to 45
  seconds of Forestwalk's undead computer: a count change takes effect a
  hundredth of a second after it (the first at 0.36, its rate from 0.37; at
  0.78, 10.979, 20.758, 32.430, and 37.737, the payouts then 3.080, 12.383,
  21.980, 33.494, and 38.308 to the microsecond with the old rate run to 0.01
  after the count and the new from there); and each payout's next period runs
  from the clock a ten-thousandth on (2.500101, 1.666765, and 1.250099 apart;
  the clock reads 0.0001 later for each event dispatched in a tick: the pair made
  at 19.050377, the old freed at 19.050476).
- A trained unit is made (`FUN_6f40daa0`, `FUN_6f677870` given facing 0 and flags
  0x404) at the point `FUN_6f66e3e0` finds: the structure's footprint rectangle
  (`FUN_6f22f1d0`, by its facing's quadrant) handed in pathing cells to the
  placement rings (`FUN_6f058cd0`, `FUN_6f16ecc0`, `FUN_6f14a1e0`), from side 2
  with no rally point (else the side toward it), each cell checked as a unit
  placed (`FUN_6f16ee00`).  Read: Forestwalk's undead computer's acolyte made at
  cells (346.5, 483.5) heading 0 (the single-player save at 16.0 and the network
  game's walk from there), the necropolis at (354, 492) with its 16 by 16
  texture: the first cell of the first ring about [346, 362) by [484, 500).  The
  port had searched rings about the structure's middle cell and faced it 270; its
  walk to the mine then took another way, arriving four steps early.  The
  keepalives agree to turn 220.
- Open at turn 221: that acolyte's walk to its mine (ordered in turn 172), read
  step by step in the original (`abtest/velk3.sh`): two straight legs, heading
  1.4375 (velocity 0.914, 6.814 cells a second) to about (348.5, 498.0), then
  1.4938 to the port's last node (349.5, 511.5), arriving at 20.757517.  The port's
  search (to the mine's footprint, refused by the defense captain's region at
  (352, 503) and the slotted acolytes' at (345..346, 509..510) and (352..353, 509))
  walks it cell by cell up x = 346.5 and arrives a step early, before the turn-208
  payout, so the payout's pair takes other slots, freed again in turn 220.  The
  first leg's end is the path's coarse target: workers' paths plan over the
  accelerator map (+0x88's 0x200000, `FUN_6f165b60`) and steer for the coarse way's
  point some 10 cells along (`FUN_6f167ae0`, from the way's end back to the first
  point within its reach, `FUN_6f167d70`), searching locally toward it
  (`FUN_6f167ce0`); the port's walkers search locally to the goal, as only
  wanderers (no coarse plan) do.  Not yet done: the accelerator plan's search
  over the pyramid (`accel.rs` builds it).
  Read under winedbg at `FUN_6f167ae0` (`abtest/coarse2.sh`): a walker has two
  paths, its order's (mask 0, goal the mine at cells (350, 516)) and its
  movement's (mask 0x02000002).  At 17.128 the order's coarse way holds ten points
  in accelerator units (half cells; each a pathing cell's middle: 174.75 is
  349.5): the goal (175, 258), then (174.75, 252.75), (174.75, 250.75), and down
  x = 174.75 by one to (174.75, 244.75), and the start (173.25, 241.75); the
  movement path's way is the same but the goal, its goal (349.5, 505.5), the
  order's point before the mine.  Its local search to that goal (up x = 346.5,
  then across to it) met no refused cell, so the walker heads straight for it:
  heading 1.43746 from (346.6295, 484.1010), as read.  At 19.198, at (348.41,
  497.39), the order's way is picked again (`FUN_6f167ae0` from index 1) and the
  movement path planned anew from there (five points, its goal the mine); its walk
  then heads for the mine's edge cell (349.5, 511.5).  The pick scans from the
  start's end for the first point at least some reach away: 11.10 accelerator
  units at 17.128 (index 1 taken, 9.12 not), the goal itself at 19.198 (4.09 not):
  a reach of 10 (`DAT_6fd54194`, read at run time).  The scan starts below the
  last pick's index and never weighs the goal: the second pick returns it at once.
  The second pick comes from the group's step check (`FUN_6f16c4f0`): when
  `FUN_6f16b120` finds its members done (a member's +0x2a bit, the steering's stop
  answer 0x10000 in its flags; or near within 16, `DAT_6fd541a4` 256 squared) or
  its count at +0x60 passes 99 (0x18c with the group's 0x100), the order's way is
  picked again (`FUN_6f1697a0`).  Read at each step from 19.0 to 19.2
  (`abtest/groupk.sh`): the group's steps at +0x5c from 63 (it began at 17.128), a
  countdown at +0x64 4, 3, 2, 1, 0, then 16 at 19.168 (a local search again,
  seemingly), and at 19.198 the member stopped (its speed at +0x20 0, flags
  0x10000), at (348.41, 497.39): within reach of a near waypoint after that search,
  refused, seemingly (provisional).  The coarse points come from the search's nodes
  (`FUN_6f162a30`): each node's remembered cell plus 0.25 and a half (0.75 for a
  unit of size 1), the goal's and the start's own points in place of theirs.
  The stop at 19.198 is the steering's arrival (`FUN_6f16e910`): read at each step
  (`abtest/arrivek.sh`), its reach 8.3125 cells, the order's (`FUN_6f05a5c0` sets it
  from the two units' sizes, the target's with the ability's range: 16, 50, and
  Aaha's 200, over 32), tested at the walker's place in the step against the
  coarse point it steers for: 8.3936 away at 19.168, 8.1874 at 19.198.  No local
  search ran between; the group's countdown at +0x64 is not one.  The port plans
  so (`accelpath.rs`: `FUN_6f164c30`, `FUN_6f164a20`, `FUN_6f163f50` and the
  expansions, `FUN_6f162a30`; its ways at 17.128 and 19.228 the original's, point
  for point, with gold mines, haunted or entangled, left out of the pyramid: the
  goal (175, 258) is open in the original's), picks (`next_target`, reach 10), and
  stops so (`World::coarse_step_arrive`), for an AI's worker walking to its mine.
  Its stop then falls where the original's did, (348.4108, 497.3854).  Its walk on
  to the mine searches with refused cells where the original's met none (the
  necropolis's footprint, the defense captain's region, the slotted acolytes'),
  walking node by node where the original went straight for (349.5, 511.5); the
  slotted acolytes' and the captain's regions do refuse (without them the
  keepalives part at turns 9 and 32).  The keepalives agree to turn 235.
- AI threads due at a movement step's own main tick run after its movement; those
  due at earlier ticks before it.  Read, every draw of the original's generators
  from 17.0 to 19.2 (`abtest/rngk3.sh`): at 17.098131, turn 172's first step, a
  wanderer's walk end drew its wait (`FUN_6f5af385`) before peon_assignment's
  GetRandomInt at the same tick; the port had drawn the script's first, giving the
  wanderer (0x70000072) the script's number, its wait 8.2786 where the original's
  6.3685 put its next wander at 23.467 (turn 235).  The keepalives agree to turn
  245.
- The orders those threads give free their objects with the step's walk ends,
  after them.  Read (`abtest/tf176k.sh`, the original's objects from turn 176):
  turn 172's first step ended a wanderer's walk (its tree's seven, `move_arrived`)
  at the tick the computer's harvest order was given (`AI_HARVEST`, its wrapper
  and four tasks to free); the original freed the walk's seven first, the order's
  taking none of their slots, so the next wanderer's order (step 1) took the
  order's slots for its first eight and freed `0x70ef` first where the port freed
  `0x74e7`.  Unseen in the keepalives (only the free list's top is) until the
  haunted mine's payout at turn 245 freed the slot.  The keepalives agree to 270.
- A payout of the haunted mine due in a main tick before a wanderer's, both after
  a turn's last step, comes first.  Read: after turn 270's last step, the mine due
  26.980217 (main tick 5397) and a wanderer 26.987633 (5398); the original's free
  list's top after the turn was the wanderer's order's last freed (`0x7196`), not
  the payout's.  The keepalives agree to turn 325.
- A unit's state (+0x194) is set by its orders' blocks (`FUN_6f6927a0(unit, n)`
  makes the `tskA`; event 0xd0162, `FUN_6f697ed0`, sets it): harvest 5 (or 6 for
  an acolyte's), return 6, attack and move 7, an undead build 2, building 3.  An
  AI's builder (`FUN_6f9cb230`) is one in 0, 5, 6, or 3 but for a peasant (`hpea`,
  `nhew`); no other structure is ordered while one is in 2 (`FUN_6f9c9280`).
  Read (`abtest/st325.sh`, the acolytes' states and the AI's build tries to 33.6):
  the acolytes at the mine in 6, the crypt's acolyte in 2 from 0.1 to its build's
  end at 2.98, build tries only at 0.1 (crypt), 4.43 (ziggurat), and 32.430
  (altar), the last taking the newest acolyte, at the mine (0 then 2).  The port
  had kept acolytes at the mine from building, reading the pause to 2.98 as that.
  Its build frees the acolyte's six harvest objects left from `ordt`, last first,
  after the new order's first block (turn 325).
- The acolyte taken from the haunted mine changes its count at the AI's tick
  (32.430, the rate's change 0.01 on), the mine paying at 33.494152: past turn
  335's next-to-last main tick (33.489811) and by its last (33.494808), the one the
  next turn's clocks begin with.  It paid in turn 335, after what the turn
  released went (the trained acolyte's AI objects, `Tags::free_due`), its old
  `` `rtl `` the free list's top.  The keepalives agree to turn 342.
- An AI's training (to an empty queue) is made at the main clock of its order's
  tick plus the build time, in the turn's steps: the fifth acolyte, ordered at
  18.43, made at 33.432907 (its state's first set, `abtest/st325.sh`), after turn
  335's first step had freed a walk's path, the unit's objects taking the slots
  two places on (read in the original's frees of the turn: each of the port's
  fifteen two places up its free list).  The fourth, ordered at 0.1, made at 15.1
  as turn 152 began, fits both.  Players' trainings are still made before the
  turn's steps.  The keepalives agree to turn 383.
- An acolyte stands 16 up and right of its haunted mine's ring slot
  (`FUN_6f4b9910`: the slot's place less -16 each way given the unit's position
  call, vfunc +0x180); the slots themselves (`FUN_6f4ba3f0`, `FUN_6f4d7e40`) and
  the save's markers are unmoved.  Read: the altar's acolyte leaving its slot 4 at
  (344.556, 518.431) for the port's (344.056, 517.931); the second acolyte's
  steering at 0.42 about the first, standing so, the original's to six digits
  (`abtest/vela.sh`).
- The steering's arrival (`FUN_6f16e910`): in reach (the two sizes and the range
  over 32, at least 0.49) it stops; arrived only with its heading within
  `DAT_6fcd53d4`, 0.2, of the target's bearing, else it turns, a turn a step, and
  is judged again.  Read: the second acolyte in reach of its mine at 0.75 26.8
  degrees off, stopped then and arrived at 0.78; the first, 5.4 off, arrived as it
  came in reach at 0.36; the altar's acolyte in reach of a coarse point at 35.4887
  14.2 off, arrived at 35.5187, walking on from 35.5487.
- An AI's build walk steers by a coarse way too, its reach StructureFollowRange
  (100) over 32; and the coarse way is planned with every structure in the pyramid
  but the walk's target (gold mines included; provisional: the saves' grids still
  leave mines out).  Read: the altar's acolyte going south about its haunted mine,
  stopping at (352.8942, 501.1347), 3.076 from its point and not a step before,
  3.276, as the port's.
- A builder arrives by its place at the step's clock within StructureFollowRange
  of the site (velocity traces, `abtest/velc.sh`, `abtest/velb.sh`): the crypt's
  acolyte 103.56 at 1.95 and stopped at 1.98, 96.95, its order ending at 2.979974;
  the altar's 102.75 at 38.337 and stopped at 38.367, 96.5.  A build order ending
  after a turn's last step by its last main tick ends in it.  The keepalives agree
  to turn 385.
- A JASS timer due exactly at a turn's end goes off as the next turn starts, at
  its first main tick, after that tick's blight checks (due by game time) and
  before its first step; the port had run it at the turn's end.  Read with the
  main clock (`abtest/tf21c.sh`): Forestwalk's melee setup's 2-second timer's
  forces at 1.999998, turn 21's first main tick, after the blight checks' four
  `` `rcm `` then, before the first step at 2.009998.
- An AI build's walk path and cluster go as an ended walk's do, in the next
  movement step (`Tags::walk_ended`; the frees from `FUN_6f15aaec`'s step, read:
  the crypt's at 2.009998, the altar's in turn 385 before a wanderer's tree), and
  what the begin lets go (its `tsk.`, the structure's AI objects) as the begin's
  step ends (read: the ziggurat's, begun at turn 77's second step, before its path
  in the third).  The keepalives agree to turn 411.
- A worker whose tree falls under its chopping before it is full goes on to the
  next tree as its last backswing runs out, not home (`FUN_6f437fb0`, event
  0xd0185 of CAbilityHarvestLumber: carrying less than its load, it searches with
  `FUN_6f4153a0` about itself, the felled tree left out, else about the tree its
  order named (`FUN_6f418080`), and is given a harvest order, 0xd0032; full or
  with no tree, home, 0xd0031).  Read (`abtest/tf411c.sh`): Forestwalk's undead
  computer's ghoul, its tree felled at 40.6459 with ten of its twenty, given the
  harvest order's seventeen at 41.085636, its `^clg` freed then, the old order's
  eight and the new one's five after.  The keepalives agree to turn 415.
- A next tree's cost (`FUN_6f4153a0`, each candidate by `FUN_6f439e00`, the first
  of the cheapest kept, strict `<`): `FUN_6f059590` lifts the two ends' pathing and
  plans over the accelerator pyramid (`FUN_6f1627e0`, 0x20 pops): within one node
  (or from no node) the two places' doubled straight length, else the way back's
  legs between the nodes' remembered cells (`FUN_6f163440`; a parent flagged at
  +0x23 counts 2, not yet told apart), none found skipped.  Read: the ghoul's walk
  (`abtest/velg.sh`) turning north at 41.2455 and stopping 4.43 from (1600, 6080)
  at 41.425449, its reach (116 + 32) / 32; that tree's cost 8 against the one
  enumerated first's 10.  Ported for a tree felled under the chopping only: a
  Booty Bay peon's drop near turn 7290 takes a tree of 12 over two of 8 enumerated
  after it, so something (`FUN_6f41d160`'s list of eight at the ability's +0x70,
  or `FUN_6f3ff600`'s refusal of a target flagged 0x40) leaves those out; the drop
  keeps the way's steps meanwhile (provisional).  The keepalives agree to turn
  421.
- A haunted mine's ring slot is held only by the acolyte standing on it: one gone
  off (to build) holds none, and another may take it meanwhile.  Read: the altar's
  acolyte sent back to the mine at 41.095631 by the AI's harvest order, the newest
  acolyte standing on the slot it had left, the mine paying 1.25 seconds after its
  last at 42.057987 (its count unchanged); the port had moved the newest off for
  the returning one's kept slot, its count three then.  The keepalives agree to
  turn 445.
- An AI thread runs at the first main tick whose game time (the tick's
  milliseconds over 1000, single precision) is strictly past its due.  Provisional:
  inferred from the keepalives, not read in game.dll.  Forestwalk's undead
  computer's 1-second thread due 41.0999985, the time of tick 8220 to the bit, ran
  at tick 8221 in the original (a comparison of at most would run it at 8220, its
  order then a step early).  The keepalives agree to turn 478.
- A worker's pause after a swing (Ahar's Dur less its first weapon's damage point
  and backswing, `FUN_6f40bce0`, 0xd01be) runs on as its felled tree sends it to the
  next: reaching that tree before the pause runs out, it swings as it does.  Read:
  Forestwalk's undead computer's ghoul, its swing felling a tree from 40.2555, its
  backswing out at 41.085, at the next tree by 41.455, swinging from 41.605350 (1.35
  after the last), every 1.35 after, full at 47.836926.  The keepalives agree to turn
  516.
- Back from its hall, a lumber worker that had gone on from a felled tree to
  another goes about the tree its harvest order named, not to the one it went on
  to: that tree if alive, else the one cheapest about it.  Read: Forestwalk's
  undead computer's ghoul, its first tree felled with ten of its twenty, home full
  from the second, then steering for (371.5, 507.5) and stopping 4.584 from the
  middle of the tree at (1728, 6080) at 51.564877, not going back to the second.
- A walk home ends in reach of the hall's middle by the walker's and the hall's
  sizes over 32 (`FUN_6f16e910`'s reach, no range), as a walk to a tree does by Rng:
  in reach and on its bearing it arrives, else it stops, turns a step, and arrives
  in the next.  Read: the ghoul (31) home to its necropolis (196, 7.094 cells), 7.25
  from its middle walking on, at 7.022 stopped 0.21 off its bearing at 49.585964,
  arrived at 49.615948, walking out at 49.765865; 0.516 short of its way's last
  point, where the port's way's end in 0.49 had it a step later.
- A lumber run's searches have the mask 0x10000010 from its first walk: other
  units' regions and the captains' (0x02) refuse it no cell.  Read under winedbg at
  `FUN_6f1489a0` for cell (352, 503), the defense captain's: the ghoul's move
  order's search at 1.019999, mask 0x02000002, refused there (+0xd0 set, its walk
  to the way's first node); its harvest order's at 1.109999, mask 0x10000010, the
  cell open, +0xd0 clear, so it headed straight for its way's end (369.5, 503.5) at
  1.229999, velocity (8.386509, 0.925042) as the port's now.  The port's ghoul had
  walked by the first node, 0.0126 cells off the original's ever after, and missed
  its third tree's reach by 0.0035 in the squares at turn 516.  The keepalives agree
  for all 530 turns of the network game with the undead computer.
- A weather effect is no handle: `AddWeatherEffect` (`jass_AddWeatherEffect`,
  `FUN_6f78b020`, `FUN_6f762b20`) gives the weather system's own id, the first free
  place in its list of areas (`FUN_6f763300`), so the first is 0, and
  `EnableWeatherEffect` takes that id as it is.  Read in Forestwalk's test map's
  save at 2.5 seconds: its three regions' rects in handles 0x100d7c to 0x100d7e
  together, no handle for the weather effects made after each; the port's table had
  three empty handles between them, every later handle three higher (42 globals and
  33 registers apart).  Now the tables are the same size, every global and register
  agreeing.
- A player-unit event with listeners (`FUN_6f2609b0`, 0x8021a for a construction's
  start) makes its data agent and gives it a handle as it goes to its triggers, then
  the unit's (`FUN_6f262cb0`, each by `FUN_6f282e30`, which takes the last freed
  handle or a new one); when the executions end the data's references are gone
  (`FUN_6f283f40`) and its handle is freed (`FUN_6f28a300`).  Read under winedbg on
  Forestwalk's test map (`abtest/hdl.sh`): at 1.979998 the data (born 30074) took
  0x100f61, the crypt 0x100f62, and 0x100f61 was freed; at 1.999998 the melee timer's
  first force took 0x100f61.  The game's record lists the handles' rows as they were
  given (the force's after the crypt's), not by handle.  The port does both, and its
  handle table and rows now agree with the original's save at 2.5 seconds but for
  the crypt's slot.
- A unit's continuous modifiers in a save (`` `rcm ``), read against Forestwalk's test
  map's at 2.5 seconds: besides its regeneration's (and on blight its life's), each
  Unholy Aura buff's on its life (kind 0x60050044, the aura's DataB a second, no
  limit, due 150 after it was made, every 150), and its last day change's on its
  sight while that lasts (kind 0x60040044, the change's rate a second and its 3
  seconds, one node due as it ends: the creeps' at the melee setup's 8:00, 200 a
  second from night sight to day sight).  They chain on the unit by birth (links
  flagged 0) and on their real by birth (flagged 1); each real's head is its newest,
  its rate theirs together (a satyr by a Hellcaller's life 1.0 and 0.5, rate 1.5),
  and a sight changing holds its value as the change began, stamped then, at the
  change's rate (800 and 200).  The port writes them so: the units' reals 601 of 650
  agree (from 485), the modifiers but for their timing counts (the property
  counter's, some 1 to 15 low).
- A thread an AI script began with StartThread is saved as the map script's
  callbacks are: a base frame holding its return place (the original's "none" code
  address, 0xFCB6BFFC for the AI's code there) and its function's frame marked its
  run's first; the AI's main has no base frame (read: Forestwalk's computer's four
  and its main at 2.5 seconds).
- The port's saves with a computer crashed the original as it loaded them, from the
  AI threads' objects on (commit 9d51f5f, bisected with `abtest/bisectrun.sh`): as a
  thread woke and slept again, `ai_Sleep` takes the running thread's object from
  its AI's list of them (CAIPlayer +0x254, three words each, by the thread's number
  at +0x29c, `FUN_6f9cc500`) and called through none (caught under winedbg,
  `abtest/dbgload.sh`: a read of 0x24 from `FUN_6f9cc500` under `ai_Sleep`).  The
  AI's record now lists its threads' objects by number in its six places (read:
  the main's and its four).  Then a second, as the first night fell: a flyer's
  burrow detection's in-range object (`` `pil ``, event 0xd01b4, its Rng over 32, a
  look every half second, its entries marked 2), made again as it woke at dawn, was
  missing, and its sleep let it go through none (`CAbilityCreepSleep` vf191,
  `FUN_6f48c8c0`, a read of 0x74).  The port writes it now.  The original loads the
  port's saves of Forestwalk's test maps at 2.5 and 5 seconds and plays on past the
  first night (seven minutes, no exception).
- An agent record the port wrote empty (a class whose save it had no layout for) made
  the original's loader read the next object's bytes as its own and hang on a
  garbage count (caught by attaching winedbg to the hung loader, `abtest/hangbt.sh`):
  Hills of Glory's creeps' Command Aura and its buffs, and Forestwalk's undead
  computer's Graveyard and hero at 160 seconds.  Their layouts, from their classes'
  save functions (vftable slot 0x38, read from game.dll's tables): the auras whose
  classes keep the aura base's save (all but Plague and Slow: Brilliance, Command,
  Endurance, War Drums, Thorns, Vampiric, a healing ward's) as Unholy Aura's, and
  their buffs keeping the buff base's as Trueshot's (a word a weapon in a counted
  list: Command Aura's 3 and 1); CAbilityGraveyard (`vf14` 0x6f5f5550) ten corpse
  places (a reference and a word), Cool, DataC, DataA, a counted list of ten units,
  and a timer; CAbilityHero (0x6f52a090) its attributes and what they give, their
  gains a level, its level, three lists of five (its skills, their levels most, the
  levels they need), two references, and its skill points (read against a
  Forestwalk test map saving at 160 seconds, `fwhero.w3x`: a Dreadlord's, the port's
  now the original's but for the references).  Hills of Glory's save now loads.
- A computer's repair logic (`FUN_6f9c3920`, run under CCompAI's timer and as a loaded
  game begins, `FUN_6f9c9880`) keeps the unit it chose last (+0x17c, set by
  `FUN_6f9cbba0`), one with Repair or Restore found by `FUN_6f48cb80` on 'Arep'; if
  that unit stands idle (+0x194 0, 5, or 6) it is taken as it is and asked for the
  ability without a check.  The port wrote there the computer's newest unit not a
  structure, at 160 seconds an idle ghoul, and the original crashed (a read of 0 at
  0x6f9c3a6b); now the newest with Repair or Restore (an acolyte, as the original's).
  Next, there: `ai_InitAssault` takes the attack captain (+0x2ac), which the port's
  saves left out, and writes through none.
- A computer's captains in a save (CAICaptain, read against Forestwalk's undead
  computer's at 2.5 and 160 seconds), each the nine objects CreateCaptains makes:
  its agent (its position its property, its newest listener its head) with its own
  record (6; its four listeners' handlers keyed by them, events 0xd01aa, 0xd01ab,
  0xd01c9, 0xd01ca; the defense captain's fixed 0xd0196 and 0xd0198; a VillagerKid
  sprite, its color word -1 or 0xff00ff00; 200; its listeners; its look every second,
  event 0xd01c1; its kind 2 or 1; its home thrice; -1, -1, 200), its position cluster
  (its place in cells, never moving, 0.8, 0.75, 0.40000004, 0.1), its two regions
  (an 8-cell block flagged 0x08000000 with 0xc0000001, one cell 0x01000002), an idle
  path, and its four in-range listeners (400, 1500, 1100, and 1300 units, the first
  two every half second, the others every second, the units within).  The port
  writes them so, and the AI's record names them; the 2.5-second save's agree but for
  counts, the search mark, and the attack captain's home's y, a single's step above
  (5208.082 to 5208.0815: the engine's normalizing, not yet read).  With units the
  attack captain lists its unit references (`uref`) and counts (four at 160 seconds),
  which the port does not yet write.
- A hero's hero ability (AHer, read against Forestwalk's computer's Dreadlord at 160
  seconds): made with two flags (its properties: its level less one, and its
  intelligence, 18) and four continuous modifiers, the first two freed as its
  attributes are set and the last two kept: its strength's life regeneration (20 ×
  0.05 = 1.0, beside the type's regenHP of 2 on blight) and its intelligence's mana
  regeneration (0.9: 18 × 0.05 in double precision; a single's product is
  0.90000004).  Its record's tail names those two modifiers; its first word is 0x92
  (0x90 with its AI's 2).  The original reads a hero's level through the first flag
  (`FUN_6f685150`: AHer +0x6c's object, +0x78, plus one), so a save without it
  crashed in the captain's look (`CCaptainAI::vf3`, `FUN_6f9d09c0`, its area
  search's filter `FUN_6f9d0bb0`).  With them the original loads the port's
  160-second save and plays seven minutes.  The hero ability's tail: 0, its unspent
  skill points (+0x90, which GetHeroSkillPoints reads), its strength, ...; its last
  word (+0x138) is its place among its owner's heroes (`CAbilityHero::vf188`: the
  player's count at +0x334, taken as the hero is made).
- A hero's attributes, as the original keeps them (CAbilityHero): base strength
  (word +0x94) and agility (+0xa8) and intelligence (its second flag, +0x7c), each
  its type's at level 1 and what tomes and SetHero* gave, without its levels' gains
  or its items'; each total the base and the whole part of its level less one times
  its gain per level (+0xd4 strength, +0xdc intelligence, +0xe4 agility;
  `FUN_6f528770` reckons agility so).  The tail's reals come from the totals:
  strength, its life (× 25), its armor (agility × 0.3 − 2), its mana (intelligence
  × 15), 0, its attack speed (agility × 0.02).  Its word +0xcc is its primary
  attribute (1 strength, 2 intelligence, else agility; common.j's 3).  Read: the
  hero map's Dreadlord at level 2, 20, 18, and 19 for 22, 19, and 21 (550, 3.7,
  315, and 0.38), and at level 4, 23 for 30 (750); its Death Knight at level 3, 23
  and 18 for 28 and 21 (700, 4.3, 0.42).  The port had read and written the totals
  there, so a loaded hero lost its levels' gains and a saved one would have had them
  twice.
- A computer's threads sleep on the main clock (each `AItr`'s timer without flag 4:
  the hero map's at 450 seconds due 150.09998 to 200.09998, the main clock at 150.0
  and the agent clock at 150.665; at 300 seconds due past 300 with the main clock at
  300.0 and the agent clock wrapped to 0.45, which had looked like dues not set back
  at the wrap).  Loading the original's save, the port took them on the agent clock,
  so they woke 0.67 seconds early, and kept the thread objects of the threads its
  restart of the AI had made, so its save then held one of the five; it now makes
  each saved thread's object again (its number, sleep, count, and place).
- A computer's thread object (CThreadAI) holds its machine and its thread's index
  (+0x38, +0x3c), which the original resolves through the script data's places as it
  loads; waking, it finds the machine (`FUN_6f90c9b0`: `FUN_6f90ab10`'s table) and
  the thread (`FUN_6f917480`).  A save's count of places (+0x8c) must reach past every
  thread's: the port's save of a game loaded from the original's 450-second one
  counted the places its own restart had taken, 5, under its computer's threads at 2
  to 7, and the original crashed as the first woke (a null machine).  The loaded
  threads now take their places in the places the machines share, as the map's.
- A haunted mine's payouts run on its ability's second real, the period counting down
  1 a second from its start, with a continuous modifier on it (-1.0 and 2.0, its node
  due 2.0 on and every 2.0) and a threshold listener due as it reaches 0 (words
  0xd01c4 and 0x20090400, every period, keyed to the ability for event 0xd01c4), the
  ability's agent heading both (read: the hero map's at 300 seconds, five acolytes,
  period 1.0 from 299.64832).  The port wrote the real empty and no pair, so after
  loading its save the original's haunted mines never paid; written so, its undead
  computer mines on (300 gold in 30 seconds after loading the port's Twisted Meadows
  save at 120, its five acolytes' rate).  Ordinary mining carries over exactly: on
  Echo Isles with player 0's peasants sent to the mine by script at 1 second, the
  port's game at 30 seconds is the original's (760 gold, 260 gathered, the peasants'
  Harvest records alike to their reals), and the original going on from the port's
  save at 30 gathers what its own game does by 60 (560).  Not yet: an entangled
  mine's payout.  Its class shares the haunted mine's payout code (177 of 192 virtual
  functions; its save, `FUN_6f4ea4b0`, the haunted mine's save's base, its tail's
  three references the same: none, the modifier, the listener), so it pays by the
  same pair, which the port does not model for it.  Tried: the pair taken as the first
  wisp enters and renewed as it pays, and the cargo hold's wisps written (CAbility-
  CargoHold's save, `FUN_6f5aabb0`: a real, three words, two lists of ten places,
  twenty words, two timers, twenty pairs of reals); the original then paid once after
  loading the port's save and no more, and the night elf start under a chosen race
  already differs from the original's at its first turn (the game module's checksum,
  over LAN with the host's W3HOSTRACE and the tags tool's TAGS_RACES), so the pair
  could not be checked there; left out.  The writer writes the pair and cargo when a
  mine has them.
- A computer's towns: its AI object (`AI-1`) lists four towns by their sites'
  indices, which TownWithMine walks (`FUN_6f9ccf50`: a human's or orc's town needing
  its site's hall, a night elf's its entangled mine); each town site (`AIts`) holds
  where its town gathers (its attack captain's home), its own place (midway from
  hall to mine once a town holds it), its hall, and its mine (a haunted or entangled
  one over the site's standing for it; after a melee undead start, which replaces the
  mine, the one at its place).  Read: Echo Isles' undead computer at 60 seconds on
  site 2 (4499.96, 2698.23; 4896, 3264, the Necropolis at 4672 and the mine at
  5120), the hero map's [4].  The port wrote every computer's towns as [1] and no
  site's hall, so after the original loaded its Twisted Meadows save the orc and
  night elf computers found no town with a mine (`HarvestGold(-1, ..)`, traced at the
  native, 0x6f9bbdb0) and sent their gold peons to the trees; written so, each asks
  of its town 0, and the orc mines on (120 gold in the next 30 seconds against 40).
  Each computer unit's AI (`AI-U`) carries flags, 0x60 and a worker's job (0x5 gold,
  0xa wood: the acolytes' 0x65, the ghouls' at the trees 0x6a), and its town's site
  for a worker or a structure; now written so.  Provisional: the flags' high bits
  (0x10000000, 0x20000000) on some, not written.
- A save's slot table keeps each player's choices (a random race as random, 0x60);
  the race the game drew is the player's record's tail word 110 (common.j's
  numbering: Booty Bay's LAN game's human and orc, the hero map's and Echo Isles'
  human and undead computer), its name the record's text.  The port loading the
  original's save alone had started the map as `-loadfile` does (the local player
  and one computer), so a skirmish against three computers came back with one AI:
  its others stood still and 5 of its 15 threads returned.  It now starts the map
  with the save's setup (`save::original_lobby`: each occupied slot its computer or,
  the local player's, this computer; the race drawn, name, team, color, handicap,
  and level), and its LAN lobby takes the drawn races too.  On Twisted Meadows with
  three computers loaded at 120 seconds, the undead and orc computers' units 60
  seconds on are the uninterrupted game's (19 and 21), the night elf's 24 of 25.
- An entangled mine (`Aegm`) is laid out as a haunted one: its gold, the mine it
  covers, 0, DataA and DataB (10 and 1.0), 0.0 (the original's one read was still
  entangling at 5 seconds, no gold and no mine yet).  The port wrote it with defaults,
  so a night elf's mine came back empty and its wisps mined nothing after a load; it
  now writes and loads both as the haunted mine's (provisional).  Its last real is
  its period with the wisps inside (DataB times its capacity over them; 0.0 with
  none), as the haunted mine's: written 0.0, the original paid once after loading
  the port's save, its next pair not made; written so, Twisted Meadows' night elf
  computer gains its 300 gold from 120 to 150 seconds in the original's continuation,
  as in the port's own game.
- A night elf start over LAN (the original joining the port's host, slot 0 night elf):
  the keepalives differed from the first turn.  Its object counts (the take's birth
  counter, `+0x50`, and the presence table's count, `+0x1c`, and free head, `+0x40`,
  read at each take and turn mark) showed four things the port did not do.  A unit
  that regenerates by night alone (the Tree of Life, wisps) has no life modifier
  (`` `rcm ``) when made by day.  The Tree of Life's `entangleinstant` makes an order
  and nine tasks (`ordt tskA tskA task task tskt task task task task`) before the
  entangled mine (freed as the next turn starts, provisional).  The gold mine's
  footprint, its three `^pmr`, is freed as it is entangled, before the entangled
  mine is made, whose first three objects take those slots (the last freed first).
  And the entangled mine's Blight Dispel (Small) (`Abds`: 768 by 64 every 0.08) frees
  its objects by turn 10 (0.9 seconds) where twelve steps from its making end at
  0.96; it is taken as a step ahead (provisional: the Booty Bay town halls' Large
  dispel ends at 1.2 as measured).  With these the keepalives agree to turn 21; at 2
  seconds the test map's script orders the wisps to harvest, whose orders' objects
  (`ordt tskA tskA task task task tskt tskt tskt task task task tskA tskO ^clg ^clu
  l^pa` for each) the port did not make.
- A wisp's harvest of its entangled mine, traced (takes and frees after turn 21,
  breakpoints enabled at the 21st turn mark): each order's `^clg` is freed at once,
  its first block and the five tasks before its closing block (indices 1, 13 to 9)
  as the next turn starts, after the script's groups destroyed before the orders.
  The wisp walks to the mine as a peon does (its way searched to the footprint); as
  the walk ends it is loaded: its `Acrg` is made, its order's two last target tasks
  (8, 7) and its spells' listeners (the last first) are freed, the mine's first wisp
  makes the payout's `` `rcm `` and `` `rtl ``, and the walk's path and cluster go as
  the next movement step starts.  Its order, block, three tasks, and first target
  task stay while it is inside.  The nearest wisp, one cell from the footprint, has
  a way of its own cell alone: it is in the mine in the next turn, its path freed in
  that turn (the port arrives it at the next tick's first step; provisional).  The
  entangled mine pays as a haunted one, each payout a new pair, the old freed.  The
  entangle order's objects are freed at turn 1 in three parts (1 and 9 to 6, then the
  entangled mine's two tasks, then 5 to 2 and 0), and the Tree of Life's Eat Tree
  listener before its own tasks (a rooted structure's; provisional).  With these the
  keepalives agree for all 1352 turns recorded, through the five wisps' arrivals and
  the mine's payouts, with two more readings (breaking on `FUN_6f4e95e0` and
  `FUN_6f4efa20`, the main clock at each): the nearest wisp's count change falls at
  2.129995, the next tick's second step, where the port had its first (the payouts,
  1.000099 apart in both, had run 0.00598 early, a fifth of that step, and from 120
  seconds fell a tick before the original's); and a payout due before a wanderer's
  order in the same step (125.848091, the order 125.863121) comes first, as after the
  last step.
- A night elf computer over LAN (Twisted Meadows, the original joining the port's
  host, the computer in the third slot, the fourth closed: `W3HOSTCOMPUTER=4,1
  W3HOSTCLOSED=1`; the map without a computer agrees for 900 turns).  At its making
  each of its wisps takes the AI's order (`+ord tskA task`) and a gold order of its
  own (`ordt tskA`, three tasks, three target tasks, three tasks, `tskA tskO ^clg
  ^clu l^pa`, a scripted order's second block less), given up at once for the AI's
  first order (its nine), whose last gives it the same harvest again, the one it
  carries out; a wisp so ordered waits for no other.  As the next turn starts the
  first harvest goes but its path and cluster (its `^clg` at once), and the second's
  five before its closing block; the first's path and cluster go at the first
  tick's end, after the time of day's objects and the game's first timer, the
  newest wisp's first.  The computer's Tree of Life takes an in-range listener after
  its AI's objects (taken as its Eat Tree's), and a creep made while a computer
  plays lets its autocast's order go as the next turn starts (four creeps with
  Slow; kept without a computer).  A structure's watch due in the game's first
  tick (0.081) begins after that tick's releases.
- The AI's harvest assignment (`FUN_6f9c7330`, within each `HarvestGold` and
  `HarvestWood` call): `FUN_6f9c7410` counts those already at the work (state
  +0x194 5 or 6 and the AI unit's flag 4 for gold, 8 for wood), then `FUN_6f9c7700`
  takes others in the AI's list, newest first: not taken in this pass (AI unit +0x28
  & 3 clear), state 0, 5, or 6, and the unit word +0x20 with gold 0x8000 or 0x10000,
  wood 0x40000 or 0x80000.  A wisp's word is 0x191445 (worker 0x100000, Awha's
  0x90000; read, the original's save).  Traced (Twisted Meadows' night elf computer,
  `HarvestGold(T,4) HarvestWood(0,1) HarvestGold(T,1) HarvestWood(0,2)`): at 0.1 four
  wisps to gold, the fifth to wood; at 1.1 that one to gold, as one on wood is not
  counted for gold and only three were.  Each order, gold or a tree, is `+ord tskA
  task` and the sixteen (`WISP_GOLD_AI`), its `^clg` at once; with the tick's orders
  the `tskA`, the earlier harvest's eight, the `task` and `+ord`, the new one's five;
  the earlier harvests' paths at the turn's end, the newest wisp's first.  The first
  wisp's harvest is given up for a build order (its eight after the build's first
  frees, its path at the tick's end, before those paths); the Tree of Life's
  training lets its Eat Tree listener go between the train order's block and order.
  With these the keepalives agree to turn 2.
- A human computer over LAN (Twisted Meadows, `W3HOSTCOMPUTER=1,1 W3HOSTCLOSED=1`):
  as each of its peasants is made it takes the AI's order (`+ord tskA task`), a
  harvest of its own at the nearest gold mine (`HARVEST_AI`: the own harvest's 19
  less a task before its closing block), the AI's first order's nine, and the same
  harvest again, the one it carries out (as a computer's wisps do).  A harvest's
  objects part after its last target task (`goldrun::harvest_split`): those before
  go when it is given up, those after but the closing block, cluster goal, cluster,
  and path as the next turn starts.  Turn 1's frees agree object for object (read:
  frees of slots from 27100).  In its AI's first pass each peasant is sent to gold
  again, newest first (`+ord tskA task` and the 19, `HARVEST_OWN`), the oldest to
  wood (the AI's wood order, its 17), each freeing its first block, its earlier
  harvest's part before the last target task, the AI order's other two, and the
  new order's tail; the earlier harvests' paths at the turn's end, the newest
  peasant's first; then a peasant trains at the town hall and the newest peasant
  is sent to build, its harvest's first part freed after the build's first block,
  its path at the tick's end.  The AI's builder may be a peasant walking to its mine
  (state 5); only one building (3) is not.  The game's first step (clock 0) moves no
  one: the waypoint provider is first asked at 0.03 (both computers' workers), the
  third peasant's way ends at 0.30 (answer 3) and it enters at 0.33, before the
  town hall's watch (0.3365) and that walk's path freed at 0.36.  With these the
  keepalives agree to turn 9; at turn 10 the second peasant reaches the mine's queue
  (`uref`) a turn later in the port.
- The same game's walks to one second (the waypoint provider, every walker): the
  second peasant, re-ordered at 0.1, heads straight for (161.5, 185.5) from 0.12
  (its search meeting no region: not refused, straight to its way's end), answers 0
  at every step to 0.78, 3 at 0.81, and joins the mine's queue at 0.84.  The other
  peasants, each stopped at the AI's clock for its order (`World::stop_walker_at`),
  still count as moving to that search; the port now takes a walker stopped between
  steps as moving to others' searches until its own next step, and its search agrees
  (along row 185 to (161, 185)).  The build peasant answers 0 from 0.12 to 0.33,
  heading for (163.5, 187.5), then (157, 199) from 0.36: it is held by no one.  In the
  port it is held at 0.12 by the third peasant (its cells ahead, (162, 187) and
  (163, 187), meeting that one's region) and then holds the second: not yet read why
  the original's test meets neither.  A unit turning in place has no speed; the
  port takes it as standing to others' tests (`World::held_back`), and one that
  makes a slower mover yield still counts itself blocked: changing either alone
  changed nothing here.  The latter is the original's too: `FUN_6f168360` never
  removes an entry from the list `FUN_6f166140` gathers (the regions on the class's
  leading edge one cell toward the target, `FUN_6f167940`, `FUN_6f149560` to
  `FUN_6f149e60`), so a walker making another yield still goes on to the blocked
  path.  The test's answers, read after its call (0x6f165caa: pos in ebx, target in
  edi; the walkers' places agree with the port's to the digit): at 0.03 only the
  fourth peasant (167.102, 188.438, toward (166.5, 187.5)) is blocked; from 0.06 to
  0.18 none, the build peasant clear at 0.12 (163.682, 186.035, toward (163.5,
  187.5)) and the second at 0.15 (165.824, 185.836, toward (161.5, 185.5)).  The
  port holds those two (the third peasant's region on (162, 187), the build
  peasant's on (163, 185)) and not the fourth; taking regions half a cell higher
  (their cell and the next) blocks others at 0.03 and the keepalives differ from
  turn 5.  The lists themselves (before `FUN_6f168360`, 0x6f1662cb: the walker's
  cell at ebp-0x18, the cell ahead at ebp-0x30, the list's count at +0x1c and its
  movers at +0xc): at 0.03 the fourth peasant (cell (167, 188), ahead (166, 187),
  south-west) lists the second's mover twice, that one standing at (166.297,
  186.133) turning to its way; every other list to 0.18 is empty, the build
  peasant's at 0.12 (cell (163, 186), ahead (163, 187)) though the third peasant
  moves at (163.042, 188.444), and the second's at 0.15 (cell (165, 185), ahead
  (164, 185)) beside the build peasant at (163.682, 186.035).  So a moving unit's
  region is not where the port puts it (its cell's span, half a cell below and
  left), while a standing one's may be: not yet read where the original keeps a
  walker's region as it moves.
- Why (`FUN_6f148ad0`, the cell's region filter, read at 0x6f148b65 in the 0.12
  step): the third peasant's region does cover (162 to 163, 187 to 188), as the port
  puts it (bounds +0x1c to +0x28: rows 187 to 189, columns 162 to 164, one past the
  last), but its flags (+0x34) are 0x01000008, not 0x02: it ghosts (0xca less 0xc6),
  and the build peasant's mask 0x02000002 does not meet it.  Others' searches in that
  step use 0x10000010, the ghosting mask.  So a computer's peasant sent again by its
  AI ghosts from that order on, before it has been in its mine (the second peasant
  still 0x02 at 0.03, before the re-order).  A region is listed only if its word
  +0x40 has none of 0x8fffffff (the gold mine's has 0x10000000) and +0x34 meets the
  walker's mask.  With the re-ordered peasants ghosting (`GoldRun::ghosted`, its mask
  0x10 though not yet in), and a re-ordered wood order's first nine freed as the
  gold orders' are, the keepalives agree to turn 14.
- At turn 15 the fourth peasant joins the mine's queue a turn early in the port.  Its
  walk differs from the setup on: the original's first node is (166.5, 187.5) (south
  west, into the second peasant's region: blocked at 0.03, answering 1 facing the
  mine at 0.06 and 0.09, its place unchanged to 0.96), the port's (168.5, 188.5)
  (east round it, its search refused by all five standing).  Searching each setup
  way as the peasant is made, before the newer ones exist, gives the original's
  first nodes for four of the five ((164.5, 190.5), (166.5, 187.5), (162.5, 188.5),
  (162.5, 185.5); the second's (165.5, 186.5) where the original has (165.5, 185.5)),
  so the original searches them then; but the port then searches the fourth's way
  again at 0.06 and finds the way east where the original's finds none, and the
  keepalives differ sooner (turn 10).  The walker's +0xa8 is its coarse path
  (accelerator units: its start and the mine, (78, 92)), not the local way.  Not
  yet read: the local way's object, and why the original's search at 0.06 finds
  nothing (its limit, or the places it refuses).
- Read (after the search's call in `FUN_6f166e90`, 0x6f166fbe: the path at
  ebp-0x14, its way at +0x34, the way's points at +0x40 and count at +0x50, the
  search's passes +0x6c and limit +0x68, 700): the original searches the five setup
  ways at 0.03, newest first, with 3, 7, 3, 9, and 6 points; the fourth's (18
  passes) runs (166, 187), (165, 186), (165, 185), and along row 184 to (161, 184),
  through the cell beside the second peasant, which had just begun to move in that
  step.  So each walker's search comes in its own turn of the step, after the
  walkers before it have moved (`World::step_walkers`); the port searched them all
  first, and its fourth's way went east round the second.  The fourth is then held
  by the second (a mover, so a four-step wait: answers 1 at 0.06 and 0.09, no new
  search), and the AI's new order at 0.1 starts its walk afresh: it moves at 0.12
  toward (161.5, 185.5) (the port had kept the old wait).  With these the
  keepalives agree to turn 22.
- At 2.1 the build peasant begins its farm.  After the structure's objects (its AI's,
  the event's data, Build In Progress and its real and rise, `` `rcm `rtl `rcm ``)
  the peasant takes its order to build (`ordt tskA task task tskO Atrg tskO task
  task task tskO tskA`, `Atrg` CAbilityTarget) and the rise anew (`` `rcm `rtl
  `rcm ``); as the step ends go the structure's first rise (the last first), the
  build order's six left (6 to 2, then its `ord.`), and the new order's last six,
  the last first, before the walk's path and cluster.  With it the keepalives agree
  to turn 28; then the oldest peasant, sent from wood to gold, reaches the mine a
  step late: on its wood walk the original, past its first node (166.5, 189.5),
  heads straight for (183.5, 177.5), some twenty cells on (its place at 1.11 on that
  line), where the port's look-ahead stops sooner.  The cause is its tree (the
  original's search at 0.12, 101 passes, goes to (186, 174) by a way of 19 points
  ending (183.5, 177.5); the port's went to (192, 178)): the AI's wood workers go to
  their town's trees (`FUN_6f9c55d0` about each drop-off, `FUN_6f9c5950` judging each
  by `FUN_6f059590`, the plan's cost with both ends lifted, the eight cheapest kept
  and sorted), not the tree nearest the town (680 from the hall where the original's
  is 689).  The port takes the first of the cheapest by the accelerator plan from the
  hall (`World::ai_town_tree`; provisional: the area, and which of the eight a worker
  gets, read since: each the next in turn, below); the keepalives agree to turn 32.
- A structure's blight dispel does not run while it is built (read: the human
  computer's farm, begun at 2.1, its `Abli`, `Xbli`, `` `pos `` and two `^pmr` kept
  past 3.3, where the port had let them go as a pre-placed town hall's go 1.2
  seconds in).  With it the keepalives agree to turn 98.
- A worker out of a gold mine where another stands is placed as a builder put off a
  footprint is, the room its size class asks for clear (read, places and targets
  at 8.07 and on: the fourth peasant out at (161.5, 183.5), the oldest standing
  where it went in, the ring's earlier (160.5, 183.5) having its 2 by 2 below and
  left on the mine's column 159; the port had put it there and it walked home six
  steps behind).  With it the keepalives agree to turn 152.
- The peasant trained as turn 152 begins (15.1) takes its AI's harvests as the setup's
  peasants did, and what its making lets go goes then, before the turn's steps (its
  making's two tasks, its AI's objects, the `ptcf`), before a wood peasant's arrival
  at its tree in that turn's steps; and the AI's pass at 15.2 leaves it be: a worker
  its AI gave its harvest as it was made joins the harvest requests only from the
  pass after the next (the original's first wood order for it at 16.2).  Forestwalk's
  trained acolyte, given no harvest as it is made, is ordered in its first pass and
  its tasks and `ptcf` go after those orders at the turn's end, as before.  With
  these the keepalives agree to turn 162.  At 16.2 the AI sends that peasant to build,
  its setup harvest (18) still its run, and frees that harvest's eleven before the
  last target task as it does a re-ordered one's (turn 164 then agrees).
- A walker that comes within reach of its structure stopped and off its bearing
  arrives only within 0.2 of it, not within its turn: the fourth peasant, 0.925 off
  as it reached the mine at 16.318 (the provider's answer 3), turned 0.6, then onto
  the bearing, and joined the queue at 16.378, its path freed as turn 165 began.
  With it the keepalives agree to turn 177.
- A builder is placed off its structure as a unit made there is (with the room its
  size class asks for), not only when on the footprint: the farm's peasant, begun at
  17.608 at cells (170.15, 178.63), stood then at (171.5, 179.5).  An AI's peasant
  not then touching the footprint (`FUN_6f166310`/`FUN_6f148790`: a cell of the ring
  about its class's square, class 1 the 4 by 4 from two below and left of its cell,
  is the structure's; provisional: its cell its place rounded, the altar's peasant
  at (158.5, 196.5) touching, the farm's not) has a `tskt` before its order's
  target and searches (`^clg` freed at once), its walk the newest; its repair range
  tested (`FUN_6f41d1f0`: 50 over 32 plus the radii, 0.5 and 2.25) at 17.608 and
  17.668, the rise made at 17.668 (births traced with the clock) and the `tskt`
  freed, its path and cluster in the next step, turn 178's first.
- The computers' threads due before a wanderer's order's main tick run before it
  (the barracks' order from a thread due 18.1, main tick 3620, before a pig's order
  due at 3621, both before the turn's first step).  Its peasant walking home with
  gold (a 16-object return run) lets go its first eight, the last first, and, given
  before the tick's steps, its path and cluster as an ended walk's, in the next step
  (before an older walk's).  With these the keepalives agree to turn 218.
- A walker the AI stops for an order turns from its facing as its next walk starts,
  not from the heading it last arrived somewhere with: the barracks' peasant, walking
  home at heading 0.2074 when stopped at 18.1, turned three steps (0.6 each) and moved
  at 18.208 toward (166.5, 187.5), as the port's now does.
- Its way (read after the search, 25 points, the end first) is the port's, but at
  18.298 it heads for (167.5, 202.5), not the way's end: its line test
  (`FUN_6f168d30`, traced cell by cell with the class-1 test's answers) refuses the
  line to (168.5, 203.5) at its third cell, the step from (166, 189) into (167, 190),
  where the port saw nothing; another walker's line through (169, 187), the port's
  defense captain's cell, passes.  The captain stands in its home's cell, (167.5,
  189.5): a captain keeps clear of the other ground units' regions (not the made
  units' two-cell rule), stays at its home exactly when the cells its region would
  span there are clear, and else takes the first clear cell's middle from its own,
  ring by ring (a start peasant's region takes the home's lower cells; Forestwalk's
  captains keep the places read).  With it the keepalives agree to turn 332.
- A worker made in the tick of its AI's pass, before the pass runs, counts as made
  since it: the peasant trained by 31.2 sits out the passes at 31.2 and 33.2 and joins
  at 35.2 (its making's harvest not counted at gold, so it is sent again).  Walking
  home with gold then, it is given its walk home again, an order (`+ord`) at its head
  (`RETURN_AI`), its old walk home's first eight freed (the last first) and that
  walk's path and cluster as an ended walk's in the next step.  Home at 36.0 it drops
  its gold and stands (that order's first eight freed, its path and cluster in the
  next step), till its AI's pass at 36.2 sends it to the mine; a computer with an AI
  script has no other look for idle workers (the port's quarter-second one sent it at
  once).  With these the keepalives agree to turn 427.
- Its harvest pass due 35.0999985 stops that peasant walking home where it was at
  main tick 7021's clock, a fifth of a step past tick 7020's (its start 0.025 cells
  further on, read under winedbg; its walk then the port's step for step, so its
  arrival at the mine, its place there, and its later walks one step on).  The tick
  is the first whose game time, 5 ms added in f32 tick by tick, is strictly past the
  due: the passes due 2.0999999 to 15.0999994 at the ticks they round to, 33.1 and
  35.1 at the next (provisional; the threads' own ticks, `ai_tick_of`, unchanged).
  With it the keepalives agree to turn 508.
- Its build order due 48.0999985 stops the peasant it sends to build a farm 0.059
  cells (two ticks) further along its walk than the tick the due rounds to (read under
  winedbg).  One rule fits that, the harvest stops, and the build order due 18.1: the
  first tick whose main clock (the lagging single-precision one) is past the due less
  a tick, for both the build and the harvest orders' stops (replacing the game-time
  rule above).  With it the keepalives agree to turn 526; next, the farm begun at
  17.6 finishes in turn 527 in the original, not then in the port.
- The same game's wisps' walks, read under winedbg (the waypoint provider's answer
  after `FUN_6f165ae0` at 0x6f16fd05, the walker's object in edi; the mine's count
  changes, `FUN_6f4e95e0`): the setup's orders walk from turn 1 (answers from 0.03),
  and the AI's re-order at 0.1 to the same mine keeps each walk (the same walker
  objects and targets on).  The wisps enter at 0.27, 0.60, 1.23, and 2.10 (the port,
  re-searching at the re-order: 0.24, 0.84, and later).  The first, standing at
  (161.0, 188.44) in cells (origin -10240, -10240), heads first for (161.5, 189.5), a
  cell north (the port's search finds its own cell already touching the mine and gives
  it no way), answers 3 at 0.12 with the mine's middle (156, 184), and enters at 0.27:
  as a turn of about 2.8 from that heading at 0.6 a step, set at 0.24.  The scripted
  case (Booty Bay's night elf start) fits the same turn rule: facing 270 with the mine
  2.29 off, searched at 2.01, turning to 2.10, entering at 2.13.  Calls from
  `FUN_6f167290` (the blocked walker's countdown) test contact with the mine one step
  before the others' entries.  A search whose start never counts as reached (only
  nodes popped after it) gives the port exactly the original's first nodes for the
  two wisps beside the mine (161, 189) and (161, 185); the walk then turns in place
  toward its node, and `FUN_6f165c60` reaches `FUN_6f167290` (contact test, then the
  countdown) when its way's last point is within reach or the cells ahead are taken
  by a unit not moving and it is not waiting; contact answers 3.  The port's held-back
  test has no contact answer and sets the first wisp waiting at 0.09 where the
  original's fourth wisp moves freely.  In Booty Bay's scripted start the provider is
  not called at all for the wisps' walks (none from 2.0 to 5.0), so those take
  another steering path.  (Modeled since, below: the keepalives agree to turn 12.)
- The provider as read (`FUN_6f165c60`): the walker's next point (`FUN_6f167ce0`);
  within 0.49 of it, the next, or at its way's last, `FUN_6f167290` (`FUN_6f167070`);
  short of it, the cells a step toward it (`FUN_6f166140`, the point at most a cell
  on, `FUN_6f168280`), and if those are taken the give-way settled (`FUN_6f168360`:
  for a moving one there not waiting, the walker waits 4 steps, its +0xa8 set, when
  no faster, of its group, or of another class, and else that one waits 20); one not
  set waiting and not counting down (+0x94) goes to `FUN_6f167290`:
  contact first (`FUN_6f166310`: the ring about its cell by its size class, side 3
  to 6 from 1 to 3 cells below and left of it, `FUN_6f148790`, against its target's
  region), answering 3, else the countdown.  The first wisp's 3 at 0.12, the mine's
  middle its point, fits the cells toward that middle being the mine's own, its ring
  on the mine (class 1 at (161, 188): x 159 to 162).  Its entry at 0.27 then fits a
  turn from 0.229: three steps in place toward its node from 270 degrees (0.6 each,
  counterclockwise), then from 0.12 toward the mine's bearing (3.868), clockwise
  2.644 at 0.6 a step, on it at 0.24 (the last 0.244), entering the step after, as
  the scripted wisp's.  The port's own-cell way (no turning first, from 270) would
  enter at 0.18 by that rule.  (Read since, below: its setup's walk is to a tree,
  its re-order's to the mine from its own place.)
- Measured (each search's start, goal, and way after `FUN_6f166e90`, 0x6f166fbe, and
  each walker's place, heading, and path after `FUN_6f165ae0`, to 0.3): the five
  wisps' walks from their making go to trees, not the mine: from (161.84, 186.13) to
  (156, 170), (164.16, 186.13) and (165.0, 188.44) to (160, 170), and (161.0, 188.44)
  and (163.0, 190.13) to (148, 198), in cells.  So the first wisp's node, (161.5,
  189.5), is its way's to its tree, and it turns in place toward it, 4.7124 to 5.3124,
  5.9124, and 0.2292 at 0.12, its place kept.  The AI's pass at 0.1 sends four to the
  mine and one to wood ((152, 206)); each searches anew at 0.12, the first from its
  own place to the mine's middle, its way one point, that place (contact, 3), the one
  from (164.26, 185.57) straight along row 185 (four points), the one from (165.0,
  188.44) round the north of the mine (ten), the builder first in that step.  The port
  gives each computer wisp a tree as it is made, the cheapest by `World::auto_tree`'s
  plans, fog or none (`World::ai_setup_tree`; read so for four of the five, the one at
  (165.0, 188.44)'s (160, 170) a tie the port's order breaks for (156, 170);
  provisional: why the AI sends them to wood), passes over such a wisp at the wood in
  its AI's pass (`World::ai_assign_harvest`), and gives an order's walk its own path's
  spacing (the +0x7c is the path's, a new `l^pa` each order; `World::start_gold_run`);
  an own cell's walk to the mine turns then and goes in as above
  (`World::walk_steps`).  A walker stopped by an order between steps is no mover to the
  searches before its next (`FUN_6f15f7e0` through `FUN_6f1606e0`: its velocity none,
  its region's +0x40 loses 0x20000000): the wisp at (164.26, 185.57), so stopped,
  turns the other's search north (the port had excused it, `World::regions_taken`; the
  human computer's searches then pass its peasants by their ghosting).  The AI's build
  order's walk is the newest (`World::ai_build_objects`).  The keepalives agree to turn
  12, from 3; next, the third wisp in comes a turn before the original's, in turn 12
  (where the one at (165.0, 188.44) turns the other way at 0.12, its heading held
  toward its tree, not the original's).
- Why the wisps go to wood first: an AI's look at a unit (`FUN_6f9cd390`) takes a
  wisp (its Awha) to `FUN_6f9c96c0`, which, its AI unit not flagged for gold (+0x28's
  1 clear), orders it to harvest (0xd0003) the town's next tree in turn
  (`FUN_6f9c70a0`, the AI's +0x150 counted on, the empty places passed over, eight
  tries) or, the places all empty, the nearest (`FUN_6f4153a0`); flagged, to the
  town's mine (0xd0032, +0x54, else +0x60).  The five wisps' trees, two of them
  shared, fit the places being empty as they are made.  (A peon or peasant's look,
  `FUN_6f9c9470`, goes to a tree only when flagged for wood, `FUN_6f9c6cf0`, which
  counts the same +0x150 on each call.)
- Read on to 1.4 (each walker's place, heading, and path at 0x6f16fd0a): the wisp
  from (164.26, 185.57) is held at (163.7616, 185.4978) from 0.21 (answer 1 with its
  point, then four steps answering 1 with the mine's middle as its wait, +0x94,
  runs down), walks on from 0.36, and is in contact at 0.57 at (161.99, 185.50),
  in turn 7 as the port's.  The one from (165.0, 188.44), walking north from 0.24
  on its way round the north of the mine, at 0.33 at (165.3233, 189.1246) answers 1
  with its way dropped (count 0, index -1: `FUN_6f167290`'s countdown, so blocked by
  a standing region or a wall, not a mover), its searches refused at 0.36 and 0.39
  (answer 2, ten steps not yet past its search at 0.12), and at 0.42 searches from
  there along the diagonal south of it (seven points, through (163.5, 186.5) to
  (161.5, 185.5)), turns, walks from 0.54, and is in contact at 1.20 at (161.7553,
  185.6532).  The port's, never blocked there, walks on round the north.  Open:
  what blocks it at 0.33 (no other walker is near; the Tree of Life's footprint is
  166 to 173 by 190 to 197, its collision 144 a region from 165 by its cells' rule,
  which would have held it at 0.30 too).  Measured (after the collection's call in
  `FUN_6f166140`, 0x6f1662cc, at 0.33): that wisp's (its way 0xbdb7f08) collects one
  region (`DAT_6fd53a84` + 0xc8, 1), the two others' none: the computer's defense
  captain's (the region's words naming its `` `pcu ``, slot 27465, birth 27478, made
  with the second `AI-C` as CreateCaptains runs), the captain at (163.5, 188.5) in
  cells, its region in (163, 188), on the wisp's step west along its way's end there
  ((163, 188) and (163, 189)) and not on its step north at 0.30.  The port now holds a
  walker on a captain's cells as on a standing unit's (`World::held_back`,
  `World::captain_cells`), and gives a computer's wisp's first tree by the plan from
  the tree (`World::ai_setup_tree`; provisional, above): that wisp's tree is then the
  original's (160, 170), its heading at 0.12 4.4476, and its walk the original's
  step for step through its contact at 1.20 (blocked at 0.33 and 0.51, its searches
  at 0.42 and 0.54 of seven and six points).  Read with each object's step clock
  (breaks at the tag allocator's take and free, 0x6f1cbf40 and 0x6f1cbff0, 1.05 to
  1.35): the pass at 1.1 makes the wood wisp's new order at 1.099999 and frees its
  old walk's path and cluster at 1.109999, the step after, as an ended walk's (the
  port had them at the turn's end; now `World::start_gold_run` so frees a wisp's,
  as it did a walker's home); the builder's structure is begun at 1.169999; the
  builder's walk's path and cluster go at 1.199999; the night elf structures' three
  blight dispels' objects (`Abli`, `Xbli`, `` `pos ``, two `^pmr` each, 960 at 64
  every 0.08) at 1.200099, the tick after that step; and the third wisp, in contact
  at 1.20, goes in at 1.229999, its order's tasks after and its path at 1.259999.
  The keepalives agree to turn 13; next (turn 14): those dispels' objects, freed in
  the port at turn 13's last step (`World::update_blight`'s release as the turn
  goes, for one made outright).  Tried: each made outright at its last step on the
  main clock (`Tags::doom_at`, its making plus 15 intervals), the keepalives part at
  turn 10 (another made outright then freed early; not read which).
- A blight effect keeps its Area rounded to the 128 grid, halves up, and begins its
  steps as its structure is made outright, as one built does as it is done.
  `FUN_6f4904e0` (CEffectBlight's setup, from the growth's vf32 and
  CAbilityBlightPlacement's vf262) keeps the Area as (Area + 64) less its remainder
  by 128 (DAT_6fd723dc 64.0 and DAT_6fd723d8 128.0, set by `FUN_6f070d80` at
  0x6f017bf0 and 0x6f017c00), the point likewise (halves away from zero); each step
  (`FUN_6f490070`) counts one more DataA and ends the effect (`FUN_6f04c410` with
  `FUN_6f4901f0`) once the radius reaches that Area within 0.001.  The growth's vf57
  (`FUN_6f40b7c0`) starts its timer through `FUN_6f423610` (`FUN_6f491250`, the
  first step at once).  So a large dispel's or growth's 960 is 1024, sixteen steps
  of 64, and a small one's 768 twelve.  Read (breaks at `FUN_6f490070` and
  `FUN_6f491250`, Twisted Meadows' night elf computer's game): the four effects
  begun at clock 0 from `FUN_6f423610`, a step there and one each 0.08 on, the
  three town halls' (Abdl) to 1.2 and the entangled mine's (Abds) to 0.88; their
  objects freed at 1.200099 and in turn 9.  The port had counted a structure made
  outright on a turn's timer, its first step an interval on and to 960 (fifteen),
  which comes to the same turn but frees its objects at the turn's end; it now
  starts its steps on the main clock as it is made (`World::tag_unit`'s
  `blight_start`) and keeps the rounded Area (`blight::effect_area`), its objects
  freed in the step after its last as a built one's.  The keepalives so agree to
  turn 142, from 13 (the entangled mine's, made outright too, by the same count).
- An entangled mine is no town site of its own: the original's save of a night elf
  start (`nel.w3x`) holds the map's four mines' four sites, the first naming the
  entangled mine (the mine it covers made at the map's start with that site).  The
  object born after the entangled mine's tasks, where the port had made a site, is a
  fogged effect image (`+EIf`, CEffectImageFogged, its record's first words the Tree
  of Life's tag; its record not yet parsed).  The port made a fifth site for it,
  the duplicate site seen in its saves; it now makes the image, and its saves of
  Twisted Meadows hold twelve sites for its twelve mines.  (The take trace's code
  reads are not always the object's: it showed `+EIf` for the Tree of Life's `Artn`,
  which its save holds, and `+wgh` for an entangled mine's own `+w3u` in one run.)
- A wisp sent to the nearest tree (`autoharvestlumber`, by script, over LAN with the
  original; `abtest/nel2.w3x`, two of the five so sent): its order makes 19 objects,
  `+ord tskA tskA` and then a harvest order as the mine's but for its second block
  (no generic immediate order's seven); its `^clg` is freed at once, its first two
  blocks, its order, and the five before its closing block (1, 2, 0, 15 to 11) as the
  next turn starts.  The tree is CAbilityHarvest's (`FUN_6f426870`, `FUN_6f4153a0`
  within 2000, of the trees its owner sees, each judged by `FUN_6f439e00`): the first
  of the cheapest by the accelerator plan's cost (32 pops, the tree lifted), as a
  peon's next tree; the original's two wisps went to trees 580 and 466 away where the
  nearest by distance were 452 and 420.  It walks to the tree as a peon does; as the
  walk ends it makes the harvest's art (`+EIP`, its `` `pos ``, two `^pmr`), frees its
  order's last target task (10), and its path and cluster go as the next movement
  step starts; it gathers at the tree's middle.  No objects for its deliveries.  The
  keepalives agreed to turn 25: from 2.43 seconds the original holds the first lumber
  wisp, walking west past the gold wisps, three times (`FUN_6f1689d0` on its path,
  its goal 18 cells off, so drawing its wait).  A wisp's walk to a tree meets other
  walkers as any walk does (its region mask 0x02; the port had given it a peon
  lumber run's 0x10, past walkers).  The give-way rule (`FUN_6f168360`, read): for
  each moving walker in its cells whose own path waits for none (`FUN_6f1680f0`),
  the walker waits 4 steps for it if they share a walk's cluster (`FUN_6f1702a0`,
  the cluster's flag 8 clear), or it is no faster, or their size classes differ;
  else the other waits 20 for it; one with no cluster is passed by.  An arriving
  gold wisp's frees (its two target tasks, its spells' listeners) and the first
  wisp's payout pair come as the step ends, after the step's ended walks' paths
  (read: one arriving in the step a lumber wisp's walk ended, its frees after that
  walk's path).  A payout in the main tick of a wanderer's order comes ahead of it
  (due 56.616413, the order 56.620937).  The payouts' times are the engine's
  arithmetic within the clock's wrap: each the last plus 0.0001 plus the period, by
  `FUN_6f06fbb0`'s add, which drops the low bits (the original's 64 payouts to 110
  seconds read under winedbg, all so; in double precision the port had run 0.000107
  early by 64 seconds, its steps changing as the clock crosses 8, 32, and 64).  And a
  payout so near a tick's last main tick that its old pair's frees, events a
  ten-thousandth after it, fall past that tick goes on in the next tick's first step
  (109.952751 by 109.952782; 104.952461 by 104.955528 freed at once).  At the first
  nightfall (200 seconds) each unit regenerating by night alone (the Tree of Life and
  the five wisps) gains its life's modifier, right after its object of the change:
  the original's save a second after nightfall (`nel201.w3x`) holds each wisp's and
  the Tree of Life's two `` `rcm `` together among the change's objects (and the
  melders' pairs follow, six places later than the port's had them).  The port had
  not set the flag of a unit made without one (its making by day) but on blight.
  With these the keepalives agree for all 2253 turns recorded.  (Its second table's
  `+EIm` takes after the nightfall have their own birth count and do not enter the
  keepalive.)  The original's save of such a wisp
  (`nel.w3x` played alone to 30 seconds, `MeleeStartingUnitsNightElf` for player 0
  as `-loadfile` ignores the race preference): its Wisp Harvest's own fields after
  Dur, DataC, and DataA hold the tree's place and reference, the art's reference, the
  tree twice more, and its lumber timer (due, Dur, event 852417); one in the mine the
  entangled mine in its tail.  Written so (the art's reference none for a wisp the
  AI sent, which keeps the old harvest), Twisted Meadows' night elf computer's lumber
  goes from 115 to 165 between 120 and 150 seconds in the original's continuation of
  the port's save (the port's own game 175), where it had stayed at 115.
- A unit's own path (`l^pa`, the tenth of its first objects) carries in its first
  word's high half 0x1 for a computer's unit only while it stands: Twisted Meadows'
  computers' walkers read 0x122 (0x1200_0000 for a gold run's walk, 0x2 moving), those
  standing 0x3.  The port set 0x1 for every computer's unit that moves, and the
  original, loading its save at 120 seconds, left the orc computer's three walking
  peons where they stood (their clusters counting 1002 steps by 150 seconds, the
  orc's gold up 120 where the port's own game's was up 300).  Written as read, they
  walk on, the orc's gold up 290 by 150 seconds.  Not the cause (tried): the position
  cluster's flags (1, 0, as the port writes for a gold walker) and the order's third
  word (0 as the port writes; the original's own orders 0, 1, or 2).
- The original's saves hold no state of its random generators (neither the main one
  nor the 45 streams, as seeded or by value, under the locked seed or the slot
  table's): the original goes on from a save with generators of its own, so neither
  game's continuation draws as the uninterrupted game.
- A unit's abilities that a script added are its own objects in the save, as its
  type's are (an ability object's kind its code, its id its own tenth word, its
  level less one the eleventh): the melee setup's Militia Conversion on a human
  start's town hall (`MeleeStartingUnitsHuman`'s UnitAddAbility; made after all the
  start units, Echo Isles' tag 15051 in both games).  The port's load makes units
  from their types, so it lost these; it now adds each saved ability its unit lacks
  (not an item ability, nor one its carried items give, by id: the hero map's
  Death Knight's Sentry Ward is its Sentry Wards' and on the code Aeye).  A town
  hall made again reruns its Blight Dispel (1.2 seconds) after the load.
- A gold mine's gold is its Gold Mine ability's (`Agld`) first own field after the
  ability's thirteen (the hero map's 11000 and 14000), and a haunted mine's
  (`Abgm`) too, then the mine it covers (9760 over its mine); the port had loaded
  every mine full, and the undead computer's haunted mine with none, so it
  collapsed and its acolytes mined nothing.  Loaded, the computer's gold over the
  next 150 seconds is the original's (1490 against 1450).
- A captain's order (its tail's 51st word from the end): an attacking captain's
  AImove (851988) to its goal, one going home move (851986), its goal where it
  stands, as CaptainGoHome leaves it (the hero map's at 450 seconds).  A ghoul's
  Cannibalize order carries flag 8 and refers to the order under it, 851974
  (0xD0006, no name: the internal stop Cannibalize issues once its life is full).
- A script's SaveGame saves once a session (`jass_SaveGame`: nothing in a network
  game, for an empty name, or while the game's flag at +0x3e4 is set; it sets it,
  and a new game's start, `FUN_6f1e05a0`, and a load clear it): the original's own
  game saving at 60 seconds and again from a trigger woken at 80 writes only the
  first, and the woken thread runs on; a game loaded from the first save, or one
  that never saved at 60, writes the second.  A trigger fired by a timer event holds
  the event's own timer in its context, which scripts never had: the port's handle
  table had counted it as a handle, and with that timer apart from the handles'
  numbers a save during such a trigger's sleep asked for 52 GB.  With both, the
  original loading the port's save of 61 seconds saves from the woken trigger.
- A script's timer (CTimerWar3) runs on the main clock (the original's 320-second
  timer fired as the main clock read 20.0 in its second wrap; the header's game time
  then 320.555).  Its agent timer (CAgentTimerExtended) runs a run of 120 seconds or
  more in 120-second chunks: +0x14 the chunks, +0x16 those left, +0x18 the rest,
  flag 0x10 while in them, a periodic node of 120 (vf2 `FUN_6f0602a0`); each chunk's
  firing counts +0x16 down (vf3 `FUN_6f060d40`), and as the last passes the flag
  drops and a node for the rest is made, due then plus the rest, newly numbered on
  the clock (read: the hero map's 320-second timer at 200 seconds due 240.0 every
  120 with 2 chunks and 1 left, its start's count; at 250 due 320.0 for 80.00001,
  none left, count 45906).  An expired one keeps its chunks, none left (the save
  timers of 300, 450, and 900 seconds: 2, 3, and 7).  A node's due is in the clock's
  current wrap (a periodic 30-second timer due 150.00009 at 450 seconds, the clock at
  150.0 in its second wrap; a due beyond 300 stands until the clock wraps), and a
  periodic node keeps its count across its repeats.  The port wrote every running
  timer's node as at its start (a periodic one's first due, a chunked one's first
  chunk, and no wrap), so a script's timer of 120 seconds or more, or any past a
  wrap, fired late or not at all in the original after loading the port's save; it
  now reckons the node from the run's start (`World::timer_node`), and reads a
  chunked one's time left from the chunks left and the rest.
- Mafarazzo TD's save at 30 seconds (`abtest/mafsv.w3x`, its own SaveGame; compared
  object by object with `w3zdiff`), the kinds the port's save had left out, each read
  in its class's save (vf14) and own (vf18) functions (`tools/saves/classmap.txt`):
  - A region (CTriggerRegion, `+agr`, `FUN_6f05fea0`): no own fields and only the
    agent base's handlers (`FUN_6f0562b0`, no lists of events 0xd0000 on), then the
    lists of events 0x401900c8 and 0x401900c9 (its enter and leave registrations, each
    with its event) and its pathing region (+0x20).  That region (`^pmr`, made with it)
    is on the pathing map, owned by the region, of no kind, its bounds all -1, its
    words the search mark, the count of its cells, and 0x10000000; RegionAddRect's
    cells (a rect's from its least corner's cell to its greatest's, both in: a rect 32
    wide two columns) each list it, newest first among the others there.
  - A region's registration (CRegionEventReg, `+rev`, `FUN_6f27eb00`, own
    `FUN_6f27f140`: five words): its region, its filter, a timer (CAgentTimer, +0x3c)
    and the queue of units met (+0x50, `FUN_6f271ae0`: each a reference and a word),
    its trigger on the list of event 0x80205 (enter) or 0x80206 (leave).  A unit's
    move asks the filter, queues the unit, and starts the timer of no wait
    (`FUN_6f277bd0`), which runs the queue (`FUN_6f2777c0`): between turns both empty.
  - A trigger's condition (CTriggerCondition, `tcnd`, `FUN_6f299690`): its
    expression's object (+0x24); its trigger lists it.
  - A chat registration (CPlayerChatEventReg, `pcvt`, `FUN_6f27e820`, own
    `FUN_6f27f190`): the player event's ten words (event 0x80260, its player) and
    whether the text matches whole; the player event's filter and list, its text, and
    its trigger on the lists of events 0x80210 and 0x80260; its player lists it on
    both too.
  - A leaderboard (CLeaderboardWar3, `+ldb`, `FUN_6f246530`): its frame's
    (`FUN_6f3cb7a0`: its size from there, a short count of items, each
    `FUN_6f3a6b00`'s label, value, whether its label, value, and icon show (+0x58 to
    +0x60, a new one the board's +0x188 and +0x18c and no icon, `FUN_6f3aaa30`), its
    player, and its label's and value's colors as ARGB (the player's,
    `FUN_6f343df0`, where none was set: red 0xffff0303), and whether the frame shows),
    its size by item count (the frame's +0x194, LeaderboardSetSizeByItemCount's,
    `FUN_6f3ce9a0`), whether displayed (+0x28, `FUN_6f2499d0`), and its title.  Each
    player it is shown to names it (CPlayerWar3 +0x2d4, the record's 139th and 140th
    fields after the event lists).
  - A camera setup (CCameraSetup, `+cst`, `FUN_6f246030`): its place (+0xa4, +0xa8)
    and 0 (+0xac, CameraSetupSetDestPosition's, its duration elsewhere); its seventeen
    reals in the second table (after the camera's, from 1096): the distance, the far
    clip, the near clip (100 unset), the field of view, the roll, the rotation, the
    angle of attack, and the height offset as set (`FUN_6f2477e0`, in degrees, a 0 as
    -0), their eight durations, and the place's (the roll's and height offset's places,
    both 0 here, and the durations' order provisional).
  - A computer's guard post (CGuardPost, `AIgp`, own `FUN_6f9befe0`): a word, its place,
    its unit type, its unit, 0, the units it has had, and the post made before it; its
    player's AI names the newest (CCompAI's own, after its towns), the unit's AI
    (`AI-U`) its own post (+0x1c) and its flags 0 (0x60 and its job only for a
    computer out of its campaign mode, provisional), and the unit lists it among its
    listeners on events 0xd01a0 and 0xd01a2, after its player's AI.
  - The time of day suspended (CGameState +0x6c, its record's 26th field after the
    event lists: SuspendTimeOfDay's bit 1, `FUN_6f28bb00`): its modifier gone, the
    time's real's rate 0 and its first link none, its listeners on the time checking
    nothing (no node).  The port's save had the rate 0.05 and no bit, so the original
    ran the time on from it (night by 270 seconds).  The day change's thresholds are
    the engine's arithmetic (`FUN_6f283c80` by `FUN_6f06fa90` and `FUN_6f06fbb0`:
    dusk's 18.009998).
  - The fog's two bytes are its mask's (+0x10, FogMaskEnable, `FUN_6f24f9c0`) and its
    fog's (+0x14, FogEnable, `FUN_6f24f930`), both off here.
  - A trigger's words: +0x58 its evaluations (an event's, counted before the enabled
    flag is asked, `FUN_6f299c90`, and TriggerExecute's, `FUN_6f2979a0`; never
    TriggerEvaluate's), +0x5c its executions, +0x60 those running, +0x74 its
    conditions' last result (`FUN_6f296de0`: an event's and TriggerEvaluate's, not
    TriggerExecute's), +0x78.  Its conditions are all evaluated, passing if all pass
    (`FUN_6f296820` in mode 2, a condition without an expression failing).  Read:
    WalkCenter evaluated once and executed none for the peasant made in its region;
    triggers run by another's TriggerExecute 1, 1, 0, 0.
  - A group (`FUN_6f2464b0`): after its units, its last enumeration's filter (+0x38)
    and its last ForGroup's function's handler (+0x3c, `FUN_6f22ff30`), then +0x40
    (-1), the player its last enumeration took or -1 (+0x44), its limit or -1 (+0x48,
    `FUN_6f240890`; 0 and 0 before any), and 0.
  - A timer dialog's string is its title as set.  A timer's handler without a
    function numbers 0.  An execution kept within SaveGame names no action under way
    (+0x2c), its actions listed.  A unit's speed in its move ability and position is
    as SetUnitMoveSpeed set it; a script's move order names its unit's owner (its
    third word); a unit whose pathing a script turned off lacks agent flag 0x40, its
    walking byte, and its position's first flag, its position's first real 0.49, its
    cluster's word 0x20000, and its path's owner word 0x200 (provisional); a collision
    of none is one in its position and its path.  A walk's path's long way names the
    corner it walks to, from the goal's 0 (3 of 8 halfway down the creeps' road).
  The original loads the port's save so written (`savetest/mafsv30_v1.w3z`) and
  plays on as from its own: daylight, the leaderboard shown, the creeps walking
  their route, the lives falling, the defeat dialog by 270 seconds.
  - A player: its race preference (+0x25c: a map fixing its players' settings, w3i
    flag 0x20, its race's bit alone; the neutral players 0x60), its start location
    (+0x274, an empty slot of the map's its own) then its team (+0x278, none unless
    playing: the port had written them the other way, Booty Bay's alike either way),
    the players whose sight it shares (+0x2e0, `FUN_6f26b3d0`: 0x7ff for the map's
    eleven allies), an empty slot's name none and its agent word 0.  A start location
    whose player's start the w3i fixes is taken and fixed (`FUN_6f1dbec0`'s +0x18 and
    +0x28, 1 and 1).  The game's unit name table follows the standard types with the
    map's own as "custom_" and their ids, in its w3u's order.
  The port writes all of these (`w3zsave/script.rs`), and loading an original's save
  makes its leaderboards, timer dialogs, and camera setups again under their handles
  (`Game::restore_boards`), the players shown each board too.  Not yet: the message
  log (the game's third part's first list: each text shown), the local player's
  interface's six `` `fcl `` and the creep camps' `+ccu` in the second table, the
  game state's fifteen context stacks and free handles (tail 10 and 11), a walk
  without pathing's unit's own path (the original's a partial way of some 32 cells,
  its cluster's local goal that way's end), the slot table's empty slots, the map
  setup's flags 0x300000.  The single-player game's draws: the original's creeps'
  speeds are the port's shifted by 23 draws (its first creep's the port's 24th), some
  draws at the start the port does not make (the network game's agree).
- A unit moving among others has its position cluster (`` `pcu ``) end in its
  separation's record (CPoSeparate, `FUN_6f16f9d0`: two reals and a word), its
  second flag set (read, the hero map's 18 dragons on the move).
- Life and mana are stamped on the agent clock, which wraps at 300 seconds; a stamp
  before the last wrap counts from there (the hero map's Dreadlord at 300 seconds,
  its stamps near 290 and the clock at 0.45).
- A hero's abilities made with it carry 0x80 in their first word (a Dreadlord's
  Amov, Aatk, AHer, and AInv), its learned skills 0x10 (Sleep 0x12 where a creep's
  is 2), and its attack lacks 0x4000 and 0x2000000 (0x20080092).  Its attack's
  weapons both as the data has them, enabled or not (the Dreadlord's second, a
  missile of 500, with `weapsOn` 1; every single-weapon unit read has a blank
  second), each damage with its primary attribute (20), and its speed factor 1 +
  agility × AgiAttackSpeedBonus in the engine's arithmetic (1.3199999 for 16; the
  engine reads "0.02" a step up, so its hero ability's 16 × 0.02 is 0.32000002).
- A learned skill makes its ability's objects as a unit's making does (Sleep:
  `AUsl` and its listener on mana, event 0xd01b7 below its cost less 0.1 in the
  engine's arithmetic, 99.899994).
- A structure being built (read: the Crypt at 2.5 seconds, a Graveyard at 160): its
  Build In Progress ability (`ABnP`, after its other abilities in its chain; own: 1.0,
  its build time, its builder's build ability, 0, eight 0.0; its listener keyed to
  event 0xd01c4; tail: its listener, its rise's modifier, 60.0, its life's modifier,
  two timers), that's real (the build time at the start, falling 1 a second), the
  rise's modifier (-1 for the build time and a second more, 0x60040044) and listener
  (0xd01c4 at 0, a node every 4 seconds), and the structure's life modifier (its HP
  × 0.9 over the build time in the engine's arithmetic: 19.499998 for a Crypt's
  1300 over 60).  The Graveyard's five `+EIP` are its own effects, not
  construction's (not yet written).
- Items (CItem, read: the melee start's Town Portal scroll held by the Dreadlord,
  and Claws of Attack on Booty Bay's ground): the agent with its position, own flags
  (held 0x2757, on the ground 0x3416), the game's handler, its sprite (the inventory
  model; hidden while held), owner, type, shorts (0, 0x8001 held), -99999, life, 0,
  HP, charges, its first ability (AItf for Claws, whose cooldownID is AIat), its
  holder, 0, 15, its abilities' objects (four places), two timers; its position
  facing 270° with 1/32, and regions (held: its block flagged 0x0c000000, its cell
  none with 0x18; on the ground: block and cell, 0x01000018).  Held, its abilities'
  objects are its holder's (the scroll's `AItp` and its listener on mana at -0.1,
  keyed to the ability), in its chain by birth; its inventory's slots name it.  The
  original keeps the scroll's owner 15 while its Dreadlord holds it, where the port
  gives it its holder's owner (not yet resolved).  The `AItp` record: CAbility's
  fields, its casting time, three 0, its item, three 0, none; ten 0, 852290, four 0,
  its listener, two timers, three none.
- Units' orders (read against Forestwalk's undead computer at 2.5 seconds).  An order
  is an agent of its own (`ordt` targeted, `ord.` at a point, `+ord` plain), not on
  its unit's chain; its tasks are agents on the unit's chain after its abilities,
  each naming the one before (own: 0, that one, 0, its event), the last the unit's
  current task.  The unit's fields after its sight: its current task, its timer, its
  order, and the order after it; its agent flags lose 0x200 and its flags word 0x1
  while it has an order.
  - An acolyte at work in a haunted mine: `ordt` (0, harvest 852018, 1, none, 0, 0.0,
    0.0, the mine, five 0, 0.0, 0.0; none, 0, two none), `tskA` 852322 (and 0), tasks
    852326, 852298, 852296, and `tskt` 852355 on the mine (0, 0.0; a none); its next
    order the same.  Its harvest (Aaha) listens for 0x1a2 from then (after the AI);
    its sprite's current sequence 0xfffe with Stand Work Gold (4) at 0x400; its state
    word 6, the words after its tint 2, 4, 35, and its last word 0x80 more.  (The
    port keeps these six as `ai_order`: `AI_HARVEST`'s fourth to ninth.)
  - An acolyte summoning a structure for its AI: `ord.` (0, the type, 1, its next
    order, 0, the site), the next `+ord` (851974), `tskA` 852322, tasks 852296,
    852298, 852292, and `tsk.` (the type, the site, 0.0; a none); its timer the
    order's end (event 0xd019d, a second on); its sprite's flags 2, 1, 0 with Stand
    Work Gold from 0; its state word 2, the third word after its tint 37.
  The original loads saves with these and plays seven minutes.  Not yet written:
  a walk's cluster and path (`^clu`, `l^pa`, so a ghoul walking to its tree and a
  player's workers stand), the player's harvest orders (`goldrun.rs` keeps their
  objects), and the running animation clocks and counts.
- A player's workers mining gold (read against Forestwalk's human peasants sent to mine
  by script, saved at 12 seconds; the test map `fwhm`):
  - The worker's own harvest order from the hall (`ordt` 852018 at the mine's place,
    flags 0) with `tskA` 852322, tasks 852326, 852298, 852296, 852306, target tasks
    852369, 852355, 852354 on the mine; walking to it a task 0xd0151 (852305, its
    ghosting's end) and the walk's target task 0xd0171 (852337) more.  Its order
    home (`ordt` 852017 at the hall) with `tskA`, tasks 852326, 852296, 852298, 852306,
    and target tasks 852358 and 852340 on the hall.  The unit's state words: its
    work 5, 1 in a mine, a byte while it walks, after its tint 2 carrying, 5 walking,
    35; 0x80 on its last word in the mine; in the mine its flags 0x7 and the game's
    handler kept.
  - A walk is the walk task's: a cluster (`^clu`: its walker's position, the goal in
    cells, its speed and heading; the target's position; the goal; counters; a
    heading; 0x21000; the target's second footprint region and the walk's path) and
    a path (`l^pa`: the goal thrice, an owner word ((0x46 | 0x8 local) << 4 | owner)
    << 16, the collision in cells, no short way, the long way in half cells goal
    first, the target's region).  The walker's position names the cluster and holds
    its velocity, its current speed, its heading, flags 1, 0; its own path holds the
    goal, 0x12000000 in its first word, 0x10000010, a local way (cells, from its end
    back to the walk's start) and the long way; walking home its movement names the
    hall and listens to the hall's 0x1a2 and 0x1a4.  Without its walk a walker
    stands after a load.
  - The run on the worker: its harvest's flags 0x80000010 | 0x40000000 (been in) |
    0x1 | 0x800 walking or 0x9000 inside, its hall, its load (the fifth word), its
    speed bonus, last travel time, and bonus before, its mine, and its round trip's
    timer (10 seconds, event -1); its movement's speed factor (1 + bonus) and 0x4
    walking; its position's speed by that factor, 0x4000, and a first real 2.0625
    (6.0 going home); its fine region ghosting 0x08 with 0x20000000, at its mine 0xda;
    its repair 0x100000 more; its attack 0x80 more and 0x2000000 none (as any unit's
    with an order).
  - The mine: its shaft (`shft`) the worker in it, 1, and its time to come out on
    the mine's ability (event 0xd01c1, the mine's time); its ability 1, the player it
    shows the mine to, and its fog modifier (`+fgm`, 72 bytes: 0x1d4, 1, 600.0, ten 0,
    the mine's place, 0, none).  Not yet made: a countdown on the mine's real (10
    falling for 11 seconds from the first entry).
  - The local player's units have 0x200000 more in their sprite's word (some others
    too, far creeps among them: not yet told apart); its own units' widget shorts
    1, 0, a unit it sees 0 second (the gold mine 4095, 0), else 0x8001.
  - Read again at 3 seconds (the test map `fwh3`, the port's tags the original's
    there): a worker waiting at the mine has the tasks of one inside, its harvest's
    flags as inside (0x9000), and its place in the mine's queue, a `uref` on its
    chain after its tasks naming the next waiting worker's (words 0, next, 0); the
    mine's ability names the queue's first and last (its fifth and sixth words'
    triples).  The harvest's hall is none until the worker first carries a load
    out, and in the mine it carries nothing yet (the port's run now keeps the last
    hall).  The original loads the port's save there and every peasant mines on.
    The order the map's script gave is the player's form (two blocks at its head,
    the first gone), where the port makes the worker's own (one block), its later
    objects a place off (not yet changed: the port's script orders all take the
    worker's form).
  With these the original loads the port's 12-second save and every peasant goes on
  mining (the gold rising as from its own save); with the orders alone they stood,
  and without the mine's shaft the walkers went in and stayed.  Not yet read:
  lumber, and the counters and turn values of a walk.
- The map script's point and target orders are a player's: the original's objects for
  Forestwalk's peasants sent to mine and to walk by script are a player's harvest
  (two blocks at its head) and a player's walk (17 objects), born where a player's
  would be.  The port now issues them as a player's (its immediate orders have their
  own way; the computers' AI scripts give none), and with that its objects at 3
  seconds are the original's but for a few late ones.
- A walk to a point under a move order (read against peasants sent off by script, 3
  seconds in; the test map `fwmv`): the order (`ordt`: 0, move 851986, 0, none, 0,
  the point, none, five 0, 0.0, 0.0; none, 0, two none), `tskA` 852322, `tskO` 852326
  with the order, tasks 852296, 852298, 852325, and the point task `tsk.` 852331 (the
  point, 0.0; a none); its walk the move list's last two (a cluster with its local
  goal and the goal, no target, 0x20000; a path with the goal, the owner word
  ((0x206 | 0x8 local) << 4 | owner) << 16, the long way's corners).  The unit's work
  7 and 0x20000000 in its agent flags; its movement's 0x80, 0x800, and 0x4 (0x894),
  without which the walkers reached the point and never finished their orders; any
  ordered unit's repair 0x100000.  With these the original loads the port's save and
  the peasants walk on to the point and go idle there, as from its own save.  The
  original's peasants there stood jammed (velocity 0, its own path naming the one it
  yields to) where the port's walked on: a difference in the walk, not the save.
- A worker chopping a tree (read against Forestwalk's peasants sent to the nearest
  tree by script, 12 seconds in; the test map `fwlm`): its harvest order on the tree
  (`ordt` 852018 at the tree's place, naming the tree) with `tskA` 852322, tasks
  852326, 852296, 852298, 852306, and target tasks 852357 and 852356 on the tree
  (the player's form a place on, as for gold).  Its harvest: flags 0x40003185 more,
  2 its third word, its load its fifth (each chop's lumber as it lands, where the
  port adds it as the load fills), the tree twice in its tail, and its next chop's
  swing's part on its swing timer (the damage point, event 0xd01b1; the backswing,
  0xd01b2, its flags 0x4000 more then; the pause before the next swing, 0xd01be),
  each part's length its period.  Its state words 3, 4, 19, 36 after its tint and
  0x80 on its last;
  its position's first real 4.125 and 0x4000; its fine region 0xda; its own path
  the tree's cell last, 0x100000 more, 0x12000012, ways empty, the tree's third
  region (its first footprint region).  The original loads the port's save and
  the peasants chop and carry on, as from its own.
- A rect the script removed went on holding its object in the port (RemoveRect let
  its handle go only), and a save then had the rect's object without its record:
  the original's loader hung in CRectWar3's read.  RemoveRect now frees it as
  RemoveLocation does.
- Walking to its tree (read: Forestwalk's peasants at 19 seconds, `fwl2`): the
  order on the tree as when chopping with the walk's target task 852337 on it more
  (its real 116.0, the reach), then the walk's `^clu` and `l^pa` (the walk at the
  order's places 15 and 16); its harvest flags 0x40000981 more and the tree twice
  in its tail; state words 1, 5, 36, 36 after its tint, no walking byte; its
  position's first real 4.125 and 0x4000, flags 0, 0; its fine region ghosting as
  a gold walker's.  Home with its lumber (read: `fl15`, 15 seconds): the gold walk
  home's order, tasks, walk, and own path exactly; its harvest as walking to its
  tree with its lumber the fifth word; state words 2, 5, 36, 36; its position's
  first real 6.0 as with gold.  The original loads the port's saves at 12, 15, and
  19 seconds with no worker idle, and its lumber after 30 seconds' play from the
  port's 19-second save is its own save's (400).
- The player's gold and lumber gathered (states 15 and 16) are kept in tenths, as
  gold and lumber are: GetPlayerState (`6f201ad0`) divides all four by ten.  The
  port saved the two gathered as they are, a tenth of the original's.
- Several workers on one tree (Forestwalk's five peasants sent to one tree, `fwlm`):
  the port took each worker's load from the tree as it stopped, and the loads that
  emptied it left it standing with no life, its pathing kept: the next worker home
  searched for a tree from its cell, found no way, and stood idle.  The emptying
  load now fells it (its pathing lifted, its regions freed).  Provisional: the
  original takes each chop's lumber from the tree as it lands and fells it at the
  chop that empties it (`FUN_6f405110`), the others' chopping stopping short (read:
  four full at 13.84 to 15.34 seconds, the tree felled under the next round about
  22.5).
- Not yet as the original, read with `abtest/trarrive.sh` (each tree arrival,
  `FUN_6f4377d0`; chop start, `FUN_6f436400`; tree given up, `FUN_6f3f6840`; next
  tree, `FUN_6f437fb0`): the order of the five peasants' arrivals at the one tree
  and their times (the original's chopping from 2.94, 3.15, 4.26, 4.44 seconds; the
  port's earlier, 29389 from about 2.45 against 4.44).  The fifth, 29323, arrives
  at 5.94 out of reach with no retries left (flags without 0x1) and gives the tree
  up at once; after the half second's wait (task 0xd0167, 500) it searches about
  itself for the next tree (event 0xd0185, the task under the arrival's 0xd0184
  and the walk's 0xd0171 that every harvest order pushes) and takes another, own
  order 852018 on it.  An arrival in reach but not yet free walks to reach the tree
  with three retries (+0x19c), each after a half second's wait; the port models the
  reach walk, not the retries nor the giving up.
- Longer computer games (Forestwalk's hero map saved at 300, 450, and 600 seconds,
  `fh300`, `fh450`, `fh600`, the original's own beside each): the port's 300-second
  save hung the original's loader in an auto-target spell's read (`FUN_6f41e200`,
  `FUN_6f055f80` looping on a count read off the record's end).  The port wrote empty
  records for the abilities it had no layout for, and the original's reader ran off
  them.  Written now, each read with `w3zobj probe` against the decompile: Web
  (Faerie Fire's layout), Spider Attack (the ability's own fields alone), Burrow (a
  morph, CAbilityMorph's `FUN_6f4eb500` and `FUN_6f4ea410`: the spell's first
  fields, two reals, DataA and UnitID, a real and two words; the spell tail, a
  reference, five timers, `FUN_6f224e20`'s state; then its look every second, vf50
  `FUN_6f606200`, event 0xd01bf, started as the computer's unit joins its AI as
  Cannibalize's is), Death Coil (`FUN_6f42e2b0`: the spell tail and two timers),
  Allied Building (CAbilityAllied `FUN_6f42dd20`: twelve references, one, twelve
  triples, its `` `pil ``, a timer), Make Items (Sell Items' with a word more), and
  the Meat Wagon's Exhume (its corpse every Dur from its making, event 0xd01c3,
  repeating), Meat Load and Meat Drop (spells), Plague Toss (the buff base's tail),
  and its cargo hold (the Devour cargo hold's).  Their target masks and flags as read.
  `w3zobj dumpall` lists every record for such comparisons.
- A spell's listener on mana, re-armed as the mana passes it or let go as the creep
  sleeps, stayed named by the spell in the port, its slot then another's: the save
  named an object it did not hold.  The spell's own reference now follows it.
- The main clock and the agent queue's clock are set back by 300 seconds as the main
  clock reaches 300 (`motion::WRAP`), and the original saves on them: its agents'
  timers and its properties' dues set back as many times (read: at 450 seconds both
  from 150), its properties' time stamps the main clock as stamped.  At 600 its save
  timer fires as the clock is set back the second time, its agents' timers due from
  0.06 and its properties' still from 300 (theirs set back as the next step begins).
  The port's writer reckons most dues on the game's time; those due now or later
  are set back as the clock has been, once all records are made (a gold run's are
  on the agent queue's clock already).  Each object's making keeps the times the
  clock had been set back then (`Tags::made_wraps`, `World::made_abs`), and the
  writer reckons from it.  The port's main clock falls behind the game's time (to
  298.945 at 600.21 seconds), so its 600-second save is before the second wrap.
  The object manager's two clocks are saved as they stand, each with the times it
  has been set back (read: at 300 seconds main 300.0 not yet, agent 0.45376 once; at
  450 main 150.0 and agent 150.66518, once; at 600 main 0.00009 and agent 0.8862,
  twice); the port wrote the game's time and none.  Now the main clock as the port
  keeps it, and the agent queue's the game's time set back as often, the dues the
  writer reckons on each.  Provisional: the clocks' last words (the queues' counts,
  read 49867 and 367 at 300 seconds), 0.
- A structure under construction past its listener's checks hung the writer: its
  check stepped by the time left, none left, the same due again.  Its making's time
  was the main clock, set back by then; reckoned on the game's time now, and the
  loop ends when no time is left.
- `W3Z_TRACE`: the writer's steps and each unit as it is written, to stderr.
- At 900 seconds (`fh900`): the Necromancer's Cripple and Unholy Frenzy (the spell's
  fields, `FUN_6f42e2b0`'s tail), and the melee defeat dialogs, the human player's
  base gone.  A dialog (`+dlg`): a word, the game's handler (event 0x80267), its
  buttons, three words.  A button (`+dlb`, CDialogButton `vf14_6f246090`): its text
  as the string has it ("|CFFFFFFFFQ|Ruit Game"), its hotkey, event 0x8025b's list
  (the registrations of its clicks), and its quit and score flags.  A button's event
  (`bevt`, CDialogButtonEventReg `vf14_6f27e430`): five words, its button, event
  0x8025a's list (its trigger).  The port's quit buttons took no object (the
  original's "Quit Game" is a `+dlb` as the others), and its GetLocalizedHotkey gave
  the string's first byte ('|'); now the letter the string sets off in color (67,
  81 as read; the original reads a "HotKeys" table, `FUN_6f047710`).
- An in-range listener's (`` `pil ``) units are saved with the array's room (bytes
  twice, 32, its places): 32 places, doubled as more are met (read: [768, 768, 32,
  64] in the original's).  The port always wrote 32; with more units about than that
  (41 at 865 seconds) the original read them past its room, overran its heap, and
  hung later in the second table's first records (in Storm's reallocation from the
  agent reader, `FUN_6f146390`).  Found by tracing the original's agent first-record
  reads (`abtest/trfirst.sh`: each read's stream position, aligned with the save's
  records by `w3zobj lens`) after a bisection by save time (`abtest/loadquick.sh`).
- A path's ways are arrays with room too (128 points of 8 bytes, as read); the port
  now doubles it as an in-range listener's for a way longer than that, which the
  original would read past its room.  A haunted mine's ring slots (`rslt`) kept by
  the port after the mine was gone (mined out by 1800 seconds) are left out, their
  records otherwise empty (the original's loader hung on them).  The port's saves
  at 1200 and 1800 seconds load.
- Every melee unit's abilities (a test map making one of each unit of the four races
  and the creeps for the player, heroes at level 10 with their skills, saved at 5
  seconds: `fwall`, and one per race): the port wrote about 130 kinds of ability and
  buff empty, each a hang in the original's loader.  Their layouts are now read from
  game.dll and fitted to the original's records (`tools/saves`): each class's code
  from its vf7 (`classmap.py`), its own fields (vf18) and save (vf14) translated
  from the decompile into layout strings (`xlate.py`: the writing primitives, the
  base saves already read, references, counted and fixed loops, writes gated by
  build; a timer plain or extended as fitting settles), fitted with `w3zobj fit`
  against every record of the kind (`fitall.py`).  Ninety share the spells'
  families (`ability_family`); the rest are written from the original's record of
  each at rest (`gentemplates.py` makes `w3zsave/kinds.rs`): their base fields the
  port's, a spell's mana listener its own, a companion reference the unit's object
  of that kind, a repeating timer restarted from the save's time, lists of art
  emptied, other references none.  Provisional where no record of the original's was
  read: Robo-Goblin, the healing ward's aura buff, the poison buff (its second timer
  extended, as the original's reader showed by reading 8 bytes past the plain one,
  `abtest/trfail.sh` at FUN_6f15c390's length check).  Target masks from the data's
  `targs1` (`data_targets`).  The orcs', night elves', and neutral's build abilities
  have the undead's three words more.
- The rest of the ability and buff classes (362, no record of the original's read
  for them): their layouts from the decompile alone, each timer's kind by the
  class's constructors (the vtable put at its offset, CAgentTimer or
  CAgentTimerExtended, through the base constructors called: `xlate.resolve`; this
  reproduces all 108 layouts already fitted whose timers it settles), into the
  formats crate's table and `w3zsave/kind_layouts.rs` (`genlayouts.py`).  Their
  records at rest are their layouts read from zeros (`default_fields`: no timers,
  no references, lists empty), a spell's order and listener its own.  Translations
  that branch around writes are refused (`xlate.risky`), with the two forms of an
  optional reference understood.  The engine's own class codes (CAbilitySpell's
  AAsp and the like) are named in `codes.rs` by class; the generators write every
  code by its name there (`codenames.py`).
- Shared vision (`Csha`, CSharedVisionData: its agent, a mask of the players shown
  the unit and sixteen counts, `w [w]`): a unit's record names it at +0x13c (its
  fifth field), its detection (`Cdet`) at +0x130 (its second).  The buffs that
  show their bearer to their caster's owner (vf213 `FUN_6f5304c0`: Devour Vision,
  Ensnare's three, Faerie Fire, Shadow Sight, Shared Vision, Web's two) make the
  bearer's the first time (`FUN_6f688ba0`, after the buff's own object) and count
  their player in it; each names that player in its own record (`FUN_6f4ec0d0`,
  +0xcc, its ninth field past the ability's).  Letting one go (vf207
  `FUN_6f52e010` -> `FUN_6f678240`) takes it for granted: the creeps-only save,
  its Ensnare buff on a dragon written without one, crashed so.  A shop's neutral
  ability shares vision too (`CAbilityNeutral` vf190 `FUN_6f3f8010`), as a
  player's unit comes in range (the every-unit map's shops' at 5 seconds); the
  port does not, and the original makes them again after loading.
- The game's Save writes the original's format alone (`Scene::save_files`: the fresh
  writer, the header 1.27's, `PX3W` 27 6059), and loading reads it as the
  original's (`save::read`, `Game::restore_original`), the port's own saves too.
  Nothing rides in the save besides: the original refuses one with bytes after its
  record (back to the menu) or after its container ("The save file is unavailable or
  corrupted").  Measured: a save at 60 seconds of the hero map loads in the original
  and plays.  What the fresh writer still leaves out, freeing their slots (its
  W3Z_COVERAGE=list names each): the objects of kinds it has no records for, the
  hashtables (`ghth`; the hashtables part written empty), multiboards (`+mdb`),
  quests and their items (`+que`, `+qui`), defeat conditions (`+dfc`), and fog
  modifiers (`+fgm`), whose handles the natives give no agent in the handle table; a
  unit's order of a kind not modeled (an attack, an AI's move: its `ordt`, tasks, and
  walk); and the objects of units already removed that the port keeps (7 units' on
  the hero map at 300 seconds).  The load leaves out the hashtables, multiboards,
  quests, dialogs, and the fog.
- The port's own copy of a LAN save loads as the original's copy does once the
  object manager's main clock is the engine's at its last tick (53.783657 in
  Mafarazzo TD's save at 53.8 seconds, which every timer's due is reckoned against)
  and a walker's own path holds its way as the port keeps it: its local way from its
  local goal to the node it steers for (by that node's index), then where it stands,
  and its own coarse way (`move_coarse`, limit 400), its walk's path the order's
  (`coarse`, limit 5000), its cluster's local goal the cluster's point.  Then the
  port's copy of the turn-539 save agrees as far as the original's against the
  mafload captures.  A `^clu`'s word 0x20000 is not its walker's pathing: 78 of the
  save's 180 have it, every creep's pathing off (its unit's first word without 0x40).
- A network game's save (the original's of a LAN game on Booty Bay, read): its
  second name empty, its description the game's GAMEINFO (its name, no password,
  the stat with the lobby's options, `0x648 << 8 | 2`, and the host's name, and the
  words people and 9, as a game played alone's "Local Game", 12, and 1), the slot
  table W3GS carries, and the local slot and player.  The port's network saves are
  written so (`Replay::network_head`); loading a network game, each computer loads
  its own copy, as the original's do.  A game the port joined has the
  stat its host offered (below); not yet loaded by the original (its LAN load wants
  the other players to join it: the host example's W3HOSTLOAD now offers one as the
  port's LAN hosting does, the original to join it).
- Loading a network game over LAN, each computer loads its own copy, and a joiner's
  copy must be the host's game's: the original's join (CLocalMultiplayerLoad vf3)
  compares the game's announced identifier (GAMEINFO's map checksum) with its own
  copy's word after the slot table (CSaveGame +0x430, `FUN_6f24d770` reading it),
  the objects' sum `FUN_6f324930` (each live object's vf 0x3c over the object table:
  the same on every computer of the game; the Booty Bay save's 0xc2aa926c).  The
  players' names are in their records (`+ply`, the first text of the tail after the
  event lists; "Host" and "Jack"), by which the lobby keeps each person's slot.  The
  port now offers and joins a load of a save in the original's format so (its
  identifier that word, its slots from the save's slot table and names), each
  computer starting the map with the lobby's setup and then loading its copy
  (`restore_original`): two of the port's, as Host and Jack, loading the original's
  Booty Bay save over LAN play 300 frames alike.  The port's own saves now carry the
  sum (below), and the port announces and checks a save in the original's format by
  that word, its own state beside it or not, as the original does.
- The objects' sum (`FUN_6f324930`, the "tempest checksum" of the debug dump
  `FUN_6f328d70`; the save takes it through `FUN_6f32b1c0` in `FUN_6f1e8630`, before
  the object manager's records): over the manager's main table (`DAT_6fd6860c`: its
  entries at +0xc, eight bytes each, -2 and the object when live, their count at +0x1c;
  the second table at +0x20, the negative tags', is not summed), each live object's vf
  0x3c adds to it.  Each word it adds by its four bytes, `b * 0x7a2d ^ b` each, so the
  order does not matter.  The table holds the engine's own objects, each a
  CPresenceTagged (an agent of the game's by its wrapper, CAgentBaseAbs, the `+agl`
  family), each class's vf 0x3c its base's then its own: CPresenceTagged
  `FUN_6f1c54e0` its tag and birth (+0x14, +0x18), and no more for the pathing maps,
  the accelerator grids, and the long-range system and its accelerator; CPrRelation
  `FUN_6f15d960` (the wrappers, the classes' definitions) its real (+0x44), its two
  words (+0x48, +0x4c), and the tags of the object at +0x30 (its first pair's second
  reference), twice; CRlProperty `FUN_6f153c90` its real and word (+0x70, +0x74), then
  CPoFlag `FUN_6f152720` its value, CPoReal `FUN_6f153310` its four, CPoPos
  `FUN_6f152a50` its seven, CPoPosBh (`` `pbh ``) `FUN_6f152cd0` its nine reals more,
  and CPoPosCl (`` `pcu ``) `FUN_6f16f0a0` all its own but its third word, its flags,
  and its reference, with its separation's three (CPoSeparate `FUN_6f159ef0`); the
  threshold, flag, and in-range listeners and the continuous modifier
  (`FUN_6f153240`, `FUN_6f1524f0`, `FUN_6f152860`, `FUN_6f153000`) their one or two
  own; CPmRegion `FUN_6f14cc20` its bounds and word; CLrPath `FUN_6f166510` its six
  reals, each way's count, points, first word, and short, its first three words, and
  its word and real; CPrCluster `FUN_6f153480` each entry's first two words, two
  reals, and last word, and all its own but its third word and references;
  CPrClusterGroup (`^clp`, its record `FUN_6f16aec0`: a word and three reals)
  `FUN_6f153b20` those four; CBhPoProjectile (`>prj`, its record `FUN_6f173160`)
  `FUN_6f152230` eight of its own.  Every field summed is one its first record holds
  (vf 0x24), so the sum is the save's own: reckoned over its records
  (`SaveGame::objects_sum`) it is the word of every save of the original's at hand (66
  in its Save folder, Booty Bay's LAN save's 0xc2aa926c and Mafarazzo TD's at 30
  seconds' 0x2dbc90f0 among them).  The port writes its saves' word so, the sum of
  the records it writes, the same on every computer of a game.  Its save of the quiet
  test map at 5 seconds carries the original's word (0xdb4ef280: every summed field of
  every object agrees).  Eleven other test maps' saves, the port's played from the
  map's start (its playtest's two slots, where the original's of Forestwalk had three,
  the third open; not the original's seed), differ where their records do: the
  objects its saves leave out (a walk's order, tasks, cluster, and path; a computer's
  captains), the listeners and modifiers about them, and the walkers' positions; and
  on Forestwalk at 0.3 seconds 22 pathing regions' word (+0x34: the port's 0x04000000
  for four where the original's is 0, 0x010000ca for 18 where it is 0x01000000), the
  blights' places (`Xbli`'s `` `pos `` and its regions' bounds, 2 cells apart), the
  dragon whelps' separations (CPoSeparate, which the port writes for none at rest),
  and the Satyr Hellcallers' speed (10.999999 cells a second, the port's 10: an aura's
  tenth more, provisional).  A restored original's save (Mafarazzo TD's at 30
  seconds) written again has a threshold listener's record of another class's length
  (slot 647, an agent's wrapper's), its sum not reckoned (0 written).
- A network game's save action (0x06, CNetCommandSave, its file's name: the
  original's "lansave.w3z", the name typed with the extension) is read with its
  turn's actions (`FUN_6f325e10`: the command kept at the net game's +0x1b48, +0x1b44
  set) and carried out once the turn's message is handled (`FUN_6f327040`), after
  the turn's keepalive (`FUN_6f32e3f0` -> `FUN_6f330830`): the net module is set back
  first (`FUN_6f32f930`, NTempest::CSyncData's vf2 `FUN_6f32f5f0`: its sum and count
  0), then the command's handler (`FUN_6f26f450`, registered for 6 at 0x6f2712e6 by
  `FUN_6f271290`; 7's `FUN_6f26f430`).  It goes on while no save is under way and,
  but in a game played alone (its provider 'LOOP' or 'NONE'), the saver has made none
  (`FUN_6f32b800`: the player's +0x3c, which it adds one to after, `FUN_6f32d010`); it
  sets the session saving (`FUN_6f32c6a0`, below), pauses the interface's game for
  player 0xff (`FUN_6f3719c0`; `FUN_6f1e5040` pauses the trigger executions, `+tex`),
  passes the name on (`FUN_6f284760`, event 0x80303), writes Save\Multiplayer\<name>
  (`FUN_6f2bfc90`, `FUN_6f1e8630`) but in a replay, and sends 0x07 with the write's
  result (`FUN_6f26c130`, CNetCommandSaveDone; the original's 07 carried 1).  So the
  save holds the game after its turn's actions and before its turn's time: the
  original's LAN save of Booty Bay has game time 58800 ms and its main clock 58.78091,
  which is the port's engine clock at tick 11760, the end of turn 588 (its 06 then in
  turn 589, whose keepalive's time is 58800; provisional until a capture shows the
  turn).  The port does so (`Game::after_keepalive`, from `play_turn` and
  `play_frame`): each computer makes its copy then (`Game::saves`, `Game::save_files`,
  in a game being recorded), its client keeps it in Save\Multiplayer under the
  action's name, and says its save is done (0x07 with
  1, `Command::SaveDone`).  Its interface's Save now sends the name with the extension.
- A network game's save under way (the net session, `FUN_6f32c6a0`): +0x2dd the
  saver, +0x2e0 8000, +0x2e4 set, and the session's state 5 (`FUN_6f32feb0`).  The
  turn loop (`FUN_6f32d670`) runs turn time in chunks of 25 ms, each ticking the game
  three, four, or five steps of 5 ms by the speed (`DAT_6fce54dc`) in state 4 or 5,
  state 5 then becoming 6, in which the chunks pass without ticks: the save's turn has
  its first 25 ms and then the game holds.  Each later turn's message takes its
  milliseconds off +0x2e0 (`FUN_6f32e3f0`, after its actions, before its keepalive);
  each 0x07 marks its player done (`FUN_6f32c7b0`: nothing while none is under way);
  when every player of the session is, or +0x2e0 runs out (8000 ms of turns), the
  save is over (`FUN_6f328bd0`: state 4 again, +0x2dd 0xff, CNetEventSaveComplete,
  0x40090096, which the interface takes as its handler 5, registered at 0x6f367e14;
  not read).  Meanwhile the turns' pauses, orders, and cancels (0x01, 0x10 to 0x15,
  0x1d, 0x1e) are read without being carried out (`FUN_6f325e10`, its +0x2dd not
  0xff).  The port keeps it so (`world::SaveWait`, `Game::save_turn`, `Game::advance`,
  `actions::ignored_while_saving`; provisional: the interface's part at the end, the
  game going on, and the 25 ms at the fast speed only).  Two of the port's
  (`nettest`, NETTEST_NETSAVE) save at a third of a Booty Bay game: each makes its
  copy, the same objects' word in both (0x43529444), both 07s come the next turn, and
  the game goes on in step; with NETTEST_NETLOAD each loads its own copy and they go
  on in step.
- A network save's description (`FUN_6f968cf0`, the session's +0x13c, which
  `FUN_6f96bef0` copies from its source: its words from +0x9c and +0xa0, the second
  only for a first of 1 to 16): GAMEINFO's slots and game type (read: Booty Bay's LAN
  save 2 and 9; Mafarazzo TD's, two people in twelve slots, 12 and 1), and
  its stat GAMEINFO's as the host offered it (the host's name, the size it gave:
  160 by 72 there).  Its map setup's second and third words (`FUN_6f2465b0`: +0x34,
  +0x38 less 0x80000) are the game type and advanced options (9 and 0x648 there; a game
  played alone's 1 and 0x02008000).  The port takes them from the game's record
  (`NetRecord`'s stat and start spots, the offered or joined GAMEINFO's) and writes its
  slot table as W3GS carried it and its local slot by the lobby's order
  (`Replay::network_head`).  The tags example's copy of the quiet Booty Bay game, saved
  at turn 589 as the original's 06 came (TAGS_SAVEDIR), has the original's header but
  the objects' word (the stat, words, slot table, local slot and name, game time);
  its records differ where the port's saves still do (its word 0x45ab73e0, the
  original's 0xc2aa926c), and further: the players' agents' own word (CPlayerWar3
  +0x20, bit 2 set by `FUN_6f26a9f0` and `FUN_6f26aa20`) is 0 for Host and 2 for Jack in
  Jack's copy, where the port writes 0 for the local player (not yet changed), and
  the clocks, the original's 58.78091 and 36.015045 where the port writes the game
  time.
- The original's save of a Mafarazzo TD LAN game (its 06 in turn 539, the host's 07
  in 540, its own in 543; the port's tags copy from the host's log, TAGS_NETGAME) has
  the port's header in every field (words 12 and 1, game time 53800, (539 − 1) × 100).
  Its keepalives give the hold: turn 540's time is 53825 (the save turn's 25 ms), held
  through 543, the net module set back (each keepalive then as the port's but for one
  bit).  That bit was a birth: the save's event data (`` `gsed ``, its class
  `FUN_6f285380`; made by `FUN_6f283920` from `FUN_6f284760`, which keeps the save's
  name at its +0x20, sets the game's +0x3e8 to 0, and passes it with script event
  0x80303 to the triggers) is in the save at the next slot (9156), its first record its
  class's definition and a second timer of 0.0001 s from the main clock (its count
  the manager's 11252), its second the game's handler for 0x80267 in the first fixed
  list and no event lists; held by the event, it is let go by that timer in the next
  tick, so before the next keepalive born counts one more and the next slot is again
  9156.  The port makes and lets it go so (`World::save_event`,
  `World::save_event_records`): its keepalives now agree through turn 554 of that game.
- After a network game's save its turns run 25 ms off the tenths, and the detectors'
  looks (agent timers each half second, `World::detection_looks`) fall due a turn
  before the half second's turn by the port's count: the look due 55.0 within the
  reach of the turn ending 55.025 (its main clock past 55.0 by its end), its two `Cdet`
  born before that turn's keepalive in the original's game (the keepalive's born two
  more, next slot 0x2216).  The port takes a look a turn ahead when its due (as a
  100 ms turn crossing its half second has it) is within the turn's reach, and not
  again at the crossing: that game's keepalives then agree through turn 628.  Left:
  turns 629 and 665 to 666 part one turn each and 686 on for good; at 629 the born
  and next slot fit three objects (`+tex`, `+loc`, `ordt`) of a step's region event
  one turn early in the original's, or five a turn late (provisional: a step's
  region trigger near the turn's end, `World::carried_region_triggers`, not yet
  read against the original).
- The object manager's clocks as the original saves them (`FUN_6f04dd30`: the main
  and agent queues at DAT_6fd3c82c +0x40 and +0x44, each its time +0x40, word, 300,
  word, and count +0x50): Mafarazzo TD's at 53.8 seconds main 53.783657 (the engine's
  clock at tick 10760, the port's `main_clock_now`), count 491, agent 53.853115,
  count 6; the manager's header (`FUN_6f15c9b0`, four queues of its own at +0x54 on)
  53.783657 count 11252 (the `gsed` timer's count), 53.853115 count 0, and the two
  again with counts 0.  Booty Bay's LAN save: main 58.78091 count 5203, agent
  36.015045 count 2 (header 2657).  The port writes the game time for both clocks
  before the clock's first wrap and counts 0 (`World::saved_clocks`, unchanged: the
  agent queue's time, which runs ahead of the main clock there and far behind it in
  the quiet Booty Bay game, and the counts are not modeled; the port's count of agent
  timer starts was 17).
- Loading the original's Mafarazzo TD save over LAN (the host example's W3HOSTLOAD,
  the original joining with its own copy), its first keepalive is the port's with:
  the net time the save's game time (53800: `FUN_6f32a860` reads the net game's
  +0x1c70 and +0x1c74 back), the net module a fresh session's one message, the
  generators seeded afresh (the random data module a fresh one's, the main
  generator's hash a fresh one's: none of the save's, no map script run), the game
  module the saved game's (each player's tech registrations as saved: the port's
  load made each peasant twice), born the save's births and two more, and the next
  slot the saved free list's third (6273 and 8477 taken by two objects made as it
  loads).  The port's load now takes the saved tech registrations
  (`SaveGame::player_techs`), seeds its generators afresh, and counts the save's
  game time in the keepalive's time (its game time, which goes on from the save's,
  below).
- The two objects the original makes as it loads (read: that loaded game saved again
  at its turns 77 and 246, births 19013 and 19014 at slots 6273 and 8477 in both) are
  threshold listeners (`` `rtl ``), one for each flying peasant (units 1383 and 1406):
  on the unit's second real (`` `rel ``), its word 852407 (0xd01b7), at -0.1, linked
  after the unit's in-range object (its burrow detection's `` `pil ``).  They are its
  Repair's listeners made anew as the load sets its mana, the old ones (slots 1400
  and 1423) going as the first tick begins (below; `Game::loaded_objects`).  The loaded game goes on from the save's times:
  at turn 77 its game time 61400 (53800 and 76 turns), its main clock 61.379482 and
  its agent clock 61.533245 (the saved clocks run on, not started again), the
  manager's counter 3070 (2816 saved; 3633 at turn 246) and its first queue's count
  13224 (11252; 17263), the main and agent queues' counts 618 and 6 (491 and 6; 907
  and 35); born 21543 and 26165; the second table 1156 and 1181 objects (1137), 19
  and 20 of its slots free.
- The port's load of an original's save now restores the object table
  (`Game::restore_original`, `tags::RestorePlan`): each unit and the objects made
  with it take their saved slots and births (the saved objects born one after
  another from the unit's birth, of the codes the port's making takes in turn); an
  order of a player's kind (`ordt`, a script's or a player's walk) is given again as
  a player's (`World::ordering_player`), its objects at the saved order's by their
  places from its first (`ordt`, `tskA`, `tskO`, three `task`, `tsk.`, `^clu`, and
  `l^pa` at 0, 2, 3, 4 to 6, 7, 15, and 16, as `Tags::player_move_issued` makes
  them); a unit's detection record (`Cdet`, its record's second and third words)
  is taken at its saved slot and birth, found by no player yet, so the next detector
  to find the unit takes it rather than making one; what else the restore makes takes
  slots past the saved table; then the
  allocator is the save's (births, slots, and free list, those the port's objects
  hold kept out of it), and the objects its save does not hold as they are
  (`Tags::strangers`) are given back to no free list.  Of Mafarazzo TD's 6463 saved
  objects 5751 are then the port's alike (1414 before; 690 of the rest leaked
  locations), of Booty Bay's LAN save's 12270 12223; the LAN load's first keepalive
  is the original's, the second born one off (or seven) and its next slot a saved
  detection record (6463) or a walk's order (6462) where the port's is 7879 (open:
  the port's clocks starting again from 0 where the original's run on, and the
  restored walks' state; both since, below).
- A loaded game goes on from its save (`Game::restore_original`), read against the
  three LAN loads of the Mafarazzo TD save (`mafload`, `mafload2`, `mafload3`):
  - Its clocks (`World::carry_clocks_to`): the game time the save's (53800, the
    keepalive's time with it), the main clock the engine's sum of 5 ms ticks to its
    tick (53.783657 at 10760; at the load's turn 77 61.379482, as the original saves
    it there), and the movement steps planned to then.  The path manager's step
    counter (+0x538, `FUN_6f15aa80`: 1024 at the start, one more as each step begins;
    the manager header's fifth word, `FUN_6f15c9b0`) is 1024 and the steps but the
    game's first, at its clock 0: 2816 at 53.8 seconds, 3070 at the load's turn 77,
    as the port now writes (provisional: which step goes uncounted).  Not yet: the
    agent queue's clock (the original's 53.853115 and 61.533245, the port's last
    step's due 53.758354 and 61.378044), the four queues' counts, and the header's
    first two words, which the port writes 0.
  - Its walkers go on with their walks, neither planning nor searching anew: each
    one's mover its clustered position's (`` `pcu ``: place in cells, velocity,
    heading, and its stamp, its last step's main clock), its speed that position's
    (SetUnitMoveSpeed's, Mafarazzo TD's creeps' random), its local way its own path's
    (`l^pa`, the position's last reference: the cells from its local goal to where its
    search set out and the index of the node it steers for, every creep's 0, its
    local goal, as the port's straight way), its own coarse way and index that path's
    second (limit 400), and its order's coarse way and index its walk's cluster's path
    (the position's first words name the `^clu`, its third reference the `l^pa`: limit
    5000, read 0 to 8 along).  Before, the creeps stood where they were saved, their
    walks begun anew and held there.
  - Who is inside each watched region is who stands there as it loads: no unit enters
    one it was saved in (the port's 16 creeps entering their lanes' turning regions
    anew at the first step, ordered on again, were 16 walks' objects more than the
    original's by turn 77).
  - Its walks' order (walkers step, and their old paths are freed, newest first): a
    loaded walk's place its cluster's slot, highest newest, all older than any ordered
    after (the load links each cluster at the list's head as the table gives them;
    read: four creeps' old paths freed in the load's fifth turn, the original's last
    the cluster at slot 3592, born between the others).
  - A detector's looks go on by its in-range object's timer (`` `pil ``, its due on
    the agent queue): its look clock set so the next falls at that due, in the tick
    after it (the flying peasants' due 54.0 as saved at 53.8; the load's two
    detections at 55.0 made after its keepalive there, not a tick before).
  - The map's start's units go with their own objects, so the units made anew take
    their slots: the flying peasants, made at the start at slots 1383 and 1406 in both
    games, had been made anew at free slots, their in-range objects too.
  - The properties made again keep their saved stamps (reals', flags', and
    positions' base reals: the creeps' reals 19.997644, made at 20 seconds), the units
    their saved facings in radians, and a unit its pathing (SetUnitPathing's) as its
    own flags have it: off where its record's first word (CUnit +0x20) lacks 0x40, a
    structure's never having it (read: every Mafarazzo TD creep's, 0x20000406; its
    walk's cluster's 0x20000, set on 78 of the 180, no guide to it, as the port had
    taken it).  Saved at once after the load,
    the port's records are then the save's for every real (917) and every standing
    unit's position; the 180 walkers' positions (their speed scalar, the original's
    6.2227297 where the port's velocity's length is 6.222687, two turning reals, and
    flags), their clusters (two counters, `^clu`'s 81 and 81 where the port writes 0,
    and its last real), their paths (the first word and word), and 13 pathing regions
    still part, the fields the port's own save of the game parts on.
  With these the port's keepalives agree with all three loads' through turn 44
  (where they parted at turn 2), and the second's again from 46 through 82, its save
  at turn 77 and the hold after among them.  Left: in the first and third, turns 45,
  80, 94, 95, 111, 116, 120, 128, and 135 part one turn each (in the second 45, 83,
  84, 96, 98, 101, 118, 119, 123, 131, and 138), the births the same and the next slot
  another (provisional readings: at 45 and 80 a step's three orders at the tick's
  last step, the original's next slot the second order's tenth object, as if its
  last freed then; at 94 a re-order's objects between the look's detections in the
  original's, the port's all before it); from about turn 144 on they part for good,
  born one more (or seven fewer).
- The loads' parts read in the original (its LAN load of the Mafarazzo TD save,
  every tag take and free, `FUN_6f1cbf40` and `FUN_6f1cbff0`, while births are 20370
  to 23870: its turns 39 to 145, `abtest/loadbirths.log` and `loadbirths2.log`; the
  breaks catch the second table's takes and frees too, told by the first table's
  births and free list head standing): every tick from 41 to 145 takes and frees the
  first table's objects of the same kinds in the same order as the port's, only
  their slots differing, so the one-turn parts (45, 80, 94, 95, 111, 116, 120, 128,
  135, 143) are not of the ticks' order but of the free list below its head.  At
  keepalive 44 the original's holds four slots the port's holds live (0x1ded, 0x1de1,
  0x1ddb, 0x1ddc), at 40 three (0x1dfc, 0x1de8, 0x1de9: the second of turn 37's two
  orders' `task`, `^clu`, and `l^pa` in the port); the parting is older still (not
  yet read).  One source found: the threads the map's start left asleep, which the
  port's load kept with their executions' objects (the Initialization trigger's 20
  seconds of sleeps: its `+tex` at slot 1376, where the save holds a walk's `^clu`,
  so that walk's cluster was made elsewhere and its frees parted from turn 39); the
  port's load now lets them go (`Game::restore_original`).  Saved slots the port's
  start holds with other objects are then four: two timers, a handler, and a
  modifier, over two leaked locations, a leaderboard, and a handler.
- The load's first tick, read in the original (its takes and frees from births 19013,
  `abtest/loadbirths3.log`): 0.0001 after its first step (53.783756) it frees the
  save's event data (`gsed`, slot 9156) and the flying peasants' old Repair
  listeners (`` `rtl `` at 1400 and 1423), whose new ones it made as it loaded (births
  19013 and 19014): as when a script sets a unit's mana, each spell's listener made
  anew the first time and the old let go (`World::mana_changed`), the load setting
  each unit's mana.  The port's load now does so (`Game::loaded_objects`: the save's
  `gsed` and the old listeners go as the first tick begins, before its first step).
  With that the load's ticks take and free as the original's, slots and all, from
  the first; the three LAN loads' keepalives agree through turn 192 (the second's
  through 193; their saves at 77 and 246 among them), then part at a few turns (the
  first's 193, 218, 219, and 267; the second's 194, 222, and 270; the third's 193,
  218, 219, and 270), the births the same and the next slot another: in each, the
  last freed detection record (`Cdet`) is the one the port gave another creep found in
  the same look (0x23a8 and 0x216c, 0x1cd7 and 0x1f7e, each pair made together),
  since the looks took their creeps in another order (below).
- The in-range search's order (a detector's look, an aura's, `FUN_6f15f210`): the
  query's rectangle of the path manager's 256 grid walked row by row (`FUN_6f15fa80`),
  each cell's list from its head (`FUN_6f15f9b0`, a unit counted at the first of its
  cells), the new units appended in that order and then taken last first
  (`FUN_6f15f2a0`, to `FUN_6f15fdc0`).  A cell's list takes a unit at its head as the
  unit is made and as it walks into the cell, so the search finds each block's latest
  come into it first: Mafarazzo TD's creeps, walking a lane in file, are found in the
  order they walked into their block, not the order they were made in, the port's tie
  before (read: the LAN load's looks at 58.6 and 60.6 seconds, by the detection
  records the original freed as the creeps leaked).  The port notes each unit's block
  and when it came into it, as it is made and at each movement step
  (`World::note_block`; a loaded game's units in their saved order), and orders its
  searches by that (`natives::units::spatial_order`; provisional: a unit moved
  otherwise counts as just come in).  With that the three LAN loads' keepalives agree
  with the original's through every turn compared, the saved game's (mafsave)
  through all 825 (from 712), and the plain game mafa7's through all 772 (from 728).
- A creep KillUnit bursts (ExplodeUnitBJ, read at Mafarazzo TD's first creep to leak
  after the load, turn 144, and the second, 145): WalkEnd's trigger runs before
  Losing life's (the regions a step's walker enters taken oldest first, the two made
  from one rect).  In a game played from its start the order is the other way: the
  regions newest first, as each is put at its cell's head as it is made (read: the
  plain-map game's first creep to leak, turn 681: Losing life's execution, its
  trigger run, then WalkEnd's and its kill, Losing life's execution freed first of
  the burst's; `World::step_regions`, `World::regions_reloaded`; the plain-map
  game's trace without the second table's objects now agrees to turn 695, from 680).
  (Provisional: the load's order taken as the regions remade oldest last.)  Its
  kill's nine tasks are followed by its owner's death event's
  data (`pudd`, the event listened for), the death trigger's execution, and Losing
  life's; then the death's effect (`+EIP`, `` `pos ``, two `^pmr`, the second
  table's).  0.0001 after the triggers all go together, in order: its AI's `uref`
  and `AI-U`, the kill's first task, its walk's tree (top first, then `ordt`), the
  kill's ninth to seventh, its regeneration's `` `rtl ``, the death trigger's
  execution and the `pudd`, the kill's sixth, WalkEnd's and Losing life's
  executions, the kill's fifth to third, its abilities (the last first), its own
  objects (reals, path, regions, position, itself), and the kill's second; its
  detection's record 0.0001 later; its walk's path and cluster at the next step.  It
  leaves no body.  The port does so (`World::burst_begun`, `World::burst_end`): the
  two ticks then take and free as the original's, and the load's keepalives agree
  again from 146 to 157.  (The eight frees of slots 0x471 to 0x478 at turn 145's last
  main tick, 68.175743, are the second table's: the first table's head stands
  through them, and a break on frees of 0x473 caught none then in another run of the
  same game.)
- A walker's clustered position (`` `pcu ``, `FUN_6f16f890`: its words +0x9c and
  +0xa0, +0xa4, eight reals, +0xd0, +0xd4, the flag word +0xd8, then its first flag
  set when +0x4 or +0x8 is, its second when it has a separation, +0xac): in all 646
  moving positions of 37 of the original's saves its fifth real is its top speed
  (the fifth value), not its velocity's length, and its first flag is set exactly when
  its seventh real is not its heading (191 of them, turning; their sixth and eighth
  reals not yet named); a walk to a point's first real is 0.49 (Mafarazzo TD's 180
  creeps, Booty Bay's LAN save's two walkers).  The port writes the first two so.
  A creep's own path's local way keeps a second word near the path manager's step
  counter when its search was made (2736 for one, its cluster's first counter 81 at
  the save's 2816: of the 180 walkers' first counters 70 are 2817 less it and 24
  one less than that, the rest's word 0; provisional, not yet
  written).
- The positions' stamps (CPoPosCl's base real): a walker's is the main clock of its
  last step's tick (Mafarazzo TD's 180 walkers 53.753674 with the main clock at
  53.783657), as the port now writes.  The objects' word of that save still parts
  at those walkers' headings, speeds, and turning reals (their `` `pcu ``, the
  heading 4.7431884 where the port's velocity's angle is 4.743192), their clusters'
  words (`^clu`'s 61 and 61) and last real, their paths' first word and word
  (`l^pa`'s 0xc0a00000 and 0x04000004, the port's 0x00200000 and 0x02000002), and
  14 pathing regions' words.  The placed state (PLAYER_STATE_PLACED) is 1 for a
  network game's open slots (the map's nine unplayed user slots there; Mafarazzo TD
  loaded directly, `maf_save30`, places only those playing), as the port now sets
  it.  A placed destructable's saved facing goes through degrees (the map's radians
  times 57.29578, then times 0.01745329: Mafarazzo TD's six at 0x40963ceb save
  0x40963cea), as the port now writes it.
- A unit's clustered position's flag word (CPoPosCl +0xd8, its record's third word
  after its eight reals): its type's orientInterp in bits 8 to 11 and formation in 12
  to 15, set as the unit is made from its type's data (`FUN_6f652620`, `FUN_6f653800`:
  CUnit vf50 `FUN_6f698c80` to `FUN_6f171010`, then vf51 `FUN_6f698750` to
  `FUN_6f171070`; a morph's, `FUN_6f6417c0`, the new type's, its data's words 0x7a and
  0x7b); bit 16 by `FUN_6f171060` (cleared by `FUN_6f16f560`); and bit 24 by an
  attack (CAbilityAttack through `FUN_6f05c350`: set by `FUN_6f49bc40` as it starts its
  timer of event 0xd01bd with no target, cleared by `FUN_6f497310`, its vf41 and
  vf63).  Read: every unit of the quiet test map's at 5 seconds and of Booty Bay's LAN
  save the type's two alone, and on the every-unit map (`fwall`) player 0's units bit
  24 besides.  The 0x4000 the port wrote for a walker was the peasant's formation; it
  writes the type's two (provisional: bits 16 and 24 not set).
- Continuing the original's saves (`Game::restore_original`): the port's compiled
  code is the original's word for word (map script and AI scripts), so each machine
  comes back as saved: its globals (strings by their text, a handle where the port
  holds an object of the kind the save's handle table names), and its threads
  (`thread_from`, `thread_of`'s inverse: each frame's function from the code, the
  current one's from its place, a caller's from the return place it pushed, a run's
  base below a frame marked its run's first; exact on every port AI thread,
  `aiground`).  An AI thread wakes by its `AItr` timer, bounded by the sleep it
  records (a due not yet set back at the clock's wrap at 300 seconds); a computer's
  captains (`AI-C`, attack kind 2, defense 1) by their units (the owners, on their
  first lists, of the chain of `uref` from the head the record names), their goal or
  home, place, and target.  A map thread asleep (TriggerSleepAction) wakes by its
  execution's object (`+tex`, CTriggerExecution: fifteen context words, slot 0 the
  triggering unit, 1 widget, 2 trigger, 3 player, 4 and 5 the event's own objects, 6
  the event id, 7 to 9 the filter unit, player, and destructable, 10 to 13 the enum
  unit, player, destructable, and item, 14 the filter item, as the natives read them
  through `FUN_6f2892d0`; then its timer, the sleep in it, the thread's place, its
  action, and its trigger).  The thread records the sleep itself too (`real`, which
  the port writes 0).  Measured: the hero map's computer at 300 seconds resumes its
  attack and trains; a trigger waiting at 60 seconds wakes 20 seconds after loading.
- Loading the original's save runs the map's start again, so what the script made
  then is set back as saved: a trigger's second flag is its enabled state (0 once
  DisableTrigger turned it off, read on a trigger disabling itself as it ran), and a
  timer event's registration (`tmvt`) its timer running to its due or stopped once
  fired (on the main clock).  A timer event's own timer takes no script handle in
  the original (its event's handle the next after its trigger's), nor now in the
  port; each unit's order (its record's fifteenth and sixteenth fields: `+ord`, or
  `ordt` with its point and target) is given again without its events.
- A player's build order (a point order, flags 4, the structure's id as its order)
  makes the AI's build order's objects (`ord.`, two blocks, …, `tskO`, a task, a
  block, its walk's `^clg ^clu l^pa`), its `^clg` freed at once and its first block and
  last six with the next turn's orders, but no last order as the AI's makes; the
  structure is begun as an AI's peasant's is (its construct order, the rise anew, the
  build order's leftovers).  Traced over LAN with the original: Mafarazzo TD's host
  player's peasant sent to build an Arrow Tower at turn 31, begun at turn 182.
  (Provisional: a builder that works on its structure's alone.)
  The first block and last six go after the timers due as the next tick begins, as an
  upgrade order's objects do: traced (the maft game, Mafarazzo TD's whole tech tree
  built by its host's player), the Town Hall ordered at turn 21, the leaderboard
  trigger's objects at 2.0 made in the freed `^clg`'s slot and those before it, then
  the seven freed, then the trigger's execution (its slot the last free as turn 22's
  keepalive was taken); the port had freed them as the tick began, with the orders
  given there (`World::player_build_objects` now frees them by the main clock as the
  order was carried out).  A build order refused (the HSBC without its Castle and St
  George Bank, the selections of that game having missed) makes `ord.` and a block,
  both freed in the next tick, the block first (provisional: for every refusal).
  A flying builder begins its structure as one touching it whenever it is within its
  repair's reach (`FUN_6f41d1f0`'s distance), the rise made anew at once, no target
  task or search: the same game's flying peasant, 2.97 cells from its Cutter Tower's
  middle, clear of the ring of cells about its own (which the ground peasant's test,
  `FUN_6f166310`, reads), began it so.  (Provisional: why; the air layer does not hold
  the structure.)  With these and the ids its selections name, both maft captures'
  keepalives agree as far as taken, 2326 and 4129 turns, and the traced one's objects
  all, slots and all.
- Each client, as any unit changes its type (`FUN_6f670950`, for one without the
  unit's +0x5c flag 0x100), rebuilds its own player's selection's subgroups and sends
  0x1a, DispatchUnitRefreshSubGroup (`FUN_6f291f90(1)`, `FUN_6f6bd310`), then 0x19 for
  its selected subgroup if it has one (`FUN_6f6bd280`).  So the original, Blue and
  selecting nothing, sent a lone 0x1a two turns after each of the host's upgrades
  that took (turns 144, 352, and 452 for those of 142, 350, and 450), and later under
  the tracer's lag (329, 739, 911).  (Not yet in the port's own client.)
- A unit a structure trains that flies is placed at the first ring's first cell about
  its footprint, as a walker is when nothing stands there (provisional: the rings'
  search for air units not read; the port had it at the structure's middle, inside
  it).
- Mafarazzo TD's Sell (its trigger on a tower's training of n001: the tower's point
  value in gold, ExplodeUnitBJ on the tower, RemoveUnit on n001), traced twice over LAN
  (the maftw games, turns 2127 and 2128):
  - The training's `rtl` and `rcm` and the trained unit's two tasks (the second first)
    go with the turn's releases, the `ptcf` after what the finish's triggers let go
    (their execution first); the port had freed them at once, before the trigger made
    its objects (for an AI's unit, freed at once as before).  A trained unit its
    finish's trigger removed takes an `ordt` still, freed last (provisional: why).
  - KillUnit's nine tasks on a structure: its first, ninth, eighth, and seventh, the
    structure's own listener, and its sixth go with the turn's releases; its fifth and
    fourth in the tick its death (UnitData death, 2.34 for the Freeze Tower) falls in;
    its third, the structure's abilities (the last first), its own objects (reals, path,
    regions, position, itself), its second, and its detection's record as its body goes,
    its death and StructureDecayTime on, exploded or not (the port had removed an
    exploded structure at once).  Its footprint's three regions go as it dies, at once,
    before the trigger's next action's objects; the port frees its ground then too.
  - The dead tower and the removed n001 had left their owner's tech tree's present by
    the next keepalive: a unit leaves it as it dies and as a script removes it, at once,
    not as its body goes (`natives::units::leave_tech`).
  - A detector's look finds no unit a script has removed (n001, found by none).
- A detector's look due after a turn's last step is taken in that turn only if its
  agent queue reaches it, as it is brought to the next main tick's clock; else it comes
  with the next turn's, before its first step: traced, the maftw game's flying
  peasant's look due 155.0, after turn 1551's last step (154.97177, its agent queue at
  about 154.98 as the turn ends), its detection of the HSBC begun in that turn made as
  turn 1552 began; the port had taken every look left at the turn's end
  (`World::looks_carried`).
- A flyer holds no walker back (provisional, as it takes no cells from the searches,
  `World::regions_taken`): the port's waypoint provider held a flying peasant behind a
  wisp beside it for good, its searches going straight on through it (Mafarazzo TD's
  peasant sent from its Town Hall past a wisp standing at the hall's middle).
- A flyer's walk plans its coarse way over the accelerator's last layer (its cells'
  no-fly flag, 0x04; structures and trees not in it) and heads straight for each point,
  stopping a step at it, the cluster then picking its next point on the same way
  (`FUN_6f16c4f0`, `FUN_6f1697a0`; summed legs, 10 on).  Measured (abtest/mafbp.w3x,
  Mafarazzo TD with a trigger pinging Red's flying peasant's GetUnitX and GetUnitY each
  tenth of a second, sent at turn 31 to build at (-3392, 1536) from (0, -3264)): it moves
  from 3.09 at 400 a second along 91.2 degrees, for the way's ninth point from the goal,
  (191.5, 113.5) in cells, where the straight line lies at 125.2; it stops a step 3.1
  cells short of it, within StructureFollowRange as an AI's build walk does (above), and
  heads for the way's eighth, (177.5, 129.5), not a new plan's (175.5, 129.5); and so on
  through the seventh, sixth, fifth, fourth, and second (the third, 8.06 on, passed
  over) to the site.
  Its heading turns at once (its shown facing, GetUnitFacing, turns by its rate,
  through 266, 235, 200.7, 154.8, 120.5, and 92.1 from 3.0 to 3.5 seconds; not yet in the
  port).  The port's flyers' build walks so step with the engine's walkers; each of its
  219 pings is the original's to 0.001, and the tower is begun at turn 182.
- A player's builder's order to build, none queued behind it, ends with its
  structure, the rest of its objects (`Atrg`, `tskO`, two tasks, `tskA`, `ordt`, the
  last first) freed after the structure's own (traced over LAN: the flying peasant's
  Arrow Tower, done at turn 192, a second after its begin).  The flying peasant's game
  so agrees to turn 216, where the original makes six `Cdet` (detections, the creeps
  made at 20.0 coming near), now the port's too (a burrow detection's test, above).
- A player's build order's walk is the newest as it is given, as an AI's: its cluster
  is linked at the list's head, so the step reaches its builder before the older walks
  (`World::player_build_objects`).  Traced over LAN (maftr, turns 348 to 374): the
  peasant sent at turn 325 to build an Arrow Tower, begun in turn 349's third step,
  its structure's and order's objects made before the path of a creep's walk of turn
  313, ended in the step before, went; the port had kept the peasant's serial from its
  last walk, older, and freed that path first.
- Its builder's order's objects go as the next turn starts when the structure is done
  in its turn's last main tick (the first at or past its due): the builder's order
  ends in the next main tick (`World::player_builder_done`).  Measured over LAN with
  the original (Mafarazzo TD's maftr games, their keepalives and a trace of turns 86
  to 201): the Town Hall due 8.589906, done in the main tick at 8.594559, the last of
  turn 86, its `rtl`, two `rcm`, `rel`, and `ABnP` freed in it and the order's six in
  turn 87 (the original's next free slot after turn 86 the `ABnP`'s); a Cutter due
  20.977648, done at 20.982479, two ticks short of turn 210's end, its order's six in
  that turn (the next free slot the `ordt`'s).
- What the timers going off at a turn's start release goes in the next main tick, so
  before a first movement step in a later tick, after the timed frees due by then (the
  next frame's destructions, above; `World::first_steps`).  Measured (the maftr games'
  keepalives): the New Round's timer, due at 20.0 as turn 201 begins, its first step
  two main ticks on: its 180 creeps' old orders' objects, the three `+tex`, and the
  `wsed` freed before that step's detection look and the paths of a walk ended in the
  last step of turn 200 (the original's next free slot after turn 201 the walk's
  `^clu`; the port had freed them at the turn's end); the leaderboard's timer at 2.0 in
  turn 21, its first step also two ticks on, its `+tex` after the hall's build order's
  leftovers, freed by their due.  Provisional: whether all of a first tick's releases
  go so (an action's, at least, are timed already).
- A construction is done (`CAbilityBuildInProgress`'s vf205, 0x6f427e40, from
  `FUN_6f424320` on its listener's event 0xd01c4) on the main clock at its build time
  from the step its rise began in: the step it was begun in, or for a builder still
  walking to it the step the builder arrives in (the rise's modifier and listener
  made anew).  Measured under winedbg on Twisted Meadows' human computer: its altar,
  barracks, and second farm done at 62.130093, 81.837433, and 85.755417, their
  begins' steps and build times on to 0.0001, and its first farm, its peasant
  arriving at 17.668, at 52.668034, in turn 527 after that turn's first order (the
  port, taking a turn's 0.1 a time from its begin, had it done a turn early).  Its
  objects go at once as it is done: the rise's listener, the rise's modifier, the
  life's modifier, the real, and Build In Progress, in that order.  The AI's builder
  then takes its next order at once: the order its AI's build order queued behind
  the build (the issue `FUN_6f693450` after `FUN_6f680320`, `FUN_6f9cb7c0` and
  `FUN_6f9cb230`: `+ord` 851974, 0xd0006, the build order's last, kept through its
  order to build), which the unit runs (`CUnit`'s vf3, `FUN_6f9bd6e0`): a task
  (0xd0194, `FUN_6f691f20`), then its AI's look at it (`FUN_6f9cd390`), for a worker
  with Ahar `FUN_6f9c9470`: one carrying a load (the ability's +0x1c4) is ordered
  0xd02b4 with the `+ord` again behind it; one its AI sent to wood since its last
  ClearHarvestAI (flag 2 of the unit's AI record +0x28, set by `FUN_6f9c7410` and
  `FUN_6f9c7700`, moved to 8 by the clear) goes to a tree (`FUN_6f9c6cf0`); else to
  its town's gold mine (`FUN_6f9cc360`: its harvest, 0xd0032, on the town's mine at
  +0x54 unless `FUN_6f9cd8e0` refuses it, then the one at +0x60, then +0x54 anyway),
  the worker's own harvest order (`ordt`, its block, four tasks, three target tasks,
  a task, a target task, four tasks, a block, and its walk).  Traced in turn 527:
  the task, the harvest's 19, its `^clg` freed at once; then the order to build's
  six (its `Atrg`, `tskO`, two tasks, block, and `ordt`), the task, the `+ord`, and the
  harvest's block and four tasks before it.  The port does so for a computer's
  peasant carrying nothing, at its first town's mine (`World::ai_builder_done`);
  provisional: when in the structure's end (here after its finish events, as the
  structure's event 0xd01da, `FUN_6f679d50`, follows them; the trace cannot tell);
  not yet in the port: the load and wood cases, the second mine.
- A wisp building an Ancient is consumed.  CAbilityNightElfBuild (`AEbu`, the build
  ability the engine gives a night elf builder) differs from the orc's (`AObu`, whose
  builder goes inside and comes out as the structure is done, vf209's `FUN_6f566dd0`) in
  its vf208 to vf210.  As the structure is begun (vf208, `FUN_6f5441e0`, from
  `FUN_6f54d0f0` once Build In Progress is given) the builder goes inside as an orc's
  does (`FUN_6f544140`), and when the structure's class has `ancient` (the unit's +0x248,
  its type's, bit 0x80; the classes' table at 0x6fce5fa0, read by `FUN_6f684340`: giant
  1, undead 2, summoned 4, mechanical 8, peon 0x10, sapper 0x20, townhall 0x40, ancient
  0x80, neutral 0x100, ward 0x200, standon 0x400, tauren 0x800) it also leaves each
  player's own selection (vf115 `FUN_6f6948d0` with -1, by `FUN_6f2928c0`, "Async
  remove": the synchronized selection keeps it), has its current order's tasks cleared
  (`FUN_6f673e80`), and leaves its owner's tech tree by its +0x64 bits (8, in
  production, `FUN_6f25c260`; else 0x10, present, `FUN_6f266f70`) and its food (0x800
  used, 0x200 made), the ability's flag 0x200 set.  As the structure is done (Build In
  Progress's vf205 `FUN_6f427e40`: after its finish events, its vf213 `FUN_6f422c10`
  calls the builder's vf209, `FUN_6f543eb0`) such a builder is removed
  (`FUN_6f694690(1, 0)`: RemoveUnit's without the unit's vf 0x84 and task 0xd0156); a
  cancel (vf210, `FUN_6f543ca0`) gives it back (present and made again, `FUN_6f25ba00`,
  or in production, `FUN_6f26b140`; its food) and lets it out.  So Mafarazzo TD's Tower
  of Vengence (e000, class Ancient, made from the Ancient Protector) takes its wisp.
  The port does so (`World::build`, `World::complete_construction`,
  `natives::units::remove_with_tasks`).  Traced over LAN (the maftw game, Mafarazzo
  TD's tree built by its host's player on a map without its start's waits): a night elf
  player's build order makes the AI's build order's objects as a peasant's does, its
  walk the engine's; begun, the Ancient frees none of them (its walk's path and cluster
  go as the walk ends).  As it is done, the wisp's removal makes its block and four tasks
  before the structure's five objects go, then frees (with the turn's releases) its
  first block, what is left of the order to build (its `tsk.`, three tasks, block, and
  `ord.`, the last first), its last block, tasks 0xd0144 and 0xd0178, its in-range
  listener, its spells' timers (Renew's, then Detonate's: the newest first) and its own
  listener, task 0xd0164, its abilities (the last first), its own objects, task 0xd015a,
  and last its detection's record (`Cdet`, made as its player's flying peasant first
  found it).  Its player's selection, holding it till the next selection action drops
  it, hashes it as 0 meanwhile, its object gone (a member reads its object's slot and
  birth; the port reads 0 for one whose object is no longer live).
- A structure's upgrade (an order, flags 0x40, the new type's id as its order) goes to
  its Upgrade ability (`Aupg`, CAbilityUpgrade; its vf3, 0x6f5a5990, finds the order
  among the type's Upgrade list, vf200 0x6f582970 checks it, vf192 0x6f581060 the
  cost), which makes an Upgrade In Progress (`AUnP`, CAbilityUpgradeInProgress,
  `FUN_6f56c660`, made with its real), gives it to the unit (`FUN_6f48b4c0`), and
  starts it (its vf212, 0x6f59cf30): the cost paid (`FUN_6f25b740`, `FUN_6f25b780`:
  the new type's, the old's given back unless RelativeUpgradeCost reads as set), then
  as UpgradeInProgressIdChange reads as set the unit's change of type at once
  (`FUN_6f670950` with 0x101; else only its tech tree and looks would change then, the
  type at the end), then the rise (`FUN_6f41b790`, `FUN_6f3f5950`: its modifier and
  threshold listener, `FUN_6f436b00`, event 0xd01c4), then its events
  (`FUN_6f67da80`, event 0x80242).  The change of type (`FUN_6f670950`) changes the
  owner's tech tree (`FUN_6f6703b0`: with 1 the old type, added, taken from present,
  `FUN_6f266f70` and `FUN_6f23b030`, and the new one begun, `FUN_6f26b140` and
  `FUN_6f249cf0`), the unit's abilities, and its sight, a modifier over 3 seconds as a
  change of day makes (`FUN_6f6708a0`, the unit's +0x110).  Measured under winedbg by
  the main session (Mafarazzo TD's host player's Tower of Vengeance, e000, upgraded to
  e001 at turn 21 and to e002 at turn 200, over LAN with the original): the change of
  type from `FUN_6f670950` (its return 0x6f670a8a) with 0x101, e000 and e001, the unit
  still e000 at its entry, and the tech tree's start of e001, both at 2.0000; the tech
  tree's finish of e001 (`FUN_6f22f6a0` from `FUN_6f262f30`) at 17.0000; the same for
  e002 at 19.8977 and 34.8977, the main clock as each turn's actions were carried out
  (the port's last main tick of the turn before, 1.999998 and 19.897661) and the new
  type's build time, 15, on.  Traced: the order makes `+ord`, `tskA`, `AUnP`, its
  `` `rel ``, the sight's `` `rcm ``, and the rise's `` `rcm `` and `` `rtl ``; in the
  next turn's first step, after the turn's timers due before (the leaderboard
  trigger's objects at 2.0), go the `tskA`, what the change let go, and the `+ord`
  (from e001 to e002, a type that upgrades to nothing, the Upgrade ability's real and
  then itself; from e000 to e001 the abilities stayed, both types' lists the same);
  the sight's modifier goes 3 seconds on (turns 51 and 230); the upgrade is done (vf205,
  0x6f5a75f0, then `CAbilityInProgress`'s vf205, 0x6f427f10: the tech tree's finish,
  then Upgrade In Progress taken away, `FUN_6f48e8a0`, then its events, `FUN_6f67d960`,
  event 0x80244) with the rise's listener, its modifier, the real, and Upgrade In
  Progress freed at once, in that order (turn 171), nothing made.  The port does so
  (`World::upgrade_structure_by`, `World::upgrades_to`): the mafv2 game's keepalives
  agree to turn 308 (the creep paths' known difference at 309), where they parted at 21.
  An upgrade (or a construction) due by a turn's first step's tick is done in that
  step, as in the later ones, before the step's ended walks' paths go: traced, the
  second upgrade (due 34.897661) freeing its four objects at the start of turn 350,
  before a creep's old walk's four; the port did them at the turn's end, after them.
  With it, and the walks' fixes, both upgrade captures (mafv2, mafv3) agree to turn
  396, the plain game's known difference at 397.
  The order pays the new type's whole cost (measured: Red's gold pinged every 0.1 s,
  30150 to 24150 at 2.0 for Super Tower of Vengence's 6000; the setting reader
  `FUN_6f047710` called at 0x6f59cf82 returned 1 at both upgrades' starts).
  Provisional: which setting that read is (MiscGame.txt has UpgradeInProgressIdChange
  and RelativeUpgradeCost 0, yet the first behaved as set), an order by script or by
  an AI, the event's data agent, and what an upgrading unit counts as in the AI's and
  the requirements' counts (the old type, as before).
  The type's change as the upgrade starts (`FUN_6f670950` from vf212 with 0x101, its
  ninth argument 0) remakes the engine's abilities alone (build, queue, Upgrade, move,
  attack); the abilities of the type's list are the old type's till the upgrade is
  done (vf205, `FUN_6f5a75f0`), which replaces them by the new type's abilList
  (`FUN_6f694f00`) before the rise's objects go and the tech tree's finish
  (`FUN_6f427f10`): the unit's abilities, the newest first (`FUN_6f48ca70`; an ability
  given heads the unit's chain, `FUN_6f48b4c0`), each kept when its flag 0x20 is set or
  its vf 0xf0 answers (provisionally, the engine's), when made permanent (+0x38), or
  when in the new list (taken from it, the list's last put in its place), else taken
  away with its objects at once (`FUN_6f6947e0`, `FUN_6f48e8a0`); then the list's others
  are added in its order (`FUN_6f4725f0`).  The port does so
  (`World::replace_type_abilities`): Mafarazzo TD's Fan Knifes Tower becoming the
  Assasin Tower frees its Moon Glaive's object (`Amgl`) and makes a Critical Strike's
  (`AOcr`), the Plague Tower becoming the Zombie Tower makes a Critical Strike's, and
  the Zombie Tower becoming the St George Bank frees its Critical Strike's and Slow
  Poison's and makes Pillage's (`Asal`), each before the rise's listener goes; it had
  changed them as the upgrade began, their objects kept.  Traced (the maftw game): the
  new ones' objects are made before the taken ones' are freed (the Assasin Tower's
  `AOcr` made, then the Fan Knifes Tower's `Amgl` freed, then the rise's objects).
  The change of type remakes the attack's splash objects (`spla`) by its two
  weapons' new kinds (`FUN_6f49cab0`, after the sight's modifier): one made as a
  weapon's kind becomes a splashing one (artillery, msplash, mbounce, mline, or aline;
  `FUN_6f4977e0`), kept while it stays one, let go as it becomes another (its
  vf 0x5c, then its last reference).  Measured: the maft game's Arrow Tower (a
  missile) becoming the Cannon Tower (an artillery) made one more object as the
  upgrade began, turn 212, born one past the port's and its slot the free list's
  last; with it the maft3 capture's keepalives agree for all 3273 turns
  (`World::retype_splashes`).  One let go is freed with the order's objects as the
  next tick begins, after the Upgrade ability's real and itself (traced: the Fan Knifes
  Tower's bounce becoming the Assasin Tower's missile, the maftw game's turn 1340).  A
  cancel (vf206,
  0x6f5a73b0) gives back UpgradeRefundRate of the cost and the old type
  (`FUN_6f670950` with 0x102); not traced.
- A built structure's Blight Growth (a human one's dispel, Abds: 64 every 0.08 to
  768) waits while Build In Progress is on it (`CAbilityBlightGrowth`'s vf105 and
  vf106, read as an ability's coming and going, vf106 from `FUN_6f48e8a0` as Build
  In Progress goes, `FUN_6f40bb00` and `FUN_6f4076e0`) and begins
  as it is done: its effect (`Xbli`, CEffectBlight, set up by the growth's vf32
  through `FUN_6f4904e0`) starts its timer (`FUN_6f491250`, event 0xd01c1 each Dur)
  and takes its first step at once (`FUN_6f490070`, the radius its count times
  DataA), its timer stopped and the growth's end run (`FUN_6f04c410` with
  `FUN_6f4901f0`, `FUN_6f423550`) as the radius reaches Area.  Traced: Twisted
  Meadows' human computer's first farm, done at 52.668034, its dispel's five objects
  (two regions, the position, `Xbli`, and `Abli`) freed as turn 536 begins, before
  that turn's first order: its twelfth step at 53.548 (the port had counted a turn's
  0.1 a time from the turn after, a turn late).  The port now counts a built one's
  steps on the main clock from its end, the first at once, and frees its objects in
  the step after its last (`World::update_blight`).  One made outright so too, from
  its making (below).
- An AI's harvest pass passes over a hidden worker: `FUN_6f9c7700` takes only a unit
  whose +0x20 has its flag 1 clear, which `FUN_6f688300` sets as a unit is hidden
  (the gold mine's let-in, `FUN_6f5c74c0`, hides its worker through `FUN_6f69c5c0`,
  which also counts up its +0x198).  Such a worker is neither taken nor flagged, and
  is counted at the work only as `FUN_6f9c7410` counts any (its state, +0x194, 5 or 6
  and its AI unit's flag 4 or 8 from the last pass, hidden or not).  Read: Twisted
  Meadows' human computer's farm peasant, sent to gold as its farm was done, in the
  mine at the pass at 55.2, given no order (its return order in turn 557).  The port
  had counted it against the wood request and ordered it not; it now leaves a hidden
  worker out of a request's takes unless it counts as at that work
  (`World::ai_assign_harvest`).  The keepalives agree to turn 602 either way (that
  request takes no other worker there).
- A worker the pass takes (`FUN_6f9c7700`: its AI unit's flag 1 for gold, 2 for wood,
  then `FUN_6f6802f0`, the `+ord` 0xd0006 run as `FUN_6f9bd6e0`'s look) that carries
  a load is sent home with it first whatever the request (`FUN_6f9c9470`: 0xd02b4
  while the harvest ability's +0x1c4 holds any), as the port's gold requests already
  did one walking home (`goldrun::RETURN_AI`); its flag keeps it from the pass's
  other requests (`FUN_6f9c7410` keeps only those flagged 4 or 8 from the last pass,
  `FUN_6f9c7700` takes only those with neither 1 nor 2).  Read by the keepalives:
  Twisted Meadows' human computer's farm peasant, walking home with gold at the pass
  at 57.2, taken for wood: the original's turns after agree with its walk home
  anew and no other worker re-sent (to turn 602), not with a wood order at once (to
  572).  The port marks it at the wood for that pass (`UnitExtra::ai_wood_pass`).  Next
  (turn 603): a gold peasant (1879048362) walking from the hall to the mine since
  58.5 arrives three steps early in the original.  The original's objects from turn
  585 (frees and takes by turn) free its walk's last two (the ghosting's end and the
  walk's target task) as turn 602's last, after the pass's frees, and its path and
  cluster in turn 603: an arrival in turn 602's last step (60.175), where the port's
  is turn 603's third (60.265), its path in turn 604.  Its speed is the port's (its
  tuning at 55.258, as `FUN_6f437ae0` does it, a factor of 0.931474; tuned instead as
  a queued worker goes in, tried, the keepalives part at turn 86).  Tried: the port's
  peasant moving from its walk's first step (58.466), instead of turning in place
  three steps from its heading on the hall (0.7655 to 2.5655, its window 60 degrees,
  moving from 58.556), arrives in turn 602's last step, and the original's objects
  then agree object for object through turn 625 (the next difference a lumber
  worker's reach at 62.5); with a head start of two steps (one turn in place) it
  arrives a step late; and without the turn at its walks out at 48.0 or 53.4 the
  keepalives part at turns 502 and 553, so those turned in place.  At 58.5 alone the
  original's walker starts as if facing its way (within the window of its first
  point's bearing, 3.036), or three steps on.  (Read since, below: it turns as the
  port's but walks from another place, put there as its drop ends.)
- That walk's provider answers, read (after `FUN_6f165ae0`, 0x6f16fd0a, 58.406 to
  58.676; the point at [ebp-0xc], [ebp-0x8], in cells): the walkers and their order
  are the port's one for one (two far units, the peasant walking home along row 188
  for (171.5, 190.5), a third far one; a far unit's new walk first from 58.586, and
  the mine's peasant out at 58.616 first from then, heading for (162.5, 184.5)).  The
  peasant answers 3 at 58.406 with the hall's middle (178, 194), its contact
  (`FUN_6f167290`, `FUN_6f166310`), is not asked at 58.436 (in reach, `FUN_6f16e910`,
  its +0xd8 0x10000 from `FUN_6f171060`), and walks first from 58.466, as the port's.
  But its point is (169.5, 185.5) from 58.466 to 58.646 and (161.5, 183.5) from
  58.676, each answer 0, where the port's is (170.5, 187.5), its way along row 187 to
  (161, 187).  As read: a search's way holds its points end first, the walker's place
  last (`FUN_6f166e90`); one that met a refused cell (`FUN_6f1489a0`'s +0xd0, a
  blocked cell or a region, as the hall's beside it) takes the first node after the
  start's cell (`FUN_6f168870`, count less 2), one that met none its end (index 0);
  and within 0.49 of its point it takes the farthest in a clear line (`FUN_6f167070`,
  `FUN_6f165e60`).  A walker in the hall's contact (its class 1 ring, x - 2 to x + 1 and
  y - 2 to y + 1, on the hall's 172 to 183 by 188 to 199: at its corner, x 171 or more
  and y 187 or more) can have neither (169.5, 185.5) as its first node nor come within
  0.49 of it in seven steps, so that point is the first node of a way searched from a
  cell beside (169, 185), or the end of an unrefused way to a point there (a coarse
  point, `FUN_6f167d70`).  The port's search from (170, 185), with the worker waiting
  at the mine as the port holds it (its region on 160 to 161 by 184 to 185), gives
  both points: (169, 185) first, refused, and its end (161, 183).  The timing fits too:
  a walker about 1.5 off (169.5, 185.5) moving at once is within 0.49 of it at 58.676
  and then 49 or 50 steps from within 0.49 of (161.5, 183.5), arriving at 60.145 or
  60.175, in turn 602 as the objects have it; from the port's place (171.16, 187.43)
  those points would bring it in at 60.29 or later.  Measured since (every walker's
  place, heading, and path at 0x6f16fd0a, 56.6 to 58.7): the peasant, its contact at
  58.406 at (171.1617, 187.4288) as the port's, stands at 58.466 at (170.5, 185.5),
  its heading 0.7655 kept, and searches from there (its way (161.5, 183.5), (162.5,
  183.5), (163.5, 183.5), (164.5, 184.5), and on, end first, index 8; its coarse way
  from (85.25, 92.75)); it turns in place three steps and walks from 58.586.  So its
  order home's end placed it again: task 0xd0152 (`FUN_6f422b10`, the harvest's +0x20
  holding 0x800 or 0x1000) gives the worker its own region and mask (`FUN_6f699290`)
  and places it where it stands clear of the units that mask meets (`FUN_6f427870`,
  its vf 0x180).  The one it was put clear of is the farm peasant (1879048396), home
  at 57.4466 at (171.0388, 187.4737) and standing there after (no walk of its own in
  the reading): the port had sent it on to repair its farm, done at 52.67 with 0.001
  of its 500 short, where a computer's repairs are its AI's (`FUN_6f9c3920`): a
  structure is taken only under its maximum life less `DAT_6fd3c76c`, 10 (set as the
  game loads, 0x6f001ac0, through `FUN_6f070d80`).  The first cell clear of it (more
  than a cell from its cell (171, 187) either way, the rings walked from the bottom
  row, `natives::units::ground_place`) is the second ring's first, (170, 185).  The
  port so places a worker at its drop (`World::place_off_ghosting`) and leaves a
  computer's structures under that margin (`World::autocast_target`; provisional: the
  AI's other tests and its timing, the autocast standing in); the keepalives agree to
  turn 632, from 602 (the farm peasant now walks to its tree from the hall, reaching
  it in turn 634, the original's in 637; and in turn 632 a peasant's first arrival
  at the mine, to wait, comes before that turn's trained peasant's AI orders' frees,
  the original's after them).
- An AI's wood workers take its town's trees in turn: each sent to a tree
  (`FUN_6f9c6cf0`, from `FUN_6f9c9470` as a worker its pass took for wood runs its
  `+ord`, and `FUN_6f9c9610`; `FUN_6f9c96c0` too) gets the next of the AI's eight
  places with a tree (`FUN_6f9c70a0`: CCompAI +0x150 counted on, 8 taken back to 0,
  the empty passed over), +0x150 7 as the AI starts (`FUN_6f9c7b20`), so its first
  wood worker has the cheapest; the nearest tree when there is none (`FUN_6f4153a0`).
  The eight (+0xac, their costs +0x110) are kept while their trees live:
  `FUN_6f9c55d0`, at each wood request (`FUN_6f9c7700`), enumerates the trees about
  each drop-off only while a place is empty (`FUN_6f9c7a20` empties a dead tree's
  place), a tree not among them taking the first empty place or, costing less, the
  first dearest (`FUN_6f9c5950`, `FUN_6f9c60a0`), and sorts them each time, each
  place swapped with the first cheapest after it.  Read: Twisted Meadows' human
  computer's farm peasant, sent to wood at 60.1 (the second so sent), reaches its
  tree (its walk's reach, the `REACH` objects) in turn 637 in the original; the port
  had sent it to the first tree, 15 cells off, reached in turn 626, and now sends it
  to the second, (192, 178), 21 off (with the walk out at 58.5 above given the
  original's start, the objects agree through turn 630, from 625).  Open: the
  original's reach is later still; the port's, walking from the hall since it no
  longer repairs (above), reaches that tree in turn 634.
  Read since (each walker's place and path at 0x6f16fd0a from 60.1 to 63.8): the
  original's farm peasant walks from the hall at 60.1452 as a cluster walker, its
  path's goal first its cluster's point (189.5, 179.5), straight for it from
  60.2951 (heading 5.8523), then from 62.99 the tree at (190, 174) (its walk to
  reach it from 63.74).  That tree is the third of the port's eight places ((186,
  174), (192, 178), (190, 174), ...), where the port's turn gives the second.
  Measured (a break at `FUN_6f9c70a0`'s entry, 0.1 to 61.0): one call, at 60.0951,
  its +0x150 0 (so the second place), the places' first four costs 21, 21, 22, and
  24.  So the places' order differs: `FUN_6f9c5950` costs each tree by
  `FUN_6f059590` from the tree to the drop-off, not from the drop-off, and the
  port's plans so give 21 for (186, 174) and (190, 174) and 22 for (192, 178), the
  second place the farm peasant's tree (`World::ai_town_trees`; the fourth, (196,
  178), 22 where the original's fourth costs 24, not yet read).  Next: the original's
  walk to it is a cluster walk, its way's point 10 on (189.5, 179.5), where it stops
  within its harvest's reach (4.01 off at 62.99; Ahar's 116 and its 16 over 32 is
  4.125) and plans anew to the tree; the port walks to the tree's footprint straight
  and reaches it in turn 634, three before the original's.  Tried: a lumber run's
  walk so stepped through its cluster's point (`World::coarse_goal`,
  `World::coarse_step_arrive` with the harvest's range), the keepalives part at turn
  29 (the first wood worker's walk at 0.1, 23 cells, so too, not read).  Read since
  (every walker at 0x6f16fd0a, 0.1 to 3.0): that first walk's path has the tree,
  (186, 174), for its goal from the start (its coarse way's index 0), where the farm
  peasant's at 60.1 has its cluster's point; why the two clusters differ is not yet
  read.  The eight places' costs at 60.1 are 21, 21, 22, 24, 24, 24, 24, and 26; the
  port's fourth, (196, 178), costs 22 by its plan, the rest as the original's.
  Read at the clusters' plans and picks (`FUN_6f169840` at 0.1 and 60.095,
  `FUN_6f1697a0` the step after): at 0.1 the pass's six clusters plan in turn (four
  to the mine, the wood worker's fifth, the builder's last), each picking the step
  after; the wood clusters' fields at their picks alike (the tree's handle at +0x40,
  the member's place at +0x54, +0x70 5.63 and 5.67).  The port's plan from the farm
  peasant's place at 60.1 to its tree has the original's coarse points ((94.75,
  89.75), (92.75, 91.75), (90.75, 92.75), (88.75, 93.75), ... in accelerator units)
  and its point 10 on is the original's (189.5, 179.5); from the first wood worker's
  at 0.1 its point 10 on would be (185.5, 179.5), where the original's member goes
  for the tree.  Read since (breaks after `FUN_6f166c30` at 0x6f165c02 and
  0x6f167278, after `FUN_6f1697a0` at 0x6f16c565 and 0x6f16cef4): every plan at 0.12
  and 60.115 returned 1, none refused.  The difference is the tree: a coarse plan
  lifts its path's target's region out of the accelerator for its search, and its
  walker's own (`FUN_6f166c30` calls `FUN_6f15d360` with 1 for the path's +0xa0 and
  +0xa4 before `FUN_6f162cb0` and with 0 after; `FUN_6f15cf80` clears the states of
  each level-0 cell of the region's rect, its cells halved and floored by
  `FUN_6f15ba80`, the levels above built again by `FUN_6f15d470`, and with 0 works
  them out again from the map).  A cluster's path's +0xa4 is its target's region
  (`FUN_6f16ce10`'s `FUN_6f168bd0` with `FUN_6f16afc0`: the cluster's +0x78, else its
  target unit's +0x98).  With the tree's cells (184 to 187 by 172 to 175) lifted, the
  first wood worker's way from (82.52, 95.07) is the original's, (93, 87), (88.75,
  91.75), (86.75, 93.75), (84.75, 93.75), and its pick (`FUN_6f165e30`,
  `FUN_6f167ae0`, the legs summed from the start) the tree itself; left in, the way
  ends at (94.5, 88.5) short of it, its point 10 on (92.75, 89.75).  The farm
  peasant's at 60.1, its tree's cells lifted, picks (94.75, 89.75), the original's
  (189.5, 179.5).  The port now walks an AI's wood worker to its tree by such a
  coarse way (`World::coarse_goal`, `World::lift_region`, `accel::Pyramid::lift`), its
  point arrived at within its harvest's range (`World::coarse_step_arrive`: Ahar's
  116, 4.125 cells, the farm peasant in reach at 62.8437 4.01 off, as the
  original's).  Then its search from (185, 181) to the tree (five nodes through open
  ground, the original's (185.5, 180.5), (185.5, 179.5), (186.5, 178.5), (187.5,
  177.5)) has the original's index 3, node by node: the ring of a popped node is
  tested cell by cell (`FUN_6f1489a0`: a cell refused sets the searcher's +0xd0; one
  with the target's region, its +0xa8, sets +0xcc), the last node's too, and that
  ring, (185, 175) to (188, 178), meets the next tree's cells (185 to 187 on row
  175, the tree at (186, 174)), where the port's search stopped at the node touching
  its tree untested and had met no refused cell, so went straight
  (`World::gold_run_search`, `lrpath::last_ring_refused`).  Provisional: whether the
  target tree's own region refuses (refusing on it alone parts Forestwalk's computer
  game at turn 517, its ghouls' walks), and the same for a mine's or a hall's (left
  as they were).  And it stood by the hall from 57.4466 at (171.0388,
  187.4737), where the port's was pushed to (171.21, 187.63) as the mine's peasant,
  ghosting, came home past it at 58.406 (`World::separate_units`, the port's own
  overlap push, now leaving out a worker ghosting on its run).  Its walk to its tree
  is so the original's to the step (its reach's objects in turn 637); the keepalives
  still part at turn 633, by turn 632's frees (above: the peasant waiting at the
  mine, its objects freed before that turn's trained peasant's AI orders' frees).
- A computer's unit trained in the steps lets go in the main tick after, before the
  next step.  Read (every tag free from 63.05 to 63.25, its clock and backtrace, in
  Twisted Meadows' human computer's game): the peasant trained at 63.096783
  (`CAbilityQueue` vf3, `FUN_6f40daa0`, its AI's harvest given through
  `FUN_6f9b9230` and `FUN_6f9c9cc0`, the order's `^clg` freed at once), then at
  63.096882 the training's `` `rtl `` and `` `rcm ``, its AI's orders' objects in
  turn (29), the last order's four tasks, the making's two, and the ptcf, all from
  the agents' destroy list (`FUN_6f04c1a0`'s `FUN_6f04f7e0`, `FUN_6f054190`,
  `FUN_6f15e500`), and only at the step at 63.113529 the waiting peasant's cluster
  and path (`FUN_6f16c150`, `FUN_6f1699c0`).  The port had freed the orders'
  objects as the turn ended and the tasks and ptcf as the step ended, after that
  step's; it now frees both, in that order, as the training is done in the steps
  (`World::trainings_to`, for a computer's unit given its harvest as it is made).
  And a worker's walk to reach its tree goes on till the tree is within its
  harvest's range: the farm peasant's move ended at 63.5633 in contact 4.70 cells
  from its tree's middle (range 4.125); its reach's way, searched at 63.6232 from
  (187, 177), is a plain one to the tree's middle, ending at the node nearest it,
  (189.5, 177.5), node by node (its index 1, (188.5, 177.5)), and it walks on from
  63.6532.  The port had ended every reach in the step after it began (as the reads
  at turns 854 and 990, in range from the start); it now walks the reach by such a
  search till in range, stops and turns there, and has reached it then
  (`World::gold_run_step_end`, `World::reach_within`, `World::walk_goal`).  The
  keepalives agree to turn 640, from 632.  Next (turn 639's frees, the keepalives
  parting at 641): the reach's task goes where the original's does, its path and
  cluster right after it where the original's go after seven others (an older
  order's tasks), not yet read why.  Read since (each free at the tag allocator,
  63.6 to 63.97): the task and the seven (26282 to 22424) go together at 63.833233
  from the destroy list (`FUN_6f054190`, `FUN_6f145c70`), and the reach's path and
  cluster only at the next step, 63.863117, from the cluster's own update
  (`FUN_6f16c150`, `FUN_6f1699c0`, in its `CAgentTimer`), before another cluster's
  (22457, 26378); the port frees the seven after the path.
- A computer's structure whose training queue empties has its abilities enabled
  again, and a Tree of Life's Entangle Gold Mine makes its listener anew.  Read (the
  tag allocator's take of a `` `pil ``, 14.05 to 14.25, Twisted Meadows' night elf
  computer's game): one at 14.1, after the trained wisp's ptcf, its backtrace
  `CAbilityQueue` vf3, `FUN_6f40daa0` (its end, `FUN_6f406c40` with slot 0: the
  queue shifted, and with nothing next `FUN_6f41de50`), `FUN_6f48c8c0` (the unit's
  abilities' disable count +0x1b8 down, each ability's `FUN_6f40bb00`), and
  `CAbilityEntangle`'s vf 0xc8 (`FUN_6f05c020`, a listener on event 0xd01aa), which
  `FUN_6f40bb00` calls as an ability's count +0x3c reaches 0 on a unit with +0x5c's
  4 (an AI's).  So the computer's Tree of Life's listener the port had called its
  Eat Tree's is Entangle's, let go as it begins training (its abilities disabled;
  provisional, `FUN_6f4076e0` not read) and made again as its queue empties
  (`World::update_training`).  And the trained wisp's tree is its town's next
  (`FUN_6f9c96c0`'s `FUN_6f9c70a0`), (150, 202) as the original's walk has it, where
  the port had given it the cheapest as at the setup (`World::town_next_tree`).
  The keepalives so agree to turn 152, from 142.  Next (turn 152): the original's
  AI pass at 15.1 sends that wisp to its mine (a 19-object order, its old order's
  objects freed), where the port's leaves it, a worker made since the pass before
  last (12.1) not yet among those its passes take (`World::ai_assign_harvest`'s
  joining, read on the human computer's peasants made in a pass's tick); not read
  what the original's AI list takes.
- An AI's harvest pass takes its workers from its unit list, which a unit joins as
  it is made (`FUN_6f9b9230` from `FUN_6f677870`: the night elf computer's wisp at
  14.100000), with no test of its age.  `FUN_6f9c7700` walks the list (AI +0x28,
  newest first; its head at 0x6f9c7783) and takes a unit whose +0x20 has 1 clear
  (not hidden) and its harvest's flags (0xc0000 for wood; 0x8000 or 0x10000 for
  gold), whose AI's +0x28 has 1 and 2 clear (not taken this pass), and whose +0x194
  is 0, 5, or 6 (7 too for a ghoul); it sets the AI's +0x28 1 (gold) or 2 (wood).
  Read (every unit at the loop's head and each take): the night elf computer's pass
  at 15.098467 takes the wisp trained at 14.1 (+0x194 5) for wood first; the human
  computer's at 15.098467 lists its five first peasants only, the one trained as
  turn 152 began not yet made; at 33.095 its newest peasant is in the mine (+0x20
  0x149447) and passed over, and at 35.099 taken for gold.  The port had kept from a
  pass every worker made since the pass before last; it now keeps only one made in
  the pass's own tick (`World::ai_assign_harvest`; provisional: the port runs that
  pass after the turn's first trainings, where the original's ran before), and the
  wisp is sent again at 15.1 as the original's.  The keepalives agree to turn 155,
  from 152, the objects to 163.  Next: at turn 164 the port's Moon Well is begun, the
  original's at 178.
- A computer's wisp made as the game runs counts its town's tree counter on twice.
  Read (`FUN_6f9c70a0`'s entry and return, Twisted Meadows' night elf computer's game):
  at 14.100000, as the wisp is trained, it runs twice from `FUN_6f9c96c0`
  (0x6f9c97f3), at the AI's +0x150 0 and then 1, and at the pass at 15.098467 at 2;
  the eight places' costs 21, 22, 22, 24, 24, 24, 24, and 24, as the port's.  The
  wisp's walk from 14.1 is to (150, 202), the first's (the port's place 1), and its
  walk from 15.1 for the cluster's point (169.5, 175.5) toward (172, 170), the
  port's place 3 (the wisp then begun on the Moon Well from (163, 185) at 17.8).
  The port had counted on once, sending it at 15.1 to (148, 198), place 2, the well
  begun at 16.4 (`World::tag_unit`; provisional: which of its two harvests takes
  which call's tree).  The keepalives agree to turn 178, from 155.  Next (turn 178):
  the original makes a `` `pil `` with the well's beginning, after the builder's order
  and before the structure's `pucs`, which the port does not.
- A computer's Moon Well's Replenish makes its listener as the well is made, and a
  structure begun lets go as the turn ends its regeneration's modifier, its spells'
  timers, and those listeners.  Read (the tag allocator's take of a `` `pil `` with its
  backtrace, 17.6 to 17.9): at 17.788015 `CAbilityManaBattery`'s vf 0xc8
  (`FUN_6f05c020`) from `FUN_6f67dff0` (its abilities' AI) in the making's hook
  `FUN_6f9b9230`, under `FUN_6f54d0f0` (the build); and in turn 178's frees, after
  the build's `pucs`, the well's `` `rcm `` (its regeneration's, the build's own taking
  its place), Replenish's `` `rtl ``, and that `` `pil ``.  The port now makes the
  listener for a computer's well (`World::tag_unit`, as Entangle's) and frees the three
  as a structure is begun (`World::build`, with its Build In Progress; provisional:
  why, the abilities read as disabled while it builds).  The keepalives agree to turn
  302, from 178.
- A training falling due in one of a turn's main ticks after its first is done there,
  after the computers' threads due before it.  Read (`GetPlayerState` and
  `SetProduce` at 29.75 to 30.25, Twisted Meadows' night elf computer's game): its
  thread at 30.095949 (the turn's first main tick) reads 250 gold and 60 lumber and
  asks for a wisp (`SetProduce(1, 'ewsp', -1)`) while its Tree of Life still trains
  one, due 30.098299, so its build loop stops there (common.ai's `OneBuildLoop`, a
  `StartUnit` refused) and no Ancient of War is asked for (no `FUN_6f9cb7c0` from 29.9
  to 30.7).  The port had made the wisp as the turn began, before that thread, its
  Tree of Life then idle and the Ancient of War begun at 30.1; it now runs the threads
  due before a training's main tick first (`World::train_before_steps`).  The
  keepalives agree to turn 304, from 302, the objects to 346 (next: at turn 347 the
  original's structure begun, the port's at 348).
- Work in progress in the original's saves: a structure being built has its Build
  In Progress (`ABnP`), its real (the ability's reference) its build time falling 1
  a second from its stamp on the agent clock; a queue at work (`Aque`) lists its
  seven places' ids after the ability's fields, two reals, and a word, its real the
  first's time likewise (a stamp before the clock's last wrap at 300 seconds counts
  from there: a research's at 279.76 with the clock at 1.21).  Loading such a save,
  the structures go on building (the hero map's two at 450 seconds) and the queues
  on working (its computer's next level of Rura at 900 seconds, done 54 seconds on),
  their objects as the port's own starts take them, a structure's stamped as long
  ago as it has been building.
- Upgrades researched (`+Wup`, CPlayerUpgrade vf14: its agent, then its player, the
  upgrade, 0, its level, and 3, and four places for its effects' objects; found by
  player and upgrade, `FUN_6f6c9b40`, its level +0x2c the fourth saved word,
  `FUN_6f6c9b00`): made as its first level comes in, then an object for each effect
  its data names (CUpgradeArmor and the rest, own `FUN_6f6b5140`: 0, the effect's
  base and amount a level from UpgradeData, 0; no tail), all without handlers (read:
  the hero map's undead computer's nine at 900 seconds, Unholy Strength's `ratd`
  born right after it).  The port makes and writes them; loading the original's
  save, each player's levels come back and apply.  Provisional: the third and fifth
  words (0 and 3 in every one read).
- Heroes and abilities in saves: every ability's level less one is its word at +0x50
  (its eleventh own field, as GetUnitAbilityLevel reads; the port wrote 0, every
  skill level 1 in the original); a hero's experience is its hero ability's first
  word (+0x8c, GetHeroXP's), its skill points the second; its level is its level
  flag (the hero ability's first `flg`, the level less one, GetHeroLevel's +0x6c);
  its strength's word (third) and agility's (tenth) and its intelligence flag (the
  second `flg`) are its own, without items.  Provisional: the reals after them (the
  Dreadlord at 900 seconds: 28.0 and what 21 agility and 20 intelligence give, its
  own 23, 18, and 17, which its items, a ring, potions, boots, wards, and a scroll,
  do not explain).  Loading the original's save, each hero's experience, level,
  skill points, attributes beyond its type's, and skills come back (the hero map's
  two Dreadlords at 900 seconds, their skills at the levels saved) and each
  Inventory's items into its slots.
- The port's own waiting triggers: a trigger's execution (`+tex`) is kept while a
  thread of it sleeps and freed as that thread ends (the original's save holds it
  with the wake timer), its timer started as the thread sleeps (the agent timer's
  count), and written as the original's (`World::execution_records`); a sleeping
  thread records its sleep (`real`).  The trigger's fourth word counts its
  executions still running.  TriggerRegisterTimerEvent's registration (`tmvt`,
  CTimerEventReg: the registration's words, then its timer, the trigger's, due at
  its timeout, event 0x80204, once unless periodic, stopped once fired, its timeout
  in chunks) is its event handle's agent and on its trigger's list, as the
  original's; a timer event's trigger hears EVENT_GAME_TIMER_EXPIRED (4) as its
  event id.  An agent timer's kind word picks its clock on loading (`CAgentTimer`
  vf9: bit 4 the agent clock, else the main), and its timing registers as saved
  (`FUN_6f060840`), nothing checked.  Measured: the original wakes a port save's
  waiting trigger (a preload file written as it wakes, Echo Isles made to wait at
  50 and saved at 59).  Open: the woken thread's own SaveGame writes nothing in
  such a game (one the original saved itself writes it).
- The dynamic shops (Echo Isles' Marketplace `Asid` and mercenary camp `Asud`:
  CAbilitySellItemDynamic and CAbilitySellUnitDynamic vf14) save as the plain shops
  and then a word and twelve (`w 12(w)`: 11, eleven 0, and 1 at 60 seconds, their
  goods not yet come; read from the original's save of the map made to save then).
  Written empty, the original called the port's 10-minute save corrupted; written
  so, it loads and plays.
- An aura bearer's own buff, made with it, is written at rest when the port's sweep
  has it off (a Death Knight's at 60 seconds of the every-unit map, else empty).
- A buff's own timer (CBuff vf211 `532b50`, event 0xd01c4) is written running to
  its end for every buff whose tail has it (`[r] w t ss f`), its span the buff's
  time as laid (an aura's its scan's); a buff keeps its slot's birth, and one whose
  slot was let go and taken since is left out.  So too a unit's day change's
  modifier, freed three seconds on (18 units naming one at 1800 seconds of the hero
  map froze it in the relation loop above), its birth kept with it.
- Relation lists (`tools/saves/checklinks.py` over a dump): in the original's every
  list is whole (each neighbour naming it back, each owner's head naming the first
  of its list, the newest).  A dying agent's relations are let go in a loop over its
  head (`FUN_6f145c70` -> `FUN_6f146590` -> `FUN_6f146650`) that never ends when the
  head names a relation not first in its list: the every-unit save froze so a
  minute after loading, as units died.  The port's were broken three ways: a creep
  made asleep, or falling asleep, kept naming its spells' listeners freed (their
  slots another unit's listeners after); kept slots taken since by others' objects
  (a flyer's making's in-range listener, freed as it slept); and the heads reckoned
  from other sets than their chains (a unit's newest modifier among its kept slots,
  not its aura buffs'), now each chain one function its records and its owner's
  head share.  A unit's in-range listeners are chained on its position too, newest
  first (the original's, its `` `pcu ``'s second head their first).
- Unit sprites (the same map): a model with the _V1 version where its file's flags
  have 2 (a Sorceress, a Lich, the murlocs); stands where a model has no plain one
  chosen among its stand sequences by the unit's animation properties and the tags a
  stand may carry, "VarN" a variation (a Guard Tower's 'Stand Upgrade First Ready
  Attack', a Sea Giant's 'Stand Ready', a Chaos Kodo's 'Stand Channel'); a model with
  none (a Locust) one empty entry, its sequence 0xffff.  Empty stand lists crashed
  the original as a unit attacked (FUN_6f1a42b0).  A Sphere's effects (its list of
  art) written empty: its null references crashed it likewise.
- The aura buffs' saves by class: Trueshot's, Command's, and War Drums'
  (`FUN_6f4ea860`) the base's, a per-weapon list, and a timer; Thorns' and
  Vampiric's the base's alone (`FUN_6f42e4c0`: its extended timer; read, the
  Dreadlord's at 900 seconds); Endurance's (own `FUN_6f4eba40`, two reals more; save
  `FUN_6f4ea910`, a timer after the extended one).  The port wrote Trueshot's for
  all.  Vampiric Aura's targets 0x4307006.  Not yet run: the aura's look every 2
  seconds (event 0xd01c5, repeating) on a hero's learned aura.
- A computer's or creep's unit's AI (`AI-U`) the port kept after its unit was gone
  was written empty; it is left out, its slot freed.  A creep's mana's newest
  listener was taken from slots it once had without checking them still listeners
  (a slot let go as it slept, then a walk's cluster).
- Not yet written nor run: a ghoul's lumber harvest's second timer, every second
  (event 0xd01c2, `FUN_6f3f4bd0`, kind 2, repeating), started with its harvest
  order's tasks (`FUN_6f419530`) for a ghoul alone ('ugho', with its unit's flag
  0x5c & 4); a worker waiting with its walk home blocked (`tskO` 852327, 5000); a
  lumber walker's sprite (the original's sequences 12, then 8 and 18).
- Still not written: the Graveyard's effects, a haunted mine's payout modifier and
  listener, and the running game's counts and search marks.
- The agent timers' counts (each start's, saved in its timer's record) with the
  computer, read against the original's starts to 2.5 seconds (`abtest/tmrstart2.sh`
  on `fw25.w3x`, 1258 by then; the port's 1227): by kind all agree but these.  At
  the load: a `FUN_6f240667` start before the units are made and another after the
  wanderers' waits; each Shadowmeld unit's (`FUN_6f51cf50`, 9) as it is made; at the
  first dawn each Shadowmeld unit's fade timer (`FUN_6f509170`'s event 0xd01be, 9,
  then 2 more by `FUN_6f50a9e0`), whether or not its fade makes objects; two
  wanderers' waits drawn again as MeleeClearExcessUnits removes them (`FUN_6f5aa910`,
  between their animations' timers); after setup `FUN_6f9d3283` (2), `FUN_6f526940`
  (8, by `FUN_6f4ffff1`), and 2 more fade timers; the auto-target spells' checks 28
  at 0.015 (the port's 36 at 0.0); a blight look restarted every second for two units
  (`FUN_6f43a050`, from 1.36 and 1.78); the haunted mine's count change
  (`FUN_6f4e95e0`, event 0xd01c5, at each acolyte's arrival).  Not yet followed.  In
  the Forestwalk game with an undead computer the original's first keepalive has 37
  births more than the port's, all else (the generator, every module) agreeing.  A unit joining its owner's AI sets its flag 4 at +0x5c, which the
  structures' alarm watch asks for; a computer's AI object is made as the map loads,
  so its units join as they are made, the AI script or not.
- The JASS handle table's references (CGameState's record, the counted list before
  the handles by agent; read against the quiet test map's 1736 at five seconds, every
  one agreeing): a handle's object's own while it lasts; each global holding it, but
  not a value its initializer set (Blizzard.j's timers and groups made so count 1);
  each returned function's local that held it, with multiplicity (Player(12) 33); each
  local of a thread asleep or kept within SaveGame; and the expired timer a sleeping
  or saving callback's context holds (also the game's sixth list of handles).  A
  destroyed object's handle keeps its references, its agent none, its birth in the
  second table (the gold mine `bj_meleeNearestMine` held, removed for the haunted
  mine).  The rest of CGameState's record agrees byte for byte from the port's state
  (`Game::game_state_second`): the trigger queue, its keyed handlers (the time of
  day's listeners with their events' agents, and those replaced since the game began,
  empty, until a load), each neutral building's registration (Aneu, event 0xd01a6,
  also on its event list 0x1a6), the time of day's listeners, the two periodic game
  timers (every 0.4 seconds and every agent step) from the clock SaveGame ran at.
- The time of day's listeners (Blizzard.j's day and night sounds, the day's change, the
  day's end): an equal registration listens at least or at most by the side the time
  is on (dawn remade at most by the melee setup's 8:00); event 0x80101 plus the
  comparison (less 0, at most 1, equal 2, at least 3, greater 4); a rising one ahead of
  the time checks every 4 seconds from its making, the original's counts those of its
  re-arms (1133 to 1136 at five seconds), which the port does not count yet.
- The second table's camera (+cam and its fourteen properties) from the port's camera,
  all values to the bit (angles as `deg * pi / 180` in single precision, but the 85
  degrees' in double; the eye's place the run back from the target in cells), but for
  the client's clock stamps, search marks, and the target's ground height.  The
  original honors a saved camera: a quiet test map save given Forestwalk's looks where
  that one did; its target point alone does not move the view.
- Saves without a template (`Game::write_save_fresh`, `playtest w3zfresh`): the
  original loads the quiet test map's at 5 seconds and the training test map's at 10,
  and plays on.  The terrain part's doodad bytes are one each for the map's doodads
  that are not destructables (a destructable's type flagged 3 is scenery): 603 of
  Booty Bay's 2011, all 2011 crashing the original as it loads (found with `w3zedit
  graft`, which takes pieces of another save).  Objects the port writes no records for
  yet (a walk's order, tasks, cluster, and path; captains) are left out and their
  slots freed, their units standing; the objects' word (the Tempest checksum) is 0.
- A map loaded directly (-loadfile) draws as the game starts, on the main generator
  alone (traced under winedbg with the breakpoint on `FUN_6f1b7130` conditioned on the
  main generator, `[0x6fd53a48]`; `drawtrace.sh`): a race for each of the 12 players
  whose preference is random, all but a map's slot left open (`FUN_6f1e9dd0`), then a
  shuffle of the free start locations for each team placed (`FUN_6f1e9e60` from
  `FUN_6f1e9fd0`, `FUN_6f1d1460` on the main generator); then the wanderers' first
  waits.  The quiet test map: 12 and 1; Forestwalk (its third slot open): 11 and 3,
  where the port drew 13 (a weighted choice's real, then 12 races: the quiet test map's
  count by chance).  The weighted choice's real (`FUN_6f1c0c40`) is another
  generator's (0x6fd685ac).  With that the undead computer's hero picks (common.ai
  PickMeleeHero) and all 944 of its AI script's globals agree with the original's at
  Forestwalk's start (the map's seed locked, 'war3').  A setup screen's game, with its
  per-team placement shuffles, already agreed (the Forestwalk network capture).
- Forestwalk's saves (the test map saving at 0 and at 5 seconds, `fws.w3x`,
  `fws5.w3x`, `-loadfile`; `abtest/savemap.sh`): every record of both is read and
  written back whole.  The classes' layouts are each class's save functions, found by
  RTTI (`VfList.java`: a class's vftable and its vf 0x38, the second record, and vf
  0x48, its own fields): the auto-target spells' base `FUN_6f42d2a0` (the buff base's
  tail, a reference, three timers: Curse, Bloodlust, Faerie Fire, and Heal's kin), a
  two-timer base `FUN_6f42e2b0` (Sleep, Rejuvenation), CAbilityAnimateDead
  `FUN_6f4e9990`, CAbilityDevour `FUN_6f5aadb0`, CAbilityManaBurn `FUN_6f648560`,
  CAbilityRaiseDead `FUN_6f5221e0`, CAbilityShadowMeld `FUN_6f522420`,
  CAbilityBurrowDetector `FUN_6f42d860`, CAbilityPurchaseItem `FUN_6f42df00`,
  CAbilityDevourCargo `FUN_6f5aabb0`, CBuffAuraUnholy `FUN_6f4ea950`, CSplashData
  (own `FUN_6f49c330`, a weapon's splash targets, three areas, two factors),
  CTriggerExecution `FUN_6f2996d0`, CDetectedData `FUN_6f696040`.  Ghidra's `vfN`
  counts entries: vf50 is offset 0xc8 (Cannibalize's `FUN_6f606220`, the creeps'
  Animate Dead's `FUN_6f6060e0`, Mana Burn's `FUN_6f606c60`: each a look every 2
  seconds, event 0xd01bf, started as the unit joins its AI).  A creep's copy of a
  spell has its own class, which the save names: the creep Animate Dead's data code is
  ACad, though the port plays it as Animate Dead (AUan).  A shop's sold items each yet
  to arrive start their arrival timer as it is made (eight of Forestwalk's eleven).
  The original loads the port's template-free save of Forestwalk at 5 seconds (its
  computer's captains and spawned threads left out, its walks dropped) and runs on.
  Still apart at 5 seconds: the computer's AI from 0.1 seconds (below), and timer
  counts some ten to fifty low in the port (timers the original starts that the port
  does not yet).
- Other melee maps, two people, no actions (the original as player 2 of the port's
  host, `abtest/netmap.sh`; its modules at a keepalive, `ckmod.sh`; its main
  generator's draws with callers and turn marks, `netdraws.sh`; its census at the
  start, `run.sh`): Harrow agrees for 1952 turns and Plunder Isle for 1953 (kept in
  `docs/reference`) after four fixes, each read so: an autocast toggle of an ability
  the unit lacks is refused (no objects; Harrow's script's "blackarrowoff"); a mud
  golem's Slow listener re-arms as the script sets its mana to none; Trueshot Aura's
  buff reaches melee units too; an aura's scan and a detection look reach a unit
  within range of its collision circle.  Hills of Glory agrees to turn 76: a critter's
  wander walk from (2512, 3664) to (2424.1, 3555.5) ends at once in the port (its
  search near at hand reaching one cell west, then nothing nearer), drawing its next
  wait on the main generator, while the original's walks on.  The way is open three
  cells south then two west; the goal's cell lies beside blocked ones.  The pathing
  maps agree there (a save at one second, `hog.w3x`: the cells' flags differ only in
  blight, 992 of them).  Its search (`World::local_path`) is refused by a sheep idle at
  (2480, 3600), its region's 2 by 2 cells across the narrow way south (236 to 237, 336
  to 337); a detour by the east side is open, past the wanderers' search budget (32
  nodes) perhaps.  Not so: in the original that sheep stands elsewhere.  A new ground
  unit's room (the cells about it walkable, `ground_place`) is its size class's
  footprint as the searches take it (`lrpath::footprint`: class 1's 2 by 2 cells below
  and left of its own, class 2's 3 by 3 about it, class 3's 4 by 4), not a square of the
  class's reach about it: read in the save at one second, gnolls and ogres stood where
  the map made them beside cliffs (199.508, 280.172 in cells, where the port had moved
  one to 199.5, 279.5), and two sheep, the first of which the port moved onto the
  second's spot, which then moved into the narrow way.  With it Hills of Glory agrees
  for all 1952 turns (kept in `docs/reference`).  Ogre Mound agrees for all 1953 as it
  was.  Lost Temple, four players, takes a lobby of four slots (the host's
  `W3HOSTCLOSED=2`; with three the original leaves at the countdown): it agreed to
  turn 752, where creeps' mana listeners fire, the original's at a creep the port had
  removed at turn 1 and the port's at one the original had.  Melee clears the creeps
  near the players' start locations, and the two had placed the players at different
  ones: the original's shuffle (`FUN_6f1d1460`, teams together's and random
  placement's) swaps each entry from the second on with one at or before it, where the
  port walked back from the last (the same on two start locations, one swap).  With
  it Lost Temple agrees for all 1804 turns captured.  Without Teams Together (the
  host's `W3HOSTSETTINGS=0x60802`, the port's `TAGS_FLAGS=608`), random placement
  shuffles the free locations anew for each team in turn, its members taking the
  first (`FUN_6f1e9e60`, `FUN_6f1e5060`; read), where the port shuffled once for all:
  five draws here, not three.  With one player a team the two placements draw alike,
  and the original's keepalives were the same either way on the 1803 turns two captures
  shared; the port agrees with the second for all 1952.  Echo Isles, the first map of
  The Frozen Throne tried, agreed to turn 2005 and parted at nightfall: after ten of
  the units' objects of the change the original made a `` `rcm `` and a `` `rtl `` more
  (call stacks: the unit's handler for the change, `FUN_6f690840`, sends it event
  0xd01d6, which Shadowmeld's class takes, `FUN_6f51d440` to `FUN_6f51d0a0` to
  `FUN_6f5067b0`), one pair for each Murloc Nightcrawler: an idle melder not hidden
  begins its meld with the night, its fade's timer and listener freed as the fade ends
  (the port's now over DataB, as the day's fade is; provisional, though the keepalives
  agree through the frees), hiding then.  With it Echo Isles agrees for all 2653 turns
  captured.  Twisted Meadows (four players, two slots closed) parted at turn 7: at the
  auras' first scan the original gave Trueshot's buff to two of each four units about
  the troll warlords, the trolls and not the ogres.  Trueshot reaches a unit with a
  ranged weapon on, its first or its second: Harrow's granite golems, melee first,
  throw missiles with their second (its data's Melee Bonus 0 and Ranged Bonus 1, DataB
  and DataC, say as much).  With it Twisted Meadows agrees to turn 1784.  There the
  original's takes had the port's codes in other slots, its free list having parted in
  turn 1782: a wanderer's order given before the turn's first step, and two walks
  arriving in that step.  The original freed the order's eight before the arrivals'
  trees, as a later step does (`Tags::end_step`); the port had freed them after.  With
  it Twisted Meadows agrees to turn 2005, nightfall.  There all but one walk's tree
  matched: the creeps falling asleep free their spells' listeners with the last
  change's listeners, as the change ends, not as the tick ends (the original's came
  before the tree of a walk that arrived in the turn's last step).  With it Twisted
  Meadows agrees for all 3752 turns captured.  Gnoll Wood (six players, four slots
  closed) agreed to turn 1217: a sheep's walk ended there a turn late, its next wait
  drawn after the original's.  Its search, from cell 468, 402 to 464, 409, was refused
  by trees and stopped by its budget, 33 passes, its best node the goal's cell: so the
  original's too (read under winedbg after `FUN_6f14aa10`: -1, 33 passes, best 464, 409,
  start 468, 402), whose way then ends at that cell's middle (`FUN_6f148100`,
  `FUN_6f147f90`), as the port's.  The port walked to the middle, searched again, and
  turned to the goal; the original's walk ends as soon as the goal itself is within
  reach (`FUN_6f16fbd0` tests arrival against a point apart from the waypoint it steers
  by; which point, read only so: provisional).  With it Gnoll Wood agrees for all 3752
  turns captured.  Emerald Gardens (twelve players, ten slots closed) differed from the
  load: the port had twenty objects more, a `` `rtl `` after each of twenty Mur'gul
  Snarecasters' making (the census at the start, `abtest/run.sh`).  Their mana set to
  none by the map's script, the port re-armed their Slow's listener as it does a mud
  golem's; but they are made asleep (units are made at midnight, before melee sets
  8:00), and a creep asleep re-arms none.  With it Emerald Gardens agrees for all 3753
  turns captured.  Plaguelands (eight players, six closed) differed at the load too,
  four ways (its census at the start):
  - Each spell's listener re-arms as the mana is set, not only Hurl Boulder's, Thunder
    Clap's, and Slow's: ten more there (treants', satyrs', thunder lizards', murlocs'),
    and those that made none elsewhere were asleep.
  - A Rain of Fire (and a Blizzard) makes an in-range listener as its unit's owner's AI
    takes the unit, after the AI's objects (`FUN_6f9b9230` calling each ability's vf
    0xc8 through `FUN_6f67dff0` for an owner with a player AI; the classes'
    `FUN_6f606140`, event 0xd01bf at the ability's +0x50; read: the Doom Guards').
  - Permanent Immolation's buff comes with the unit's making; the port took a second as
    its first update applied it.
  - Not yet: a gold mine near a town site made before joins it, making no `AIts`
    (`FUN_6f9b7c10`, `FUN_6f9ba790`: the first site whose path query from it,
    `FUN_6f0594f0` to `FUN_6f059590`, costs under 70).  Its sixteenth mine, 61.2 cells
    from its fourth, made none; Ogre Mound's and Twisted Meadows' nearest pairs, 61.1
    and 65.3 cells apart, made two each, so the cost is the path's, not the distance.
    The port's own search (class 0, the mines' footprints set aside) finds no way for
    those two pairs, and 71.0 cells (center to center, 15 and 21 a step) for
    Plaguelands' joining pair, 76.8 and more for its others: near the threshold of 70
    if the query ends short of the mine, as walks to structures do.  Not yet read: the
    query's ends and units (`FUN_6f059590`).
  With the first three Plaguelands agrees from the second turn to turn 601 when the
  site's object is set aside (a local stand-in, `abtest/tmp_site.patch`).  There a
  Satyr Hellcaller's listener, due at 60.079998 on the clock, fired in the turn whose
  clock ends at 60.0804, where the port waited for the next: a listener falls due
  through the clock's last reading in a tick, the next tick's first (an Ogre Lord's due
  at 80.08, its tick's clock ending at 80.075, still the next).  With it, and the site
  set aside, Plaguelands agrees for all 3752 turns captured (kept in `docs/reference`).
- JASS handles, read in a save's handle table (`w3zobj handles`; a test map on
  Booty Bay, 1740 at five seconds): they start at 0x100001 with Blizzard.j's global
  initializers in order (seven timers and groups); a player's handle is made as a
  script first names it (Player(0) and Player(1) in the map's config, the neutral
  players as main does); the map's destructables follow the config; a unit takes its
  handle as a script first has it (a crypt the undead computer begins takes its own
  after a force the melee check makes), and the engine's own objects never (the
  hidden gold mine under a haunted mine, its circles, the creeps' aura art); a
  function's locals (not its parameters) still holding a handle as it returns keep
  it taken for good (the group MeleeFindNearestMine destroys).  The port does the
  same (`Handles::handle`, the natives' boundary in `World::call_native`); its
  handles agree with the original's for all but the last five, where the undead
  computer starts its crypt sooner in the port.
- Replays: .w3g files as the original writes them (header and its CRC, 8192-byte
  zlib blocks with their checksums, the record of host, players, slots, and seed, a
  turn per frame and its checksum, the commands as the original's actions,
  `actions::Codec`), and the original plays them back; a command with no action of the
  original's is not recorded (the interface gives none).  The original's own encodings measured so far
  (`docs/reference/bootybay_orders.w3g`): a selection change is `Select` (remove the
  old, add the new), `PreSubselection`, and `SelectSubgroup` by the first unit's type;
  a right-click is `OrderTarget` with order 851971 (smart), flags 0 on the ground with
  no object, 8 on a gold mine; training is `Order` with flags 0x42 and the unit type;
  object ids are (slot, birth), equal for the peasants at the start.
- Options: the original's registry keys and values (HKEY_CURRENT_USER\Software\Blizzard
  Entertainment\Warcraft III), every one of game.dll's settings table (0x6fa7efe8, 79
  rows of name, key, kind, and default; the keys Video, Sound, Gameplay, String, Misc,
  and Map at 0x6fa7efcc), which the original reads at start (`FUN_6f041cc0` with 0x50)
  and writes whole at exit and after its options menu's OK (`FUN_6f042620` with 0x50),
  the reference install's Wine registry holding all 79.  Kept in a registry file
  (REGEDIT4, `Warcraft III.reg` in the data folder) since the port has no Windows
  registry; those it has no use for go back as they came.  The Map key's six strings
  are the last map chosen on the Custom Game, LAN, and Battle.net screens per data
  version (`FUN_6f2bf310` with 0x42, 0x44, and 0x46 plus the version; `FUN_6f2b9ab0`
  stores the selection).
- Profiles: campaign progress, the menu's race, the tutorial, and the difficulty are in
  the original's `Campaigns.w3p` (`war3/src/w3p.rs`: `[Campaign]` for Reign of Chaos and
  `[Campaign1]` for The Frozen Throne, a 64-bit mask per campaign named as CampaignList
  has it, mission n at bit n, 63 available, 62 opening and 61 ending cinematic, 59 and
  60 the menu's; `Magic` and `Magic1` the CRC-32 of each section's masks; read and
  written byte for byte as the reference install's), the saved game caches in
  `Campaigns.w3v`.  Profiles are the folders in `Save` holding a Campaigns.w3p
  (`FUN_6f228e00`; made as the first free `Save\ProfileN`, `FUN_6f22b790`), named by
  their PlayerName, the chosen one in the registry's Misc\campaignprofile, and the
  first when it names none (`FUN_6f247560`).  -loadfile on a map or saved game chooses
  the World Editor's test profile, "WorldEdit" unless its "Test Map - Player Profile"
  setting says otherwise, making it when missing (`FUN_6f2e2850`).  A custom
  campaign's own file is `Save\ProfileN\<campaign>.w3p` (`FUN_6f22bec0` per profile and
  campaign, `FUN_6f2325c0` naming it from the profile's folder and the campaign's file
  name without its extension): in section `Version%06d` (`FUN_6f247390`, the
  war3campaign.w3f's campaign version, set by the Custom Campaign screen's OK,
  `FUN_6f2479d0`) the difficulty chosen there (`Difficulty`, `FUN_6f2490d0`) and each
  button SetCustomCampaignButtonVisible set (`Button%02d`, `FUN_6f24d3b0`; 0 when never
  set, `FUN_6f232970`), written at each change (`FUN_6f249250` to `FUN_6f247160`), with
  `[Mojo] Jojo` its checksum (`FUN_6f228bb0`: every other section's keys and values in
  order, each string's little-endian words and then its last bytes XORed in and rotated
  left 3, `FUN_6f24d3d0`, and the string's result XORed into the sum and rotated left
  3).  The native acts only while a custom campaign's archive is mounted
  (`FUN_6f219630`); a plain map's call changed nothing on disk or in the registry
  (measured).  The custom campaign screen shows a mission whose war3campaign.w3f word
  has bit 0 set or whose button is set (`FUN_6f308600`).
- Where files go (`FUN_6f2bfc90`): saves from the game menu and quick saves
  (`QuickSaveFile` of UI\MiscUI.txt [Misc] with ".w3z", `FUN_6f3c5a70`) in
  `Save\<profile folder>`, or `Save\WorldEdit` with no profile, network games'
  `Save\Multiplayer`; the scripts' saved games (SaveGame, LoadGame, SaveGameExists,
  CopySaveGame, RenameSaveDirectory, RemoveSaveDirectory, `FUN_6f1fffd0`) in
  `CustomSaves\<name>` below that, the name the custom campaign's file's or else the
  map's, without folders or extension (the reference install's
  `Save\Profile2\CustomSaves\fw10\fw10.w3z` for Maps\Test\fw10.w3x), or the profile's
  folder itself for a map from the game's own archives (game type bit 8,
  `FUN_6f03e1a0`); names with ".." or ":" refused, and no extension added (its
  `CustomSaves\mafsv\mafsv30` for SaveGame("mafsv30")).
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
  unnamed order 852462, the hero where it lies).  The objects' ids come from the
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

The LAN host example (`crates/net/examples/host.rs`) offers a two-slot melee lobby, or
with `W3HOSTMAP=1` the map's own: its w3i players and forces, users open and the host
in the first, computers in theirs, each slot's race, team, and color as the map sets
them (the original leaves a lobby whose slots disagree with the map's, reason 7).  The
`tags` example replays such a game's actions against the original's keepalives; with
`TAGS_MAPLOBBY=1` it builds the same lobby (the first user remote, the second local).
Its turns run from when every player has loaded, one each 100 ms of real time; a
backlog sent at once (as it did before) runs the game ahead of the clients' real-time
clocks, on which a trigger's sleep ends (its CNetCommandResumeTriggerExec then sent).
With `W3HOSTACTIONS=<file>` (lines of "turn hex") it sends actions as its own player's,
for the original to carry out as a player's.
