//! SkateGTA.ini, a flat `key = value` file next to the .asi.
use crate::{sim::TestControlTuning, terrain::PatchSettings};
use std::path::PathBuf;

/// How the Skate 3 pose reaches the ped's bones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseMode {
    /// Right after GTA copies the bones, through a hook on its memcpy.
    Hook,
    /// Right after GTA's bone write, noticed by guard pages (slow, logs who writes).
    Guard,
    /// From the script only; GTA overwrites it.
    Script,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub skate3rust_dir: Option<PathBuf>,
    pub asset_root: Option<PathBuf>,
    pub toggle_key: u32,
    pub board_model: String,
    pub model_yaw_offset: f32,
    pub model_z_offset: f32,
    pub ped_z_offset: f32,
    pub ped_yaw_offset: f32,
    pub probe_flags: i32,
    pub debug_draw: bool,
    pub full_gameplay: bool,
    pub mode: skate_gameplay::host::Mode,
    pub skate_camera: bool,
    pub low_camera: bool,
    pub debug_body: bool,
    pub ped_pose: bool,
    pub ped_freeze: bool,
    pub hud: bool,
    pub grind_edges: bool,
    pub collide_walls: bool,
    pub collide_entities: bool,
    pub pose_mode: PoseMode,
    pub tuning: TestControlTuning,
    pub patch: PatchSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            skate3rust_dir: None,
            asset_root: None,
            toggle_key: 0x74, // F5
            board_model: "skate3".into(),
            model_yaw_offset: 0.0,
            model_z_offset: 0.0,
            ped_z_offset: 1.0,
            ped_yaw_offset: 0.0,
            probe_flags: 1,
            debug_draw: true,
            full_gameplay: true,
            mode: skate_gameplay::host::Mode::Easy,
            skate_camera: true,
            low_camera: false,
            debug_body: false,
            ped_pose: true,
            ped_freeze: true,
            hud: true,
            grind_edges: true,
            collide_walls: true,
            collide_entities: true,
            pose_mode: PoseMode::Hook,
            tuning: TestControlTuning::default(),
            patch: PatchSettings::default(),
        }
    }
}

pub const TEMPLATE: &str = "\
; SkateGTA - Skate 3 Rust Engine physics in GTA V (ScriptHookV)
; Folder that contains skate3rust.exe. Run it once and pick your default.xex first.
Skate3RustDir = C:\\Games\\skate3rust
; Or point directly at the converted assets folder (overrides Skate3RustDir):
; AssetRoot = C:\\Games\\skate3rust\\data\\installations\\<id>\\assets

; Virtual-key code to get on/off the board (0x74 = F5)
ToggleKey = 0x74
; Board model: skate3 = the Skate 3 board from your skater.glb (deck graphic,
; trucks, spinning wheels), none = lines, or the name of a GTA object model.
BoardModel = skate3
ModelYawOffset = 0
ModelZOffset = 0
PedZOffset = 1.0
; Extra turn (degrees) of the Skate 3 pose on the ped, e.g. 90, 180, 270
PedYawOffset = 0
; Draw the solver's deck, wheels and the sampled ground patch
DebugDraw = 1

; full = complete Skate 3 gameplay (needs all converted files),
; board = board physics only (needs skater-collections.json)
Gameplay = full
; Stock physics_mode: easy, normal or hardcore
Difficulty = easy
; Use the Skate 3 gameplay camera instead of the GTA camera
SkateCamera = 1
; Skate 3 Camera Angle: high or low
CameraAngle = high
; Pose the GTA player with the Skate 3 skater's animation
PedPose = 1
; Also draw the Skate 3 skeleton and board as lines
DebugBody = 0
; 1 = freeze the ped in place; 0 = keep it live with gravity and collision off
PedFreeze = 1
; How the pose reaches the ped: hook = written right after GTA copies the
; bones (a hook on GTA's memcpy); guard = the same noticed with guard pages
; (slow, logs which GTA code touches the bones); script = from the script
; only (GTA overwrites it)
PedPoseMode = hook
; Walls and building sides (sideways rays) are solid for the skater
CollideWalls = 1
; Vehicles, props and pedestrians are solid; pedestrians you hit fall over
CollideEntities = 1
; Find curbs, ledges and step edges to grind (yellow lines with DebugDraw)
GrindEdges = 1
; The original Skate 3 trick and score HUD (assets\\private\\hud)
Hud = 1

; Board-only mode test controls
PushForce = 40
SteerAngle = 0.3
InvertSteer = 0

; Ground sampling around the board: 1 = map only, 17 = map + objects
; (objects include street litter, which turns into bumps under the wheels)
ProbeFlags = 1
PatchCells = 8
PatchSpacing = 0.4
";

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut c = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("SkateGTA.ini line {}: expected key = value", number + 1))?;
            let (key, value) = (key.trim(), value.trim());
            let err = |what: &str| format!("SkateGTA.ini line {}: {key} {what}", number + 1);
            let float = || value.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| err("must be a number"));
            let flag = || match value {
                "1" | "true" | "yes" => Ok(true),
                "0" | "false" | "no" => Ok(false),
                _ => Err(err("must be 0 or 1")),
            };
            match key.to_ascii_lowercase().as_str() {
                "skate3rustdir" => c.skate3rust_dir = Some(PathBuf::from(value)),
                "assetroot" => c.asset_root = Some(PathBuf::from(value)),
                "togglekey" => c.toggle_key = parse_int(value).ok_or_else(|| err("must be an integer"))? as u32,
                "boardmodel" => c.board_model = value.to_string(),
                "modelyawoffset" => c.model_yaw_offset = float()?,
                "modelzoffset" => c.model_z_offset = float()?,
                "pedzoffset" => c.ped_z_offset = float()?,
                "pedyawoffset" => c.ped_yaw_offset = float()?,
                "debugdraw" => c.debug_draw = flag()?,
                "gameplay" => c.full_gameplay = match value.to_ascii_lowercase().as_str() {
                    "full" => true,
                    "board" => false,
                    _ => return Err(err("must be full or board")),
                },
                "difficulty" => c.mode = skate_gameplay::host::Mode::parse(value)
                    .ok_or_else(|| err("must be easy, normal or hardcore"))?,
                "skatecamera" => c.skate_camera = flag()?,
                "cameraangle" => c.low_camera = match value.to_ascii_lowercase().as_str() {
                    "low" => true,
                    "high" => false,
                    _ => return Err(err("must be high or low")),
                },
                "debugbody" => c.debug_body = flag()?,
                "pedpose" => c.ped_pose = flag()?,
                "pedfreeze" => c.ped_freeze = flag()?,
                "hud" => c.hud = flag()?,
                "grindedges" => c.grind_edges = flag()?,
                "collidewalls" => c.collide_walls = flag()?,
                "collideentities" => c.collide_entities = flag()?,
                "pedposemode" => c.pose_mode = match value.to_ascii_lowercase().as_str() {
                    "hook" => PoseMode::Hook,
                    "guard" => PoseMode::Guard,
                    "script" => PoseMode::Script,
                    _ => return Err(err("must be hook, guard or script")),
                },
                "pushforce" => c.tuning.push_force = float()?,
                "steerangle" => c.tuning.steer_angle = float()?,
                "invertsteer" => c.tuning.invert_steer = flag()?,
                "probeflags" => c.probe_flags = parse_int(value).ok_or_else(|| err("must be an integer"))? as i32,
                "patchcells" => {
                    let n = parse_int(value).filter(|n| (2..=32).contains(n)).ok_or_else(|| err("must be 2..32"))?;
                    c.patch.cells = n as usize;
                }
                "patchspacing" => c.patch.spacing = float()?.clamp(0.05, 2.0),
                _ => return Err(err("is not a known setting")),
            }
        }
        Ok(c)
    }
}

fn parse_int(value: &str) -> Option<i64> {
    match value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => value.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let c = Config::parse(TEMPLATE).unwrap();
        assert_eq!(c.toggle_key, 0x74);
        assert_eq!(c.probe_flags, 1);
        assert!(c.skate3rust_dir.is_some() && c.asset_root.is_none());
    }

    #[test]
    fn unknown_keys_are_reported() {
        assert!(Config::parse("Bogus = 1").unwrap_err().contains("Bogus"));
        assert!(Config::parse("PushForce = abc").is_err());
    }
}
