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
