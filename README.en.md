# SkateGTA

**English** · [Русский](README.md)

Skate 3 physics and gameplay inside GTA V Legacy through ScriptHookV. The mod uses [Skate 3 Rust Engine](https://github.com/SK8-ENGINE/skate-3-rust-engine) and your own converted Skate 3 files. Game data and assets are not included in this repository.

## v10 update

- Grind discovery follows player travel, including sideways slides and riding fakie. At rest, it uses the player's facing direction while walking or the board direction while riding.
- Ledge discovery prioritizes the corridor up to 10 metres ahead. Handrail rays point forward and to either side of travel; a detected rail is traced forward first.
- Newly discovered sections are added during grinds and flight while preserving published segments, owners and GUIDs.
- Straight rails no longer produce hundreds of redundant collision sections. Curves retain a maximum simplification error of 5 mm.
- Inclined rails, thin fence tops and curved ledge caps are supported. Bowl entry and continuous uphill transitions retain their fixes; curbs require a jump.

Discovery is spread across frames with a limited ray budget. A 10-metre range does not mean every object is detected instantly, and discovery depends on GTA collision being available. These rays cannot detect a purely visual rail that has no GTA collision.

## Features

Full mode (`Gameplay = full`) includes riding, pushing, flick-it, ollies, manuals, grabs, grinds, walking with the board, the camera, character animation and Skate 3's original trick HUD. Physical parameters come from your data. `Gameplay = board` provides separate board physics with test controls.

Full gameplay checks contacts using actual GTA shape-test rays. Confirmed handrails also receive a narrow contact mesh. Whole-object bounding boxes and the former red collision cubes are not added. Skate 3's engine solves board and skater physics.

## Installation

1. Install ScriptHookV for GTA V **Legacy**.
2. Convert your own Skate 3 files using the [skate3rust installer](https://github.com/SK8-ENGINE/skate-3-rust-engine/releases/tag/experimental): select `default.xex` with its adjacent `data` folder. Converted files appear in `data/installations/<id>/assets`.
3. Fully close GTA V. Place `SkateGTA.asi` and `SkateGTA.ini` alongside `GTA5.exe`, `ScriptHookV.dll` and `dinput8.dll`.
4. Set `Skate3RustDir` in the INI to the directory containing `skate3rust.exe`, or set `AssetRoot` directly to the `assets` directory.
5. Start the game and press **F5** or controller **Back**.

When updating, replace the ASI with GTA closed and keep your existing INI. Missing data is reported in `SkateGTA.log`.

Full gameplay needs collections, physics skeletons, OnBoard/OffBoard animations, action and motion graphs, the camera graph and joystick patterns in `assets/private/stock`. The log lists missing files.

The board model (`BoardModel = skate3`) comes from `assets/private/skater.glb`. A GTA object or `none` for line rendering can be selected instead. The original HUD uses `assets/private/hud`; gameplay continues without the HUD if its files are absent. Audio (`Audio = 1`, `AudioVolume`) uses `assets/private/audio`, AEMS projects, banks and WAV files. Missing audio banks remain silent and are listed in the log.

To uninstall, close GTA V and remove `SkateGTA.asi`.

## Controls

Skate 3 controller layout:

| Action | Button |
|---|---|
| Toggle the mod | F5 or Back |
| Push with right / left foot | A / X |
| Steer and lean | Left stick |
| Ollie and flick-it tricks | Right stick: down, then flick up |
| Manual | Slightly move right stick up / down |
| Brake / bail in the air | B |
| Grabs and crouch | LT / RT |
| Handplant / grab a ledge | RB |
| Get off / back onto the board | Y |

## Settings and diagnostics

- `SkateCamera = 0` restores the GTA camera.
- `PedPose = 1` transfers the Skate 3 pose to the GTA character. `PedPoseMode = hook` intercepts bone-matrix copying; `script` writes the pose from the script, while the slower `guard` mode is diagnostic.
- `DebugDraw = 1` shows yellow grind lines and a green ground mesh. `DebugBody = 1` shows the skeleton. Debug rendering can be disabled for normal play.
- `CollideWalls` controls the former box system and does not apply to full gameplay. `CollideEntities` controls pedestrian impact reactions.
- `Performance v10` log entries report discovery and physics time. The log also includes discovered-line counts and startup details.

## Verification and limitations

v10 passed 75 selected checks: 62 library tests, 4 curb tests, 2 grind tests, 4 handrail tests, 2 uphill/bowl transition tests and a streaming-provider test. Coverage includes discovery at 9.5 metres, travel direction with a rotated board, sloped rails and contact preservation when extending a line. Native grinding lasts 181 consecutive frames in the test scene.

These checks use gameplay code and synthetic surface probes, including your converted data. They do not establish correctness at every GTA location or a particular frame rate. Sparse probes can miss geometry. Wall impacts do not always trigger a bail; the skater may stop instead. Character animation and grind discovery remain under development.

## Building from source

The verified build used Rust 1.99.0, Windows x64 and MSVC Build Tools. Skate 3 Engine dependencies are pinned to `b3c967932a91d0db74275d14064bb6bdef8608af`. GitHub access is required to fetch them.

```powershell
cargo build --release -p skate-gta -j 1
Copy-Item target/release/skate_gta.dll SkateGTA.asi
```

To reduce memory use while compiling gameplay:

```powershell
cargo build --release -p skate-gta -j 1 --config 'profile.release.package.skate-gameplay.debug=0'
```

Selected checks using your own converted files:

```powershell
$env:SKATE_GTA_ASSETS = 'D:/skate3rust/data/installations/<id>/assets'
cargo test -p skate-gta --lib --test curb --test grind --test handrail --test transition -j 1 --config 'profile.dev.package.skate-gameplay.debug=0'
cargo test -p skate-gameplay --test streaming_provider -j 1 --config 'profile.dev.package.skate-gameplay.debug=0'
```

Without `SKATE_GTA_ASSETS`, tests that require private data are skipped. The complete `skate-gameplay` library suite requires upstream test files absent from this copy; the commands above were verified separately.

## Credits

Built on Skate 3 Rust Engine and loaded into GTA V through ScriptHookV. Changes were prepared with assistance from Claude and OpenAI Codex (GPT-6), with the tests listed above used for verification.

## License

GPL-3.0-only, matching Skate 3 Rust Engine. Electronic Arts owns the Skate 3 data and assets. The mod does not include or redistribute them.
